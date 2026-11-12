//! Runnable consumer patterns for `query()` and `job()`: partial results and
//! cancellation (beta.1 checklist).
//!
//! It runs against a synthetic loopback server: no credentials and no
//! ThetaData traffic. It shows how to:
//! - **keep partial results:** process each batch as it arrives, keep what was
//!   delivered when a later batch fails, and call the rows complete only after
//!   a clean end of stream (`Ok(None)`);
//! - **cancel mid-job:** drop a `JobStream` (or a `QueryStream`) to stop it.
//!   Its in-flight requests are cancelled and no further parts are sent.
//!   Cancellation is local: it does not prove the service stopped its work
//!   at the same instant.
//!
//! ```text
//! cargo run -p praevis-thetadata-client --example query_consumer
//! ```
#![allow(dead_code)] // The shared support module also serves the tests and benchmark.

use praevis_thetadata_client::{
    ClientConfig, NaiveDate, QueryError, RequestOptions, Session, ThetaClient,
    endpoints::StockHistoryEod,
};
use std::time::Duration;

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
/// job cancels its running parts, and it sends no more. Returns the rows read
/// before cancelling. `settle` keeps this function running for as long as
/// the job's running parts would take, so that a job that was not cancelled
/// would have started its next parts by the time it returns.
pub async fn cancel_midway(
    client: &ThetaClient,
    long: &StockHistoryEod,
    settle: Duration,
) -> Result<usize, QueryError> {
    // Two parts at a time (a job's concurrency is its own).
    let options = RequestOptions::default().with_concurrency(Some(2));
    let mut job = client.job_with(long, options).await?;
    let first = job.next_batch().await?.map_or(0, |b| b.num_rows());
    drop(job); // the cancellation
    tokio::time::sleep(settle).await;
    Ok(first)
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

/// Every request streams 10 slow batches (about 1 s each request).
fn slow_script() -> support::Script {
    let (batch, _) = support::table(support::Shape::Prices, 3);
    support::Script {
        messages: vec![Ok(support::response(&batch, false)); 10],
        message_delay: Duration::from_millis(100),
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
    match cancel_midway(&client, &long, Duration::from_millis(1_500)).await {
        Ok(rows) => println!(
            "cancelled a job after its first batch ({rows} rows); the server saw {} of its 6 parts",
            fixture.requests.lock().unwrap().len()
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
    async fn dropping_a_job_cancels_it_and_sends_no_more_parts() {
        let fixture = support::Fixture::start(slow_script());
        let client = client_for(&fixture, support::session().await).await;
        let long = StockHistoryEod::new("SYNTHETIC", day("2020-01-01"), day("2025-12-31"));
        let rows = cancel_midway(&client, &long, Duration::from_millis(1_500))
            .await
            .unwrap();
        assert_eq!(rows, 3);
        // Six yearly parts, two at a time. A job still running would have
        // started parts 3 and 4 when the first two finished (about 1 s).
        assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    }
}
