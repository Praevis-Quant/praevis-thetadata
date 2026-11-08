//! Typed requests for every RPC in the protocol descriptor (P1 design §3).
//!
//! [`generated`] is produced by `tools/generate_endpoints.py` from the pinned
//! vendor 1.0.17 inventory (`docs/research/python-1.0.17-inventory.json`) and
//! is checked in CI. **Do not edit it by hand.**
//!
//! Each request reproduces the vendor Python wrapper's semantics exactly:
//! - its defaults;
//! - its formatting (`YYYY-MM-DD` dates; `_fmt_time` and `_fmt_if_date` for
//!   times and expirations);
//! - its presence rules: an optional argument is sent only when it is truthy,
//!   as in Python. An explicit `false`, `0`, `0.0` or `""` is therefore **not
//!   sent**; this vendor quirk is kept deliberately for compatibility.
//!
//! RPCs without a vendor wrapper (futures, corporate actions) are generated
//! from their request messages and marked [`Stability::Unstable`].
//!
//! Request metadata differs from Python on purpose: the vendor copies every
//! argument into `QueryInfo.query_parameters` as Python strings; this client
//! sends only `client=rust` (see `ThetaClient::query_info`).
use crate::{ThetaClient, api, wire};
use chrono::{NaiveDate, NaiveTime, Timelike};
pub use praevis_thetadata_arrow::ColumnKind;
use std::future::Future;
use std::pin::Pin;

#[allow(clippy::all, missing_docs)]
#[rustfmt::skip]
pub mod generated;
pub use generated::*;

/// How settled an endpoint's request contract is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    /// Mirrors a public wrapper of the vendor Python package.
    VendorWrapper,
    /// Present in the descriptor only; no vendor wrapper or documentation.
    Unstable,
}

/// Registry entry for one generated endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointInfo {
    pub rpc: &'static str,
    pub python_name: Option<&'static str>,
    pub stability: Stability,
}

/// A raw server stream of response envelopes.
pub type RawStream = tonic::Streaming<wire::ResponseData>;
/// A boxed future, so endpoints can be dispatched generically.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// How `job()` may split a request into smaller requests by date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// Sent as one request.
    Whole,
    /// At most this many calendar days per request, inclusive.
    Days(u32),
}

/// A typed request for one RPC.
pub trait Endpoint {
    /// RPC name in the descriptor, for example `GetStockHistoryEod`.
    const RPC: &'static str;
    /// gRPC method path, for example
    /// `/BetaEndpoints.BetaThetaTerminal/GetStockHistoryEod`.
    const PATH: &'static str;
    /// The vendor Python wrapper name, if one exists.
    const PYTHON_NAME: Option<&'static str>;
    const STABILITY: Stability;
    /// The column kinds observed for this endpoint's responses
    /// (`docs/endpoints/schemas.json`). Empty unless registered: each column
    /// is then typed from its values, and a column with no value in a
    /// response is Arrow `Null`.
    const COLUMNS: &'static [(&'static str, ColumnKind)] = &[];
    /// The vendor wrapper's description text from the pinned artifact
    /// (unverified service claims).
    const VENDOR_DESCRIPTION: &'static str;
    /// The wire query message.
    type Query: Clone + Send + std::fmt::Debug + 'static;
    /// The wire request message: query info plus the query.
    type Request: prost::Message + Send + 'static;
    /// Wrap a query in its request message.
    fn request(query_info: api::QueryInfo, query: Self::Query) -> Self::Request;
    /// How the planner may split this request (P1 PR 4b).
    const SPLIT: Split = Split::Whole;
    /// The inclusive date range a split applies to, if this request has one.
    fn date_range(&self) -> Option<(NaiveDate, NaiveDate)> {
        None
    }
    /// The same request over another date range.
    fn with_date_range(&self, _start: NaiveDate, _end: NaiveDate) -> Option<Self>
    where
        Self: Sized,
    {
        None
    }
    /// Build the wire query exactly as the vendor wrapper does.
    fn to_query(&self) -> Self::Query;
    /// Send `query` on the raw stub and return the server stream.
    fn send(
        client: &ThetaClient,
        query: Self::Query,
    ) -> BoxFuture<'_, Result<RawStream, tonic::Status>>;
}

