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
        let mut endpoint =
            Endpoint::from_shared(url.clone())?.connect_timeout(config.connect_timeout);
        match endpoint.uri().scheme_str() {
            Some("https") => {
                endpoint = endpoint.tls_config(ClientTlsConfig::new().with_native_roots())?
            }
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
