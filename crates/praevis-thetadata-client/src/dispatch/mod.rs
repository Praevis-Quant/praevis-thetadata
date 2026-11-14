//! Build a typed request from its **name and parameters at runtime**, and
//! run it through your own [`Runner`].
//!
//! For a service that receives requests as data (for example
//! `{"endpoint": "option_history_open_interest", "params": {...}}`) and
//! would otherwise hand-write a match over every typed request.
//!
//! - [`dispatch`] accepts the vendor wrapper name (`option_history_eod`) or
//!   the RPC name (`GetOptionHistoryEod`), reads each parameter from a
//!   [`Params`] source, rejects unknown parameter names, and hands the
//!   typed request to the [`Runner`].
//! - [`params`] describes each endpoint's parameters (name, kind, whether
//!   required, and the vendor default, as a value and as written), so a
//!   caller can validate a request before sending anything, and treat an
//!   omitted parameter and its explicit vendor default as the same request
//!   ([`ParamInfo::default_value`]).
//! - [`ENDPOINTS`] lists every endpoint, with its wrapper name if it has one
//!   and its RPC name ([`EndpointName`]).
//!
//! **One request, as given.** `dispatch` builds one typed request and does
//! not split it: a multi-day tick request goes to the [`Runner`] whole. To
//! plan it as [`job`](crate::ThetaClient::job) does, run it as a job, or
//! split it with [`plan`](crate::plan) in the runner.
//!
//! **Values are sent as given, as the vendor package sends them.** Nothing
//! is normalised: a time of `09:30` and the default `09:30:00` are different
//! requests on the wire. The vendor defaults' form,
//! which [`ParamInfo::default_value`] returns, is the canonical one: time
//! text as `HH:MM:SS`. A service that compares or caches requests should
//! bring values to that form before dispatching.
//!
//! **Stability is unchanged by dispatch.** The 19 descriptor-only RPCs are
//! reachable by their RPC names, with their request fields as parameter
//! names, but they stay [`Stability::Unstable`](crate::endpoints::Stability)
//! (each request's `STABILITY` constant; [`stability`]). Reaching one by
//! name does not make it supported.
//!
//! The Python package's planned calls go through this same generated code,
//! so the two cannot drift apart.
use crate::endpoints::{Endpoint, Expiration, Symbols, TimeOfDay};
use chrono::NaiveDate;

#[rustfmt::skip]
mod generated;

/// One endpoint [`dispatch`] accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct EndpointName {
    /// The vendor wrapper's name (`option_history_eod`), or `None` for one of
    /// the 19 descriptor-only RPCs, which have no wrapper and stay unstable.
    pub wrapper: Option<&'static str>,
    /// The RPC name (`GetOptionHistoryEod`).
    pub rpc: &'static str,
}

impl EndpointName {
    /// The name to call it by: the wrapper name, or the RPC name if it has
    /// no wrapper. [`dispatch`], [`params`] and [`stability`] accept it.
    pub const fn name(&self) -> &'static str {
        match self.wrapper {
            Some(wrapper) => wrapper,
            None => self.rpc,
        }
    }
}
pub use generated::{ENDPOINTS, dispatch, params, stability};

/// Runs one typed request. Implement it once for each way you consume a
/// result; [`dispatch`] calls it with the request it built, unsplit (see
/// [`plan`](crate::plan)). If running can fail, make `Output` a `Result`.
pub trait Runner {
    type Output;
    fn run<E>(self, request: E) -> Self::Output
    where
        E: Endpoint + Clone + Send + Sync + 'static;
}

/// What a parameter takes. [`Params::take`] receives it, so a source can
/// read its own representation (a JSON string, say) as the right kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParamKind {
    /// Text.
    Str,
    /// A whole number. The requests' fields are 32-bit: a larger value is
    /// rejected, never truncated.
    Int,
    /// A number.
    Float,
    Bool,
    /// A calendar date.
    Date,
    /// A time of day, or vendor time text, sent as given (the vendor
    /// defaults write `HH:MM:SS`).
    Time,
    /// An expiration date, or text such as `*` (every expiration).
    Expiration,
    /// One symbol or a list of symbols.
    Symbols,
    /// One text value or a list of them.
    StrList,
}

