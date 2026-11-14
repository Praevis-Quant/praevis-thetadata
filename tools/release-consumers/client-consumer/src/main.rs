//! Builds the README example and the client surface (beta.2) against the
//! staged client archive, and exercises offline behaviour. Nothing contacts
//! ThetaData.
use praevis_thetadata_client::dispatch::{self, ParamKind, ParamValue, Params, Runner};
use praevis_thetadata_client::endpoints::{self, Endpoint, StockHistoryEod};
use praevis_thetadata_client::{
    ClientConfig, NaiveDate, ParquetOptions, QueryError, RequestOptions, Session, StockEodRequest,
    ThetaClient, Tier, plan, unify,
};
use std::time::Duration;

#[allow(dead_code)]
async fn readme_example(session: Session) -> Result<(), Box<dyn std::error::Error>> {
    let client = ThetaClient::with_session(ClientConfig::default(), session).await?;
    let request = StockEodRequest::new(
        "AAPL",
        NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
    )?;
    let mut stream = client.stock_eod_batches(request).await?;
    while let Some(batch) = stream.next_batch().await? {
        for row in batch.rows() {
            let _ = row;
        }
    }
    Ok(())
}

/// The client surface: generated endpoints through `query()` and `job()`,
/// per-call options (timeouts and a job's concurrency) and Parquet output.
/// No scheduler, retry or default deadline (ADR-0021). Compiled against the
/// archive; it needs a session to run.
#[allow(dead_code)]
async fn frozen_surface(session: Session) -> Result<(), Box<dyn std::error::Error>> {
    let client = ThetaClient::with_session(ClientConfig::default(), session).await?;
    let _ = client.tier().concurrency();
    let day = |d| NaiveDate::from_ymd_opt(2024, 1, d).unwrap();
    let request = StockHistoryEod::new("AAPL", day(2), day(5));
    let options = RequestOptions::default()
        .with_timeout(Some(Duration::from_secs(60)))
        .with_idle_timeout(Some(Duration::from_secs(10)));
    let mut stream = client.query_with(&request, options.clone()).await?;
    while let Some(batch) = stream.next_batch().await? {
        let _ = batch.num_rows();
    }
    let batches = client.query(&request).await?.collect().await?;
    let _ = unify(batches);
    let mut job = client.job_with(&request, options.with_concurrency(Some(1))).await?;
    while let Some(batch) = job.next_batch().await? {
        let _ = batch.num_rows();
    }
    let summary = client
        .job(&request)
        .await?
        .to_parquet("aapl.parquet", ParquetOptions::default())
        .await?;
    let _ = summary;
    Ok(())
}

/// Requests by name (0.1.2): a map of parameters, as a service reading JSON
/// would have.
struct Map(Vec<(&'static str, ParamValue)>);
impl Params for Map {
    type Error = String;
    fn take(&mut self, name: &str, _: ParamKind) -> Result<Option<ParamValue>, String> {
        let at = self.0.iter().position(|(n, _)| *n == name);
        Ok(at.map(|i| self.0.remove(i).1))
    }
    fn finish(self) -> Result<(), String> {
        self.0.first().map_or(Ok(()), |(n, _)| Err(format!("unknown: {n}")))
    }
}

/// Plans the dispatched request as `job()` would, and counts the parts.
struct Parts;
impl Runner for Parts {
    type Output = (&'static str, usize);
    fn run<E: Endpoint + Clone + Send + Sync + 'static>(self, request: E) -> Self::Output {
        (E::RPC, plan(&request).len())
    }
}

#[tokio::main]
async fn main() {
    let day = |d| NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
    assert!(StockEodRequest::new("AAPL", day(1), day(5)).is_ok());
    assert!(StockEodRequest::new("AAPL,MSFT", day(1), day(5)).is_err());
    assert!(StockEodRequest::new("AAPL", day(5), day(1)).is_err());
    let _ = ClientConfig::default();
    // Generated endpoints and the job planner, offline.
    assert_eq!(endpoints::ALL.len(), 100);
    assert_eq!(StockHistoryEod::RPC, "GetStockHistoryEod");
    let long = StockHistoryEod::new(
        "AAPL",
        NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
    );
    assert_eq!(plan(&long).len(), 3);
    // The tier's documented limit: a job's default concurrency.
    assert_eq!(Tier::Pro.concurrency(), 8);
    assert_eq!(Tier::Free.concurrency(), 1);
    // No timeout by default.
    assert_eq!(ClientConfig::default().request_timeout, None);
    let _: Option<QueryError> = Some(QueryError::Busy);
    // Requests by name, offline.
    assert_eq!(dispatch::ENDPOINTS.len(), 100);
    let named = Map(vec![
        ("symbol", ParamValue::Str("AAPL".into())),
        ("start_date", ParamValue::Str("2022-01-01".into())),
        ("end_date", ParamValue::Str("2024-03-01".into())),
    ]);
    assert_eq!(
        dispatch::dispatch("stock_history_eod", named, Parts).unwrap(),
        ("GetStockHistoryEod", 3)
    );
    let venue = dispatch::params("stock_history_quote")
        .unwrap()
        .iter()
        .find(|p| p.name == "venue")
        .unwrap();
    assert_eq!(venue.default_value(), Some(ParamValue::Str("nqb".into())));
    println!(
        "client-consumer: typed EOD validation, 100 generated endpoints, the job planner, the tier's job concurrency and requests by name work from the staged archive"
    );
}
