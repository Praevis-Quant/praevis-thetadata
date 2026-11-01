//! Builds the README example against the staged client archive and exercises offline validation.
use praevis_thetadata_client::{ClientConfig, NaiveDate, Session, StockEodRequest, ThetaClient};

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

#[tokio::main]
async fn main() {
    let day = |d| NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
    assert!(StockEodRequest::new("AAPL", day(1), day(5)).is_ok());
    assert!(StockEodRequest::new("AAPL,MSFT", day(1), day(5)).is_err());
    assert!(StockEodRequest::new("AAPL", day(5), day(1)).is_err());
    let _ = ClientConfig::default();
    println!("client-consumer: typed EOD request validation works from the staged archive");
}
