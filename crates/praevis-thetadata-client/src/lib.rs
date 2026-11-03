#![doc = include_str!("../README.md")]
//!
//! Async batch streaming for ThetaData's direct gRPC API.
mod bounded;
mod decode;
mod envelope;
mod eod;
mod framing;
#[cfg(test)]
extern crate self as praevis_thetadata_client;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod test_fixture;
pub use eod::{
    DecodeLimits, EodBatchStream, EodError, EodPolicy, EodTableStream, NaiveDate, StockEodRequest,
};
pub use praevis_thetadata_core::{BatchValue, DataBatch, TimeZone, Timestamp};

pub use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Environment, Session};
pub use praevis_thetadata_core::{Table, Value};
pub use praevis_thetadata_proto::beta_endpoints as queries;
use praevis_thetadata_proto::{beta_endpoints as api, endpoints as wire};
use std::time::Duration;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(&'static str),
    #[error(transparent)]
    Auth(#[from] praevis_thetadata_auth::AuthError),
    #[error("gRPC connection failed: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("no data found")]
    NoData,
    #[error("gRPC request failed: {0}")]
    Rpc(Box<tonic::Status>),
    #[error("timed out waiting for the next response batch")]
    IdleTimeout,
    #[error("invalid protobuf response: {0}")]
    Protobuf(#[from] prost::DecodeError),
    #[error("invalid compressed response: {0}")]
    Compression(#[from] std::io::Error),
    #[error("unsupported compression algorithm {0}")]
    UnsupportedCompression(i32),
    #[error("response exceeds the configured batch limit of {0} bytes")]
    BatchLimit(usize),
    #[error("flat-file responses require a dedicated downloader; table decoding is unsupported")]
    FlatFileUnsupported,
    #[error(transparent)]
    Data(#[from] praevis_thetadata_core::DataError),
}

impl From<tonic::Status> for Error {
    fn from(status: tonic::Status) -> Self {
        if status.code() == tonic::Code::NotFound {
            Self::NoData
        } else {
            Self::Rpc(Box::new(status))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// None chooses the session environment's default MDDS host.
    pub endpoint: Option<String>,
    pub allow_insecure: bool,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_batch_bytes: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            allow_insecure: false,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(300),
            idle_timeout: Duration::from_secs(60),
            max_batch_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct ThetaClient {
    channel: Channel,
    stub: api::beta_theta_terminal_client::BetaThetaTerminalClient<Channel>,
    session: Session,
    config: ClientConfig,
    eod_pool: std::sync::Arc<eod::Pool>,
}

impl ThetaClient {
    /// Consume a session from the standalone auth crate without authenticating again.
    pub async fn with_session(config: ClientConfig, session: Session) -> Result<Self, Error> {
        Self::connect(
            config,
            session,
            eod::Pool::new(EodPolicy::default()).expect("valid default EOD policy"),
            false,
        )
        .await
    }
    async fn connect(
        config: ClientConfig,
        session: Session,
        eod_pool: std::sync::Arc<eod::Pool>,
        typed_primary: bool,
    ) -> Result<Self, Error> {
        let tls = ClientTlsConfig::new().with_native_roots();
        Self::connect_with(config, session, eod_pool, typed_primary, tls).await
    }
    /// Private seam: production always passes the native-root configuration
    /// above. Only the TLS success tests add a generated test root (EOD-L3-005).
    async fn connect_with(
        config: ClientConfig,
        session: Session,
        eod_pool: std::sync::Arc<eod::Pool>,
        typed_primary: bool,
        tls: ClientTlsConfig,
    ) -> Result<Self, Error> {
        if config.max_batch_bytes == 0 || config.max_batch_bytes > i32::MAX as usize {
            return Err(Error::Config("batch limit must be between 1 and i32::MAX"));
        }
        if config.connect_timeout.is_zero()
            || config.request_timeout.is_zero()
            || config.idle_timeout.is_zero()
        {
            return Err(Error::Config("timeouts must be positive"));
        }
        let host = match session.environment() {
            Environment::Prod => "mdds-01.thetadata.us",
            Environment::Stage => "mdds-stage.thetadata.us",
        };
        let url = config
            .endpoint
            .clone()
            .unwrap_or_else(|| format!("https://{host}:443"));
        // Same rule as the auth URL (AUTH-L3-005): no embedded credentials, query or
        // fragment. Checked on the string before tonic parses it, so no transport error,
        // Debug or log output can echo such a URL (EOD-L3-017).
        let authority = url
            .split_once("://")
            .map_or(url.as_str(), |(_, rest)| rest)
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        if authority.contains('@') || url.contains('?') || url.contains('#') {
            return Err(Error::Config(
                "endpoint URL cannot contain credentials, a query, or a fragment",
            ));
        }
        let mut endpoint =
            Endpoint::from_shared(url.clone())?.connect_timeout(config.connect_timeout);
        match endpoint.uri().scheme_str() {
            Some("https") => endpoint = endpoint.tls_config(tls)?,
            Some("http") if config.allow_insecure => {}
            _ => {
                return Err(Error::Config(
                    "HTTPS required unless allow_insecure is enabled",
                ));
            }
        }
        let typed_endpoint = endpoint
            .clone()
            .initial_stream_window_size(eod::STREAM_WINDOW)
            .initial_connection_window_size(eod_pool.connection_window())
            .http2_adaptive_window(false);
        // Isolate raw traffic from typed admission/flow-control guarantees.
        // Establish only the selected API's channel; the other connects lazily.
        let (channel, raw) = if typed_primary {
            (typed_endpoint.connect().await?, endpoint.connect_lazy())
        } else {
            (typed_endpoint.connect_lazy(), endpoint.connect().await?)
        };
        let stub = api::beta_theta_terminal_client::BetaThetaTerminalClient::new(raw)
            .max_decoding_message_size(config.max_batch_bytes + 1024);
        Ok(Self {
            channel,
            stub,
            session,
            config,
            eod_pool,
        })
    }
    pub fn session(&self) -> Session {
        self.session.clone()
    }
    /// Use with the generated raw stub for advanced or flat-file workflows.
    pub fn query_info(&self) -> api::QueryInfo {
        api::QueryInfo {
            auth_token: Some(wire::AuthToken {
                session_uuid: self.session.session_id().into(),
            }),
            email_hint: self.session.user().email.clone(),
            query_parameters: [("client".into(), "rust".into())].into(),
            ..Default::default()
        }
    }
    pub fn raw_client(&self) -> api::beta_theta_terminal_client::BetaThetaTerminalClient<Channel> {
        self.stub.clone()
    }
}

include!(concat!(env!("OUT_DIR"), "/methods.rs"));

pub struct ResponseStream {
    inner: tonic::Streaming<wire::ResponseData>,
    max_batch_bytes: usize,
    idle_timeout: Duration,
}

impl ResponseStream {
    /// Read one bounded batch. Drop the stream to cancel the request.
    pub async fn next_batch(&mut self) -> Result<Option<Table>, Error> {
        let response = tokio::time::timeout(self.idle_timeout, self.inner.message())
            .await
            .map_err(|_| Error::IdleTimeout)??;
        response
            .map(|response| decode::decode(response, self.max_batch_bytes))
            .transpose()
    }
}

/// Trusted-TLS success fixture (EOD-L3-005, deferred item 3). Each test
/// generates its own CA, server certificate and key with rcgen (decision D-F b),
/// so no key material is committed. The client keeps its native roots and adds
/// only the generated root, through the private `connect_with` seam.
#[cfg(test)]
mod tls_tests {
    use super::*;
    use crate::test_fixture as fixture;
    use rcgen::{
        BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer,
        KeyPair, KeyUsagePurpose,
    };
    use tonic::transport::Certificate;

    struct Ca {
        params: CertificateParams,
        key: KeyPair,
        pem: String,
    }
    fn ca(name: &str) -> Ca {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.distinguished_name.push(DnType::CommonName, name);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let pem = params.self_signed(&key).unwrap().pem();
        Ca { params, key, pem }
    }
    /// A server certificate for `names`, signed by `ca`: (certificate PEM, key PEM).
    fn server_cert(ca: &Ca, names: &[&str]) -> (String, String) {
        let key = KeyPair::generate().unwrap();
        let mut params =
            CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>())
                .unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, "praevis test server");
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let issuer = Issuer::from_params(&ca.params, &ca.key);
        let cert = params.signed_by(&key, &issuer).unwrap();
        (cert.pem(), key.serialize_pem())
    }
    async fn connect(server: &fixture::Fixture, trusted: &Ca) -> Result<ThetaClient, Error> {
        let tls = ClientTlsConfig::new()
            .with_native_roots()
            .ca_certificate(Certificate::from_pem(&trusted.pem));
        ThetaClient::connect_with(
            ClientConfig {
                endpoint: Some(server.endpoint.clone()),
                ..Default::default()
            },
            fixture::session().await,
            eod::Pool::new(EodPolicy::default()).unwrap(),
            true,
            tls,
        )
        .await
    }
    fn request() -> StockEodRequest {
        StockEodRequest::new(
            "SYNTHETIC",
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn trusted_tls_delivers_numeric_table_and_raw_results() {
        let ca = ca("praevis test CA");
        let (wire, expected) = fixture::table(fixture::Shape::Mixed, 3);
        let script = || fixture::Script {
            messages: vec![
                Ok(fixture::response(&wire, true)),
                Ok(fixture::response(&wire, false)),
            ],
            ..Default::default()
        };
        let (cert, key) = server_cert(&ca, &["127.0.0.1"]);
        let server = fixture::Fixture::start_tls(script(), cert.clone(), key.clone());
        assert!(server.endpoint.starts_with("https://127.0.0.1:"));
        let client = connect(&server, &ca).await.unwrap();
        let mut numeric = client.stock_eod_batches(request()).await.unwrap();
        for _ in 0..2 {
            assert_eq!(
                numeric.next_batch().await.unwrap().unwrap().into_table(),
                expected
            );
        }
        assert!(numeric.next_batch().await.unwrap().is_none());
        let server = fixture::Fixture::start_tls(script(), cert.clone(), key.clone());
        let client = connect(&server, &ca).await.unwrap();
        let mut table = client.stock_eod(request()).await.unwrap();
        for _ in 0..2 {
            assert_eq!(table.next_batch().await.unwrap().unwrap(), expected);
        }
        assert!(table.next_batch().await.unwrap().is_none());
        let server = fixture::Fixture::start_tls(script(), cert, key);
        let client = connect(&server, &ca).await.unwrap();
        let mut raw = client.stock_history_eod(fixture::query()).await.unwrap();
        for _ in 0..2 {
            assert_eq!(raw.next_batch().await.unwrap().unwrap(), expected);
        }
        assert!(raw.next_batch().await.unwrap().is_none());
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn untrusted_issuer_and_wrong_name_are_rejected_before_any_request() {
        let trusted = ca("praevis test CA");
        let other = ca("unrelated CA");
        for (issuer, names, case) in [
            (
                &other,
                &["127.0.0.1"][..],
                "certificate from an untrusted CA",
            ),
            (
                &trusted,
                &["localhost", "other.invalid"][..],
                "certificate for another name",
            ),
        ] {
            let (cert, key) = server_cert(issuer, names);
            let server = fixture::Fixture::start_tls(fixture::Script::default(), cert, key);
            assert!(connect(&server, &trusted).await.is_err(), "{case}");
            assert!(server.requests.lock().unwrap().is_empty(), "{case}");
        }
    }
}
