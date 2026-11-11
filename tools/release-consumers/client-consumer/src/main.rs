//! Builds the README example and the frozen beta.1 client surface against the
//! staged client archive, and exercises offline behaviour. Nothing contacts
//! ThetaData.
use praevis_thetadata_client::endpoints::{self, Endpoint, StockHistoryEod};
use praevis_thetadata_client::{
    ClientConfig, NaiveDate, ParquetOptions, QueryError, RequestOptions, SchedulerPolicy, Session,
    StockEodRequest, ThetaClient, Tier, plan, unify,
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

/// The frozen client surface (beta.1): generated endpoints through `query()`
/// and `job()`, per-call options, the scheduler and Parquet output. Compiled
/// against the archive; it needs a session to run.
#[allow(dead_code)]
async fn frozen_surface(session: Session) -> Result<(), Box<dyn std::error::Error>> {
    let client = ThetaClient::with_session(ClientConfig::default(), session)
        .await?
        .with_scheduler(SchedulerPolicy::for_tier(Tier::Free))?;
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
    // The tier's scheduler limits.
    assert_eq!(SchedulerPolicy::for_tier(Tier::Pro).concurrency, 8);
    assert_eq!(SchedulerPolicy::for_tier(Tier::Free).per_minute, Some(20));
    let _: Option<QueryError> = None;
    println!(
        "client-consumer: typed EOD validation, 100 generated endpoints, the job planner and the tier scheduler work from the staged archive"
    );
}