/// A parameter's value, from a [`Params`] source.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ParamValue {
    /// An explicit "no value": it clears an optional parameter (as passing
    /// `None` to the vendor wrapper does), and a required one is missing.
    Null,
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Date(NaiveDate),
    Time(TimeOfDay),
    Expiration(Expiration),
    /// A list of text values (symbols, or another text list).
    Strings(Vec<String>),
}

/// One parameter of an endpoint, from the generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParamInfo {
    /// The vendor wrapper's parameter name, or the request field name for a
    /// descriptor-only RPC.
    pub name: &'static str,
    pub kind: ParamKind,
    pub required: bool,
    /// The vendor wrapper's default exactly as written in the pinned
    /// package (for example `'1s'`, `None` or `_fmt_time('09:30:00')`), or
    /// `None` when it has none (required parameters, descriptor-only RPCs).
    /// [`default_value`](Self::default_value) is the same default as a value.
    pub vendor_default: Option<&'static str>,
    default: Option<DefaultValue>,
}

impl ParamInfo {
    /// The vendor default as the value the wrapper passes when the argument
    /// is omitted, `_fmt_time` resolved: `None` becomes [`ParamValue::Null`],
    /// `'1s'` a [`ParamValue::Str`], `_fmt_time('09:30:00')` a
    /// [`ParamValue::Time`] of that text. `None` when there is no default.
    /// Passing it to [`dispatch`] builds the same request as omitting the
    /// parameter.
    pub fn default_value(&self) -> Option<ParamValue> {
        self.default.map(|value| match value {
            DefaultValue::Null => ParamValue::Null,
            DefaultValue::Str(text) => ParamValue::Str(text.to_owned()),
            DefaultValue::Int(n) => ParamValue::Int(n),
            DefaultValue::Bool(b) => ParamValue::Bool(b),
            DefaultValue::Time(text) => ParamValue::Time(TimeOfDay::Text(text.to_owned())),
        })
    }
}

/// A vendor default, in a form the generated constants can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DefaultValue {
    Null,
    Str(&'static str),
    Int(i64),
    Bool(bool),
    Time(&'static str),
}

/// A source of parameter values, such as a JSON object or Python keyword
/// arguments.
pub trait Params {
    /// The source's own error, for example a value it cannot read.
    type Error;
    /// The value given for `name`, or `None` if none was given. `kind` is
    /// what the request field takes; the value returned is checked against
    /// it. Every name passed here counts as used.
    fn take(&mut self, name: &str, kind: ParamKind) -> Result<Option<ParamValue>, Self::Error>;
    /// Called after every parameter of the endpoint was taken: reject any
    /// name that was not.
    fn finish(self) -> Result<(), Self::Error>;
}

/// A parameter value the request cannot take.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParamError {
    /// A required parameter is absent or [`ParamValue::Null`].
    Missing { name: String },
    /// The value is not of the parameter's kind.
    Kind {
        name: String,
        expected: &'static str,
    },
    /// A whole number outside the field's 32-bit range.
    OutOfRange { name: String },
}

impl std::fmt::Display for ParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { name } => write!(f, "missing required argument: {name}"),
            Self::Kind { name, expected } => write!(f, "{name}: expected {expected}"),
            Self::OutOfRange { name } => write!(f, "{name}: out of the 32-bit integer range"),
        }
    }
}
impl std::error::Error for ParamError {}

/// Why [`dispatch`] did not run a request.
#[derive(Debug)]
#[non_exhaustive]
pub enum DispatchError<E> {
    /// Neither a vendor wrapper name nor an RPC name.
    UnknownEndpoint(String),
    /// A value the request cannot take.
    Param(ParamError),
    /// The source's own error, from [`Params::take`] or [`Params::finish`].
    Source(E),
}

impl<E: std::fmt::Display> std::fmt::Display for DispatchError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEndpoint(name) => write!(f, "unknown endpoint: {name}"),
            Self::Param(error) => error.fmt(f),
            Self::Source(error) => error.fmt(f),
        }
    }
}
impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for DispatchError<E> {}

/// A request field type a [`ParamValue`] converts to. Sealed: only the
/// types the generated requests use.
pub trait FromParam: Sized + sealed::Sealed {
    /// The kind passed to [`Params::take`].
    const KIND: ParamKind;
    #[doc(hidden)]
    fn from_param(name: &str, value: ParamValue) -> Result<Self, ParamError>;
}
mod sealed {
    pub trait Sealed {}
}

