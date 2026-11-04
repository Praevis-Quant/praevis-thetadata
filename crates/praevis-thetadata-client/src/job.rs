//! Planned jobs (P1 PR 4b): one typed request split by date into requests
//! within the vendor's limits, run through the client's scheduler and merged
//! back, in date order, into one stream of Arrow batches.
use crate::ThetaClient;
use crate::endpoints::{Endpoint, Split};
use crate::eod::QueryError;
use crate::query::BatchPolicy;
use chrono::Days;
use praevis_thetadata_arrow::arrow_array::{ArrayRef, RecordBatch, new_null_array};
use praevis_thetadata_arrow::arrow_schema::{DataType, Field, Schema};
use std::{collections::VecDeque, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};

/// Split `request` by its date range according to its [`Split`] rule, in
/// date order. A request without a range, or with an inverted one, is sent
/// whole (the service judges it).
pub fn plan<E: Endpoint + Clone>(request: &E) -> Vec<E> {
    let (Split::Days(days), Some((start, end))) = (E::SPLIT, request.date_range()) else {
        return vec![request.clone()];
    };
    if start > end || days == 0 {
        return vec![request.clone()];
    }
    let mut out = Vec::new();
    let mut from = start;
    loop {
        let to = from
            .checked_add_days(Days::new(u64::from(days) - 1))
            .map_or(end, |d| d.min(end));
        match request.with_date_range(from, to) {
            Some(part) => out.push(part),
            None => return vec![request.clone()],
        }
        match to.checked_add_days(Days::new(1)) {
            Some(next) if to < end => from = next,
            _ => return out,
        }
    }
}

type Shard = mpsc::Receiver<Result<RecordBatch, QueryError>>;

impl ThetaClient {
    /// Run `request` as a planned job: split by date (EOD and at-time at 366
    /// days, intraday history per day), each part a scheduled `query()`, with
    /// at most the scheduler's concurrency in flight, merged in date order.
    ///
    /// A part that finds no data (`NOT_FOUND`, for example a holiday) is
    /// skipped; if every part does, the job reports `NoData`. Any other error
    /// ends the job after the rows before it. Dropping or cancelling the job
    /// cancels its queued and running requests.
    pub async fn job<E>(&self, request: &E) -> Result<JobStream, QueryError>
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        self.job_with(request, BatchPolicy::default()).await
    }

    /// [`ThetaClient::job`] with an explicit batch policy for every part.
    pub async fn job_with<E>(
        &self,
        request: &E,
        batch: BatchPolicy,
    ) -> Result<JobStream, QueryError>
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        let parts: VecDeque<E> = plan(request).into();
        let ahead = self.scheduler.policy.concurrency;
        let client = self.clone();
        let mut start = Box::new(move |part: E| -> (Shard, JoinHandle<()>) {
            let (tx, rx) = mpsc::channel(2);
            let client = client.clone();
            let batch = batch.clone();
            let task = tokio::spawn(async move {
                let mut stream = match client.query_with(&part, batch).await {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = tx.send(Err(error)).await;
                        return;
                    }
                };
                loop {
                    let next = stream.next_batch().await;
                    let end = !matches!(next, Ok(Some(_)));
                    let item = match next {
                        Ok(Some(batch)) => Ok(batch),
                        Ok(None) => return,
                        Err(error) => Err(error),
                    };
                    // A dropped job closes the channel and ends this request.
                    if tx.send(item).await.is_err() || end {
                        return;
                    }
                }
            });
            (rx, task)
        }) as Box<dyn FnMut(E) -> (Shard, JoinHandle<()>) + Send>;
        let total = parts.len();
        let mut parts = parts;
        let mut running = VecDeque::new();
        while running.len() < ahead {
            let Some(part) = parts.pop_front() else { break };
            running.push_back(start(part));
        }
        Ok(JobStream {
            next: Box::new(move || parts.pop_front().map(&mut start)),
            running,
            total,
            empty_parts: 0,
            names: None,
            types: Vec::new(),
            rows: 0,
            terminal: false,
        })
    }
}

/// The merged, date-ordered stream of a planned job.
pub struct JobStream {
    /// Starts the next part, if any remain.
    next: Box<dyn FnMut() -> Option<(Shard, JoinHandle<()>)> + Send>,
    running: VecDeque<(Shard, JoinHandle<()>)>,
    total: usize,
    empty_parts: usize,
    names: Option<Vec<String>>,
    /// The type of each column so far; `Null` until a part types it.
    types: Vec<DataType>,
    rows: usize,
    terminal: bool,
}

impl JobStream {
    /// Requests in the plan.
    pub fn parts(&self) -> usize {
        self.total
    }

    /// Cancel every queued and running request of the job.
    pub fn cancel(&mut self) {
        for (_, task) in self.running.drain(..) {
            task.abort();
        }
        self.next = Box::new(|| None);
        self.terminal = true;
    }