impl ThetaClient {
    /// Send a typed request and return the raw server stream (P1 PR 1:
    /// transport only; decoding to Arrow arrives with the columnar decoder).
    ///
    /// **Plain tonic, by design:** this raw stream does not use the client's
    /// own message framing (item 8, option 2a). tonic's decode buffer keeps up
    /// to one message per open stream for the stream's lifetime, and it is
    /// outside the client's accounted budget. Prefer `query()` or `job()`.
    pub async fn stream_raw<E: Endpoint>(&self, request: &E) -> Result<RawStream, tonic::Status> {
        E::send(self, request.to_query()).await
    }
    pub(crate) fn raw_request<T>(&self, params: T) -> tonic::Request<T> {
        let mut request = tonic::Request::new(params);
        request.set_timeout(self.config.request_timeout);
        request
    }
}

/// A date or a vendor wildcard/string, as accepted by `_fmt_if_date`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expiration {
    Date(NaiveDate),
    /// Passed through unchanged, for example `*` for every expiration.
    Text(String),
}
impl From<NaiveDate> for Expiration {
    fn from(value: NaiveDate) -> Self {
        Self::Date(value)
    }
}
impl From<&str> for Expiration {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<String> for Expiration {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl Expiration {
    /// `_fmt_if_date`: strings unchanged, dates as `YYYY-MM-DD`.
    pub fn wire(&self) -> String {
        match self {
            Self::Date(date) => date_wire(*date),
            Self::Text(text) => text.clone(),
        }
    }
}

/// A time of day or a vendor time string, as accepted by `_fmt_time`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    Time(NaiveTime),
    /// Passed through unchanged, for example the default `09:30:00`.
    Text(String),
}
impl From<NaiveTime> for TimeOfDay {
    fn from(value: NaiveTime) -> Self {
        Self::Time(value)
    }
}
impl From<&str> for TimeOfDay {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<String> for TimeOfDay {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl TimeOfDay {
    /// `_fmt_time`: strings unchanged; times as `HH:MM:SS.mmm`, truncating
    /// sub-millisecond digits (the vendor's microsecond // 1000).
    pub fn wire(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Time(time) => format!(
                "{:02}:{:02}:{:02}.{:03}",
                time.hour(),
                time.minute(),
                time.second(),
                (time.nanosecond() % 1_000_000_000) / 1_000_000
            ),
        }
    }
}

/// `date.strftime('%Y-%m-%d')`.
pub fn date_wire(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Python truthiness of an argument, for the vendor's `if arg:` guards.
pub trait Truthy {
    fn truthy(&self) -> bool;
}
impl Truthy for String {
    fn truthy(&self) -> bool {
        !self.is_empty()
    }
}
impl Truthy for f64 {
    fn truthy(&self) -> bool {
        *self != 0.0
    }
}
impl Truthy for i32 {
    fn truthy(&self) -> bool {
        *self != 0
    }
}
impl Truthy for bool {
    fn truthy(&self) -> bool {
        *self
    }
}
impl Truthy for NaiveDate {
    fn truthy(&self) -> bool {
        true
    }
}
impl Truthy for Expiration {
    fn truthy(&self) -> bool {
        match self {
            Self::Date(_) => true,
            Self::Text(text) => !text.is_empty(),
        }
    }
}
impl Truthy for TimeOfDay {
    fn truthy(&self) -> bool {
        // datetime.time objects are always truthy (Python 3.5+).
        match self {
            Self::Time(_) => true,
            Self::Text(text) => !text.is_empty(),
        }
    }
}
impl Truthy for Symbols {
    fn truthy(&self) -> bool {
        // A Python list (or a non-empty string) is truthy when non-empty.
        !self.0.is_empty() && !(self.0.len() == 1 && self.0[0].is_empty())
    }
}
impl Truthy for Vec<String> {
    fn truthy(&self) -> bool {
        !self.is_empty()
    }
}

/// `query.symbol.extend([symbol] if isinstance(symbol, str) else symbol)`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Symbols(pub Vec<String>);
impl From<&str> for Symbols {
    fn from(value: &str) -> Self {
        Self(vec![value.to_owned()])
    }
}
impl From<String> for Symbols {
    fn from(value: String) -> Self {
        Self(vec![value])
    }
}
impl<S: Into<String>> From<Vec<S>> for Symbols {
    fn from(value: Vec<S>) -> Self {
        Self(value.into_iter().map(Into::into).collect())
    }
}

#[doc(hidden)]
pub fn query_info(client: &ThetaClient) -> api::QueryInfo {
    client.query_info()
}