fn kind(name: &str, expected: &'static str) -> ParamError {
    ParamError::Kind {
        name: name.to_owned(),
        expected,
    }
}

fn iso_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

macro_rules! from_param {
    ($ty:ty, $kind:ident, |$name:ident, $value:ident| $body:expr) => {
        impl sealed::Sealed for $ty {}
        impl FromParam for $ty {
            const KIND: ParamKind = ParamKind::$kind;
            fn from_param($name: &str, $value: ParamValue) -> Result<Self, ParamError> {
                $body
            }
        }
    };
}

from_param!(String, Str, |name, value| match value {
    ParamValue::Str(text) => Ok(text),
    _ => Err(kind(name, "str")),
});
from_param!(i32, Int, |name, value| match value {
    ParamValue::Int(n) => i32::try_from(n).map_err(|_| ParamError::OutOfRange {
        name: name.to_owned()
    }),
    _ => Err(kind(name, "int")),
});
from_param!(f64, Float, |name, value| match value {
    ParamValue::Float(x) => Ok(x),
    ParamValue::Int(n) => Ok(n as f64),
    _ => Err(kind(name, "float")),
});
from_param!(bool, Bool, |name, value| match value {
    ParamValue::Bool(b) => Ok(b),
    _ => Err(kind(name, "bool")),
});
from_param!(NaiveDate, Date, |name, value| match value {
    ParamValue::Date(day) => Ok(day),
    ParamValue::Str(text) => iso_date(&text).ok_or_else(|| kind(name, "YYYY-MM-DD")),
    _ => Err(kind(name, "a date")),
});
from_param!(Expiration, Expiration, |name, value| match value {
    ParamValue::Expiration(expiration) => Ok(expiration),
    ParamValue::Date(day) => Ok(Expiration::Date(day)),
    ParamValue::Str(text) => Ok(Expiration::Text(text)),
    _ => Err(kind(name, "a date or text")),
});
from_param!(TimeOfDay, Time, |name, value| match value {
    ParamValue::Time(time) => Ok(time),
    ParamValue::Str(text) => Ok(TimeOfDay::Text(text)),
    _ => Err(kind(name, "a time or text")),
});
from_param!(Symbols, Symbols, |name, value| match value {
    ParamValue::Strings(list) => Ok(Symbols(list)),
    ParamValue::Str(text) => Ok(Symbols(vec![text])),
    _ => Err(kind(name, "str or list of str")),
});
from_param!(Vec<String>, StrList, |name, value| match value {
    ParamValue::Strings(list) => Ok(list),
    ParamValue::Str(text) => Ok(vec![text]),
    _ => Err(kind(name, "str or list of str")),
});

/// A required parameter.
fn required<T: FromParam, P: Params>(p: &mut P, name: &str) -> Result<T, DispatchError<P::Error>> {
    match p.take(name, T::KIND).map_err(DispatchError::Source)? {
        None | Some(ParamValue::Null) => Err(DispatchError::Param(ParamError::Missing {
            name: name.to_owned(),
        })),
        Some(value) => T::from_param(name, value).map_err(DispatchError::Param),
    }
}

/// An optional parameter: absent keeps the default (`None`), an explicit
/// [`ParamValue::Null`] clears it (`Some(None)`).
fn optional<T: FromParam, P: Params>(
    p: &mut P,
    name: &str,
) -> Result<Option<Option<T>>, DispatchError<P::Error>> {
    match p.take(name, T::KIND).map_err(DispatchError::Source)? {
        None => Ok(None),
        Some(ParamValue::Null) => Ok(Some(None)),
        Some(value) => T::from_param(name, value)
            .map(|v| Some(Some(v)))
            .map_err(DispatchError::Param),
    }
}

/// A plain field of a descriptor-only request: absent or
/// [`ParamValue::Null`] keeps the default.
fn value<T: FromParam, P: Params>(
    p: &mut P,
    name: &str,
) -> Result<Option<T>, DispatchError<P::Error>> {
    match p.take(name, T::KIND).map_err(DispatchError::Source)? {
        None | Some(ParamValue::Null) => Ok(None),
        Some(value) => T::from_param(name, value)
            .map(Some)
            .map_err(DispatchError::Param),
    }
}

fn finish<P: Params>(p: P) -> Result<(), DispatchError<P::Error>> {
    p.finish().map_err(DispatchError::Source)
}

#[cfg(test)]
mod tests;