    /// The next batch in date order, `Ok(None)` at the end, or a terminal
    /// error returned once (after the rows before it).
    pub async fn next_batch(&mut self) -> Result<Option<RecordBatch>, QueryError> {
        while !self.terminal {
            let Some((head, _)) = self.running.front_mut() else {
                self.terminal = true;
                if self.rows == 0 && self.total > 0 && self.empty_parts == self.total {
                    return Err(QueryError::NoData);
                }
                return Ok(None);
            };
            match head.recv().await {
                Some(Ok(batch)) => match self.conform(batch) {
                    Ok(batch) => {
                        self.rows += batch.num_rows();
                        return Ok(Some(batch));
                    }
                    Err(error) => {
                        self.cancel();
                        return Err(error);
                    }
                },
                Some(Err(QueryError::NoData)) => {
                    self.empty_parts += 1;
                    self.advance();
                }
                Some(Err(error)) => {
                    self.cancel();
                    return Err(error);
                }
                None => self.advance(),
            }
        }
        Ok(None)
    }

    /// The head part is done: start the next one.
    fn advance(&mut self) {
        self.running.pop_front();
        if let Some(part) = (self.next)() {
            self.running.push_back(part);
        }
    }

    /// Every part must have the same columns. A column typed by an earlier
    /// part keeps that type when a later part has no value for it (`Null`);
    /// otherwise the only change is `Null` becoming typed.
    fn conform(&mut self, batch: RecordBatch) -> Result<RecordBatch, QueryError> {
        let names: Vec<String> = batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        match &self.names {
            None => {
                self.types = vec![DataType::Null; names.len()];
                self.names = Some(names);
            }
            Some(known) if *known != names => return Err(QueryError::Schema),
            Some(_) => {}
        }
        let mut changed = false;
        let mut columns: Vec<ArrayRef> = Vec::with_capacity(self.types.len());
        for (column, known) in batch.columns().iter().zip(self.types.iter_mut()) {
            match (column.data_type(), &*known) {
                (DataType::Null, DataType::Null) => columns.push(column.clone()),
                (DataType::Null, typed) => {
                    columns.push(new_null_array(typed, column.len()));
                    changed = true;
                }
                (typed, DataType::Null) => {
                    *known = typed.clone();
                    columns.push(column.clone());
                }
                (typed, expected) if typed == expected => columns.push(column.clone()),
                _ => return Err(QueryError::Schema),
            }
        }
        if !changed {
            return Ok(batch);
        }
        let names = self.names.as_ref().expect("set above");
        let schema = Arc::new(Schema::new(
            names
                .iter()
                .zip(&self.types)
                .map(|(name, data_type)| Field::new(name, data_type.clone(), true))
                .collect::<Vec<_>>(),
        ));
        Ok(RecordBatch::try_new(schema, columns).expect("retyped null columns only"))
    }

    /// Read the whole job; `Null` columns get their final type.
    pub async fn collect(mut self) -> Result<Vec<RecordBatch>, QueryError> {
        let mut batches = Vec::new();
        while let Some(batch) = self.next_batch().await? {
            batches.push(batch);
        }
        Ok(crate::query::unify(batches))
    }
}

impl Drop for JobStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praevis_thetadata_arrow::arrow_array::Int64Array;

    fn batch(values: Option<i64>) -> RecordBatch {
        let column: ArrayRef = match values {
            Some(v) => Arc::new(Int64Array::from(vec![v])),
            None => new_null_array(&DataType::Null, 1),
        };
        RecordBatch::try_from_iter([("a", column)]).unwrap()
    }
    fn job() -> JobStream {
        JobStream {
            next: Box::new(|| None),
            running: VecDeque::new(),
            total: 0,
            empty_parts: 0,
            names: None,
            types: Vec::new(),
            rows: 0,
            terminal: false,
        }
    }

    #[test]
    fn parts_keep_earlier_types_and_reject_other_changes() {
        let mut job = job();
        // Null first, then typed: allowed.
        assert_eq!(
            job.conform(batch(None)).unwrap().column(0).data_type(),
            &DataType::Null
        );
        assert_eq!(
            job.conform(batch(Some(1))).unwrap().column(0).data_type(),
            &DataType::Int64
        );
        // A later part with no value keeps the earlier type.
        let later = job.conform(batch(None)).unwrap();
        assert_eq!(later.column(0).data_type(), &DataType::Int64);
        assert!(later.column(0).is_null(0));
        // Another type, or other columns, are schema errors.
        let text: ArrayRef = Arc::new(praevis_thetadata_arrow::arrow_array::StringArray::from(
            vec!["x"],
        ));
        let other = RecordBatch::try_from_iter([("a", text)]).unwrap();
        assert_eq!(job.conform(other).err(), Some(QueryError::Schema));
        let renamed =
            RecordBatch::try_from_iter([("b", Arc::new(Int64Array::from(vec![1])) as ArrayRef)])
                .unwrap();
        assert_eq!(job.conform(renamed).err(), Some(QueryError::Schema));
    }
}
