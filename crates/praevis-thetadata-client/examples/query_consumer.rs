//! Runnable consumer patterns for `query()` and `job()`: partial results and
//! cancellation (beta.1 checklist).
//!
//! It runs against a synthetic loopback server: no credentials and no
//! ThetaData traffic. It shows how to:
//! - **keep partial results:** process each batch as it arrives, keep what was
//!   delivered when a later batch fails, and call the rows complete only after
//!   a clean end of stream (`Ok(None)`);
//! - **cancel mid-job:** drop a `JobStream` (or a `QueryStream`) to stop it.
//!   Its in-flight requests are cancelled and their scheduler slots released,
//!   so the next request starts at once. Cancellation is local: it does not
//!   prove the service stopped its work at the same instant.
//!
//! ```text
//! cargo run -p praevis-thetadata-client --example query_consumer
//! ```
#![allow(dead_code)] // The shared support module also serves the tests and benchmark.

use praevis_thetadata_client::{
    ClientConfig, NaiveDate, QueryError, RequestOptions, SchedulerPolicy, Session, ThetaClient,
    endpoints::StockHistoryEod,
};
use std::time::{Duration, Instant};

#[path = "../tests/support/mod.rs"]
mod support;

/// What the caller may conclude from one request.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Clean end of stream: these rows are the complete result.
    Complete { rows: usize },
    /// The stream failed after delivering `rows`. They are valid, not complete.
    Partial { rows: usize, error: QueryError },
}

/// Partial results: consume batch by batch. `collect()` is the all-or-error
/// alternative; it drops the delivered rows when the stream fails.
pub async fn consume(
    client: &ThetaClient,
    request: &StockHistoryEod,
) -> Result<Outcome, QueryError> {
    let mut stream = client.query(request).await?;
    let mut rows = 0;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => rows += batch.num_rows(), // process the batch here
            Ok(None) => return Ok(Outcome::Complete { rows }),
            Err(error) => return Ok(Outcome::Partial { rows, error }),
        }
    }
}

/// Cancellation: read the first batch of a long job, then stop. Dropping the
/// job cancels its open parts and frees their slots. Returns the rows read
/// before cancelling and how long the next request waited for a slot.
pub async fn cancel_midway(
    client: &ThetaClient,
    long: &StockHistoryEod,
    next: &StockHistoryEod,
) -> Result<(usize, Duration), QueryError> {
    let mut job = client.job(long).await?;
    let first = job.next_batch().await?.map_or(0, |b| b.num_rows());
    drop(job); // the cancellation
    let started = Instant::now();
    // The timeout includes time queued for a slot: if the job still held it,
    // this would fail with `Deadline` after 5 s instead of waiting.
    let options = RequestOptions::default().with_timeout(Some(Duration::from_secs(5)));
    let mut stream = client.query_with(next, options).await?;
    let waited = started.elapsed();
    while stream.next_batch().await?.is_some() {}
    Ok((first, waited))
}

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

async fn client_for(fixture: &support::Fixture, session: Session) -> ThetaClient {
    ThetaClient::with_session(
        ClientConfig::default()
            .with_endpoint(Some(fixture.endpoint.clone()))
            // Loopback synthetic server only.
            .with_allow_insecure(true),
        session,
    )
    .await
    .unwrap()
    // One request at a time, so a request that is not cancelled would block
    // the next one.
    .with_scheduler(
        SchedulerPolicy::default()
            .with_concurrency(1)
            .with_per_minute(None),
    )
    .unwrap()
}

/// Two batches of two rows, then a mid-stream failure.
fn failing_script() -> support::Script {
    let (batch, _) = support::table(support::Shape::Prices, 2);
    support::Script {
        messages: vec![
            Ok(support::response(&batch, false)),
            Ok(support::response(&batch, true)),
            Err(tonic::Status::internal("synthetic mid-stream failure")),
        ],
        ..Default::default()
    }
}

/// Every request streams 20 slow batches (about 4 s each request).
fn slow_script() -> support::Script {
    let (batch, _) = support::table(support::Shape::Prices, 3);
    support::Script {
        messages: vec![Ok(support::response(&batch, false)); 20],
        message_delay: Duration::from_millis(200),
        ..Default::default()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // A real application authenticates with praevis-thetadata-auth and passes
    // the session.
    let fixture = support::Fixture::start(failing_script());
    let client = client_for(&fixture, support::session().await).await;
    let one_day = StockHistoryEod::new("SYNTHETIC", day("2026-01-15"), day("2026-01-15"));
    match consume(&client, &one_day).await {
        Ok(Outcome::Complete { rows }) => println!("complete: {rows} rows"),
        Ok(Outcome::Partial { rows, error }) => {
            println!(
                "partial: kept {rows} rows delivered before the stream failed ({error}); not complete"
            )
        }
        Err(error) => println!("the request did not start: {error}"),
    }

    let fixture = support::Fixture::start(slow_script());
    let client = client_for(&fixture, support::session().await).await;
    let long = StockHistoryEod::new("SYNTHETIC", day("2020-01-01"), day("2025-12-31"));
    match cancel_midway(&client, &long, &one_day).await {
        Ok((rows, waited)) => println!(
            "cancelled a job after its first batch ({rows} rows); the next request got its slot after {} ms",
            waited.as_millis()
        ),
        Err(error) => println!("failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_failed_stream_keeps_its_rows_as_partial() {
        let fixture = support::Fixture::start(failing_script());
        let client = client_for(&fixture, support::session().await).await;
        let request = StockHistoryEod::new("SYNTHETIC", day("2026-01-15"), day("2026-01-15"));
        assert_eq!(
            consume(&client, &request).await.unwrap(),
            Outcome::Partial {
                rows: 4,
                error: QueryError::Remote(tonic::Code::Internal)
            }
        );
        // collect() is all-or-error: the same failure, and no rows.
        let collected = client.query(&request).await.unwrap().collect().await;
        assert_eq!(
            collected.err(),
            Some(QueryError::Remote(tonic::Code::Internal))
        );
    }

    #[tokio::test]
    async fn dropping_a_job_releases_its_slot() {
        let fixture = support::Fixture::start(slow_script());
        let client = client_for(&fixture, support::session().await).await;
        let long = StockHistoryEod::new("SYNTHETIC", day("2020-01-01"), day("2025-12-31"));
        let next = StockHistoryEod::new("SYNTHETIC", day("2026-01-15"), day("2026-01-15"));
        let (rows, waited) = cancel_midway(&client, &long, &next).await.unwrap();
        assert_eq!(rows, 3);
        // Without the cancellation the next request would wait for the job's
        // part to finish its remaining 19 slow batches (about 3.8 s).
        assert!(waited < Duration::from_secs(1), "waited {waited:?}");
        // The job planned several parts but sent only the first.
        assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    }
}
