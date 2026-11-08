//! Runnable consumer pattern for the typed stock EOD stream (EOD-L3-025).
//!
//! It runs against a synthetic local server (no credentials, no ThetaData traffic) that
//! delivers two batches and then fails. It shows how to:
//! - build the typed request from an explicit, already authenticated `Session`;
//! - consume incrementally, without accumulating hidden state;
//! - keep delivered rows, but call them **partial** if a later call fails;
//! - call a result **complete** only after a clean end of stream.
//!
//! ```text
//! cargo run -p praevis-thetadata-client --example eod_partial
//! ```
#![allow(dead_code)] // The shared support module also serves the tests and benchmark.

use praevis_thetadata_client::{
    ClientConfig, EodError, NaiveDate, Session, StockEodRequest, ThetaClient,
};

#[path = "../tests/support/mod.rs"]
mod support;

/// What the caller may conclude from one query.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Clean end of stream: these rows are the complete result for the query.
    Complete { rows: usize },
    /// The stream failed after delivering `rows`. They are valid but not complete.
    Partial { rows: usize, error: EodError },
}

/// The documented pattern. Process each batch as it arrives; never treat the rows so
/// far as complete unless `next_batch` returned `Ok(None)`. After an error, the stream
/// is fused (later calls return `Ok(None)`), and that does not mean success.
pub async fn consume(client: &ThetaClient, request: StockEodRequest) -> Result<Outcome, EodError> {
    let mut stream = client.stock_eod_batches(request).await?;
    let mut rows = 0;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                for row in batch.rows() {
                    std::hint::black_box(row); // process each row here
                }
                rows += batch.row_count();
            }
            Ok(None) => return Ok(Outcome::Complete { rows }),
            Err(error) => return Ok(Outcome::Partial { rows, error }),
        }
    }
}

/// A request with explicit dates. Validation happens before any RPC.
fn request() -> Result<StockEodRequest, EodError> {
    StockEodRequest::new(
        "SYNTHETIC",
        NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
    )
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

/// Two batches of two rows each, then a server-side failure mid-stream.
fn partial_failure_script() -> support::Script {
    let (batch, _) = support::table(support::Shape::Mixed, 2);
    support::Script {
        messages: vec![
            Ok(support::response(&batch, false)),
            Ok(support::response(&batch, true)),
            Err(tonic::Status::unavailable("synthetic mid-stream failure")),
        ],
        ..Default::default()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let fixture = support::Fixture::start(partial_failure_script());
    // A real application authenticates with praevis-thetadata-auth and passes the session.
    let client = client_for(&fixture, support::session().await).await;
    match consume(&client, request().unwrap()).await {
        Ok(Outcome::Complete { rows }) => println!("complete: {rows} rows"),
        Ok(Outcome::Partial { rows, error }) => {
            println!(
                "partial: {rows} rows delivered before the stream failed ({error}); not a complete result"
            )
        }
        Err(error) => println!("query did not start: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn partial_failure_is_never_reported_complete() {
        let fixture = support::Fixture::start(partial_failure_script());
        let client = client_for(&fixture, support::session().await).await;
        let outcome = consume(&client, request().unwrap()).await.unwrap();
        // Remote UNAVAILABLE stays a remote status, never a success or a no-data result.
        assert_eq!(
            outcome,
            Outcome::Partial {
                rows: 4,
                error: EodError::Remote(tonic::Code::Unavailable)
            }
        );
    }

    #[tokio::test]
    async fn clean_end_of_stream_is_complete() {
        let (batch, _) = support::table(support::Shape::Prices, 3);
        let fixture = support::Fixture::start(support::Script {
            messages: vec![Ok(support::response(&batch, false))],
            ..Default::default()
        });
        let client = client_for(&fixture, support::session().await).await;
        let outcome = consume(&client, request().unwrap()).await.unwrap();
        assert_eq!(outcome, Outcome::Complete { rows: 3 });
    }

    #[test]
    fn invalid_requests_fail_before_any_rpc() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(
            StockEodRequest::new("A,B", day, day).unwrap_err(),
            EodError::Input
        );
    }
}
