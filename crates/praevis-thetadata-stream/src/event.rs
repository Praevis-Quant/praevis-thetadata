//! Fixed-size events: `Copy`, no heap data, exact prices.
use std::time::Instant;

/// An option root, up to 8 ASCII bytes, stored inline.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Root {
    bytes: [u8; 8],
    len: u8,
}
impl Root {
    /// `None` if empty, longer than 8 bytes or not ASCII alphanumeric.
    pub fn new(text: &str) -> Option<Self> {
        let raw = text.as_bytes();
        if raw.is_empty() || raw.len() > 8 || !raw.iter().all(u8::is_ascii_alphanumeric) {
            return None;
        }
        let mut bytes = [0u8; 8];
        bytes[..raw.len()].copy_from_slice(raw);
        Some(Self {
            bytes,
            len: raw.len() as u8,
        })
    }
    pub fn as_str(&self) -> &str {
        // Only ASCII alphanumerics are ever stored.
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}
impl std::fmt::Debug for Root {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Right {
    Call,
    Put,
}

/// One option contract, as the Terminal identifies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Contract {
    pub root: Root,
    /// `yyyymmdd`.
    pub expiration: u32,
    /// In tenths of a cent, as the Terminal sends it (`570000` is $570.00).
    pub strike: i64,
    pub right: Right,
}

/// An exact decimal: `mantissa × 10^-scale`, taken from the JSON number's
/// digits (never a float).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Price {
    pub mantissa: i64,
    pub scale: u8,
}
impl Price {
    /// The value scaled to 10 decimal places (`Decimal128(38, 10)`), if it
    /// has at most 10.
    pub fn to_scale_10(self) -> Option<i128> {
        let up = 10u32.checked_sub(u32::from(self.scale))?;
        i128::from(self.mantissa).checked_mul(10i128.pow(up))
    }
}

/// A national best bid and offer update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Quote {
    pub ms_of_day: i64,
    pub bid_size: i64,
    pub bid_exchange: i64,
    pub bid: Price,
    pub bid_condition: i64,
    pub ask_size: i64,
    pub ask_exchange: i64,
    pub ask: Price,
    pub ask_condition: i64,
    /// `yyyymmdd`.
    pub date: u32,
}

/// The `header.status` of a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Connected,
    Disconnected,
    Other,
}

/// The Terminal's answer to a stream request (`type` REQ_RESPONSE), as the
/// vendor documents it ("Verify Stream Requests").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// Accepted. It does not confirm the contract exists: data flows only if
    /// it does.
    Subscribed,
    /// Too many contracts are streaming for the subscription.
    MaxStreamsReached,
    /// The subscription does not cover this stream.
    InvalidPerms,
    /// A generic failure.
    Error,
    /// A code this slice does not know.
    Other,
}

/// A message type name, for counting types this slice does not decode:
/// up to 16 bytes of `A-Z` and `_` (a schema word, never a value).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TypeName {
    bytes: [u8; 16],
    len: u8,
}
impl TypeName {
    pub(crate) fn new(text: &str) -> Self {
        let raw = text.as_bytes();
        if raw.len() > 16 || !raw.iter().all(|b| b.is_ascii_uppercase() || *b == b'_') {
            return Self::default();
        }
        let mut bytes = [0u8; 16];
        bytes[..raw.len()].copy_from_slice(raw);
        Self {
            bytes,
            len: raw.len() as u8,
        }
    }
    /// Empty if the type was not a short upper-case word.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}
impl std::fmt::Debug for TypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Quote {
        contract: Contract,
        quote: Quote,
    },
    /// A heartbeat (`type` STATUS), about one a second.
    Heartbeat {
        status: Status,
    },
    /// The answer to stream request `id` (`type` REQ_RESPONSE).
    Response {
        id: i64,
        response: Response,
    },
    /// A message of another type (for example TRADE), not decoded in this
    /// slice; `kind` is its type name.
    Other {
        status: Status,
        kind: TypeName,
    },
    /// The WebSocket closed or failed; the stream ends after this.
    Disconnected,
}

/// One event and when its frame was read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub kind: Kind,
    /// When the reader thread had the whole frame, before parsing.
    pub received: Instant,
}
