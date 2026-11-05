//! Planned jobs (P1 PR 4b): one typed request split by date into requests
//! within the vendor's limits, run through the client's scheduler and merged
//! back, in date order, into one stream of Arrow batches.
//!
//! Options phase: an all-strike tick or 1 s option quote request is also
//! split per listed strike (see [`ThetaClient::job`]).
use crate::ThetaClient;
use crate::endpoints::{Endpoint, Expiration, OptionHistoryQuote, OptionListStrikes, Split};
use crate::eod::QueryError;
use crate::query::BatchPolicy;
use chrono::{Days, NaiveDate};
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

/// Whether an option quote request is the all-strike, fine-grained shape the
/// service is slow to answer whole: every strike (`*`), a `tick` or `1s`
/// interval (`1s` is the vendor default), one concrete expiration, and no
/// `strike_range` or `max_dte`. Measured (`opt-sizes-002`, `opt-split-001`):
/// such a request waits about 60 s before any data, while the same request
/// per strike, 8 at a time, returned identical rows 6.4-7.6x sooner. Other
/// intervals were not slow (1-minute) or are unmeasured (sub-second, 5-30 s),
/// so they are not split.
fn strike_split_expiration(request: &OptionHistoryQuote) -> Option<NaiveDate> {
    let interval = request.interval.as_deref().unwrap_or("1s");
    let all_strikes = request.strike.as_deref().is_none_or(|s| s == "*");
    if !matches!(interval, "tick" | "1s")
        || !all_strikes
        || request.strike_range.is_some()
        || request.max_dte.is_some()
    {
        return None;
    }
    match &request.expiration {
        Expiration::Date(date) => Some(*date),
        Expiration::Text(text) => NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(text, "%Y%m%d"))
            .ok(),
    }
}

/// A decimal strike as request text in dollars, with at least two decimals
/// (`542.00`, `542.50`, `0.125`), as the vendor documents strikes.
fn strike_text(value: i128, scale: u32) -> String {
    let unit = 10i128.pow(scale);
    let (whole, mut frac) = (value / unit, (value % unit).abs());
    let mut digits = scale;
    while digits > 2 && frac % 10 == 0 {
        frac /= 10;
        digits -= 1;
    }
    if digits < 2 {
        frac *= 10i128.pow(2 - digits);
        digits = 2;
    }
    format!("{whole}.{frac:0width$}", width = digits as usize)
}

impl ThetaClient {
    /// The strikes listed for `symbol` and `expiration`, ascending by value,
    /// as request text. One `option_list_strikes` request. `None` if the
    /// service lists none or the strikes are not decimals.
    async fn listed_strikes(
        &self,
        symbol: &str,
        expiration: NaiveDate,
    ) -> Result<Option<Vec<String>>, QueryError> {
        use praevis_thetadata_arrow::arrow_array::Array;
        use praevis_thetadata_arrow::arrow_array::cast::AsArray;
        use praevis_thetadata_arrow::arrow_array::types::Decimal128Type;
        let batches = match self
            .query(&OptionListStrikes::new(symbol, expiration))
            .await
        {
            Ok(stream) => stream.collect().await,
            Err(error) => Err(error),
        };
        let batches = match batches {
            Ok(batches) => batches,
            Err(QueryError::NoData) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut values = Vec::new();
        let mut scale = None;
        for batch in &batches {
            let Some(column) = batch.column_by_name("strike") else {
                return Ok(None);
            };
            let DataType::Decimal128(_, s) = column.data_type() else {
                return Ok(None);
            };
            if *scale.get_or_insert(*s) != *s || column.null_count() > 0 {
                return Ok(None);
            }
            values.extend(
                column
                    .as_primitive::<Decimal128Type>()
                    .values()
                    .iter()
                    .copied(),
            );
        }
        values.sort_unstable();
        values.dedup();
        let scale = scale.map_or(0, |s| s.max(0) as u32);
        Ok((!values.is_empty())
            .then(|| values.into_iter().map(|v| strike_text(v, scale)).collect()))
    }

    /// The parts of `request`: its date split, and for the all-strike fine
    /// quote shape each date part once per listed strike (one extra list
    /// request), in date then ascending strike order.
    async fn job_parts<E>(&self, request: &E) -> Result<Vec<E>, QueryError>
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        use std::any::Any;
        let by_date = plan(request);
        let Some(quote) = (request as &dyn Any).downcast_ref::<OptionHistoryQuote>() else {
            return Ok(by_date);
        };
        // One at a time, the split is slower than the single request (95.9 s
        // against about 60 s in opt-split-001): only split when requests
        // can overlap.
        let Some(expiration) =
            strike_split_expiration(quote).filter(|_| self.scheduler.policy.concurrency >= 2)
        else {
            return Ok(by_date);
        };
        let Some(strikes) = self.listed_strikes(&quote.symbol, expiration).await? else {
            return Ok(by_date);
        };
        let mut parts = Vec::with_capacity(by_date.len() * strikes.len());
        for part in by_date {
            let Some(part) = (&part as &dyn Any).downcast_ref::<OptionHistoryQuote>() else {
                return Ok(vec![request.clone()]);
            };
            for strike in &strikes {
                let mut one = part.clone();
                one.strike = Some(strike.clone());
                let boxed: Box<dyn Any> = Box::new(one);
                match boxed.downcast::<E>() {
                    Ok(one) => parts.push(*one),
                    Err(_) => return Ok(vec![request.clone()]),
                }
            }
        }
        Ok(parts)
    }
}

type Shard = mpsc::Receiver<Result<RecordBatch, QueryError>>;

impl ThetaClient {
    /// Run `request` as a planned job: split by date (EOD and at-time at 366
    /// days, intraday history per day), each part a scheduled `query()`, with
    /// at most the scheduler's concurrency in flight, merged in date order.
    ///
    /// **All-strike tick and 1 s option quotes are also split per strike.**
    /// An `option_history_quote` request for every strike (`*`) at a `tick`
    /// or `1s` interval, with one concrete expiration and no `strike_range`
    /// or `max_dte`, first lists the expiration's strikes (one extra
    /// `option_list_strikes` request), then sends each date part once per
    /// strike, when the scheduler runs at least 2 requests at once (one at a
    /// time, the split is slower than the single request). The rows are the
    /// same as the single request's (verified exactly, `opt-split-001`). The order is date, then strike ascending,
    /// then the service's order within a strike; the single request's own
    /// order is not documented. If the list is empty, the request is sent
    /// unsplit. `query()` never splits.
    ///
    /// A part that finds no data (`NOT_FOUND`, for example a holiday, or a
    /// strike without quotes) is skipped; if every part does, the job
    /// reports `NoData`. Any other error
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
        let parts: VecDeque<E> = self.job_parts(request).await?.into();
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
