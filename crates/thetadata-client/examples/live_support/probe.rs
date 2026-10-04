//! Single-request negative entitlement probe; no success-data feature claim.
use super::{MAX_FRAME, Provenance, Result, config};
use serde::Serialize;
use std::time::Duration;
use thetadata_client::{Session, ThetaClient, queries as api};
use thetadata_proto::endpoints::ResponseData;

#[derive(Serialize)]
pub struct Report {
    format: &'static str,
    #[serde(flatten)]
    provenance: Provenance,
    connection: &'static str,
    product: &'static str,
    declared_tier: String,
    method: &'static str,
    symbol: Vec<String>,
    venue: &'static str,
    min_time: Option<String>,
    expected_grpc_code: i32,
    expected_phase: &'static str,
    pub auth_http_status: Option<u16>,
    pub outcome: Outcome,
    pub verification: String,
}

#[derive(Serialize)]
pub struct Outcome {
    grpc_code: Option<i32>,
    phase: &'static str,
    delivered_batches: usize,
    pub matches: bool,
}
impl Outcome {
    fn status(status: tonic::Status, phase: &'static str) -> Self {
        Self {
            grpc_code: Some(status.code() as i32),
            phase,
            delivered_batches: 0,
            matches: status.code() == tonic::Code::PermissionDenied,
        }
    }
    fn first_message(message: std::result::Result<Option<ResponseData>, tonic::Status>) -> Self {
        match message {
            Err(status) => Self::status(status, "before_first_response"),
            Ok(None) => Self {
                grpc_code: Some(0),
                phase: "empty_eof",
                delivered_batches: 0,
                matches: false,
            },
            Ok(Some(_)) => Self {
                grpc_code: None,
                phase: "unexpected_response_cancelled",
                delivered_batches: 1,
                matches: false,
            },
        }
    }
}

impl Report {
    pub fn new(environment: &str, symbol: &str, tier: &str) -> Result<Self> {
        // Reuse the existing single-symbol validation without inventing ticker syntax.
        let date = thetadata_client::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        if symbol.len() > 128 {
            return Err("probe symbol");
        }
        thetadata_client::StockEodRequest::new(symbol, date, date).map_err(|_| "probe symbol")?;
        Ok(Self {
            format: "thetadata-stock-quote-denial-v1",
            provenance: Provenance::new(environment)?,
            connection: "direct-grpc",
            product: "stocks",
            declared_tier: tier.into(),
            method: "GetStockSnapshotQuote",
            symbol: vec![symbol.into()],
            venue: "nqb",
            min_time: None,
            expected_grpc_code: 7,
            expected_phase: "before_data",
            auth_http_status: None,
            outcome: Outcome {
                grpc_code: None,
                phase: "not_started",
                delivered_batches: 0,
                matches: false,
            },
            verification: "not_completed".into(),
        })
    }
    pub async fn observe(&mut self, session: Session) -> Result<()> {
        let operation = async {
            let client = ThetaClient::with_session(config(None), session)
                .await
                .map_err(|_| "probe connection")?;
            let mut request = tonic::Request::new(api::StockSnapshotQuoteRequest {
                query_info: Some(client.query_info()),
                params: Some(api::StockSnapshotQuoteRequestQuery {
                    symbol: self.symbol.clone(),
                    venue: Some(self.venue.into()),
                    min_time: None,
                }),
            });
            request.set_timeout(Duration::from_secs(30));
            let mut raw = client.raw_client().max_decoding_message_size(MAX_FRAME);
            self.outcome = match raw.get_stock_snapshot_quote(request).await {
                Err(status) => Outcome::status(status, "headers"),
                Ok(response) => {
                    let mut stream = response.into_inner();
                    let message = tokio::time::timeout(Duration::from_secs(30), stream.message())
                        .await
                        .map_err(|_| "probe idle timeout")?;
                    Outcome::first_message(message)
                }
            };
            if self.outcome.matches {
                Ok(())
            } else {
                Err("permission-denied expectation mismatch")
            }
        };
        tokio::time::timeout(Duration::from_secs(40), operation)
            .await
            .unwrap_or(Err("probe deadline"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_permission_denied_before_data_matches_and_status_text_is_discarded() {
        for number in 1..=16 {
            let code = tonic::Code::from_i32(number);
            for phase in ["headers", "before_first_response"] {
                let outcome =
                    Outcome::status(tonic::Status::new(code, "SECRET-ACCOUNT-MESSAGE"), phase);
                assert_eq!(outcome.matches, code == tonic::Code::PermissionDenied);
                assert!(!serde_json::to_string(&outcome).unwrap().contains("SECRET"));
            }
        }
        assert!(!Outcome::first_message(Ok(None)).matches);
        let unexpected = Outcome::first_message(Ok(Some(ResponseData::default())));
        assert!(!unexpected.matches);
        assert_eq!(unexpected.delivered_batches, 1);
        assert_eq!(unexpected.grpc_code, None); // Cancelled locally, not an observed remote terminal code.
    }
}
