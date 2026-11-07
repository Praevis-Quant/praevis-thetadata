//! An allocation-free parser for the Terminal's flat event JSON: objects at
//! most two levels deep, strings without escapes, numbers, booleans and
//! null. Anything else is refused (`None`) rather than guessed. Prices keep
//! their decimal digits exactly.
use crate::event::{Contract, Kind, Price, Quote, Response, Right, Root, Status, Trade, TypeName};

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// A JSON value as it appears in the text.
enum Value<'a> {
    Str(&'a str),
    Num(&'a str),
    Bool,
    Null,
    Object,
}

impl<'a> Cursor<'a> {
    fn space(&mut self) {
        while self.at < self.bytes.len()
            && matches!(self.bytes[self.at], b' ' | b'\n' | b'\r' | b'\t')
        {
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> Option<()> {
        self.space();
        (self.bytes.get(self.at) == Some(&byte)).then(|| self.at += 1)
    }
    fn peek(&mut self) -> Option<u8> {
        self.space();
        self.bytes.get(self.at).copied()
    }
    fn string(&mut self) -> Option<&'a str> {
        self.eat(b'"')?;
        let start = self.at;
        while let Some(&b) = self.bytes.get(self.at) {
            match b {
                b'"' => {
                    let text = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
                    self.at += 1;
                    return Some(text);
                }
                // Escapes and control characters never appear in these
                // messages; refuse rather than decode.
                b'\\' | 0..=0x1f => return None,
                _ => self.at += 1,
            }
        }
        None
    }
    fn number(&mut self) -> Option<&'a str> {
        self.space();
        let start = self.at;
        while let Some(&b) = self.bytes.get(self.at) {
            if b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E') {
                self.at += 1;
            } else {
                break;
            }
        }
        (self.at > start).then(|| std::str::from_utf8(&self.bytes[start..self.at]).ok())?
    }
    fn word(&mut self, word: &[u8]) -> Option<()> {
        self.space();
        self.bytes[self.at..]
            .starts_with(word)
            .then(|| self.at += word.len())
    }
    /// Read one value; a nested object's fields are reported to `field` with
    /// `parent` as their path.
    fn value(
        &mut self,
        depth: u8,
        parent: &'a str,
        field: &mut impl FnMut(&'a str, &'a str, Value<'a>) -> Option<()>,
    ) -> Option<Value<'a>> {
        match self.peek()? {
            b'"' => self.string().map(Value::Str),
            b'{' => {
                if depth >= 2 {
                    return None;
                }
                self.object(depth + 1, parent, field)?;
                Some(Value::Object)
            }
            b't' => self.word(b"true").map(|()| Value::Bool),
            b'f' => self.word(b"false").map(|()| Value::Bool),
            b'n' => self.word(b"null").map(|()| Value::Null),
            _ => self.number().map(Value::Num),
        }
    }
    fn object(
        &mut self,
        depth: u8,
        path: &'a str,
        field: &mut impl FnMut(&'a str, &'a str, Value<'a>) -> Option<()>,
    ) -> Option<()> {
        self.eat(b'{')?;
        if self.peek()? == b'}' {
            self.at += 1;
            return Some(());
        }
        loop {
            let key = self.string()?;
            self.eat(b':')?;
            let value = self.value(depth, key, field)?;
            if !matches!(value, Value::Object) {
                field(path, key, value)?;
            }
            match self.peek()? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Some(());
                }
                _ => return None,
            }
        }
    }
}

/// An integer field; `None` for fractions, exponents or overflow.
fn integer(text: &str) -> Option<i64> {
    text.parse().ok()
}

/// An exact decimal from a JSON number's digits (`110.25` is 11025 at scale
/// 2). Exponents are refused; so are more than 18 significant digits.
fn price(text: &str) -> Option<Price> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 18
    {
        return None;
    }
    let mut mantissa: i64 = 0;
    for b in whole.bytes().chain(fraction.bytes()) {
        mantissa = mantissa.checked_mul(10)?.checked_add(i64::from(b - b'0'))?;
    }
    Some(Price {
        mantissa: if negative { -mantissa } else { mantissa },
        scale: fraction.len() as u8,
    })
}

/// Parse one Terminal event message. `None` if it is not one this slice
/// understands exactly; the caller counts those.
pub fn parse(text: &str) -> Option<Kind> {
    let mut kind_type: Option<&str> = None;
    let mut status = Status::Other;
    let mut response: Option<&str> = None;
    let mut req_id: Option<i64> = None;
    let (mut root, mut expiration, mut strike, mut right) = (None, None, None, None);
    let mut quote = Quote::default();
    let mut quote_fields = 0u16;
    let mut trade = Trade::default();
    let mut trade_fields = 0u8;
    let mut cursor = Cursor {
        bytes: text.as_bytes(),
        at: 0,
    };
    cursor.object(1, "", &mut |path, key, value| {
        match (path, key, value) {
            ("header", "type", Value::Str(t)) => kind_type = Some(t),
            ("header", "response", Value::Str(r)) => response = Some(r),
            ("header", "req_id", Value::Num(n)) => req_id = Some(integer(n)?),
            ("header", "status", Value::Str(s)) => {
                status = match s {
                    "CONNECTED" => Status::Connected,
                    "DISCONNECTED" => Status::Disconnected,
                    _ => Status::Other,
                }
            }
            ("contract", "root", Value::Str(r)) => root = Some(Root::new(r)?),
            ("contract", "expiration", Value::Num(n)) => {
                expiration = Some(u32::try_from(integer(n)?).ok()?)
            }
            ("contract", "strike", Value::Num(n)) => strike = Some(integer(n)?),
            ("contract", "right", Value::Str(r)) => {
                right = Some(match r {
                    "C" => Right::Call,
                    "P" => Right::Put,
                    _ => return None,
                })
            }
            ("trade", key, Value::Num(n)) => {
                let (slot, bit): (&mut i64, u8) = match key {
                    "ms_of_day" => (&mut trade.ms_of_day, 1),
                    "sequence" => (&mut trade.sequence, 2),
                    "size" => (&mut trade.size, 4),
                    "condition" => (&mut trade.condition, 8),
                    "exchange" => (&mut trade.exchange, 16),
                    "price" => {
                        trade.price = price(n)?;
                        trade_fields |= 32;
                        return Some(());
                    }
                    "date" => {
                        trade.date = u32::try_from(integer(n)?).ok()?;
                        trade_fields |= 64;
                        return Some(());
                    }
                    // Unknown numeric trade fields are ignored.
                    _ => return Some(()),
                };
                *slot = integer(n)?;
                trade_fields |= bit;
            }
            ("quote", key, Value::Num(n)) => {
                let slot: Option<(&mut i64, u16)> = match key {
                    "ms_of_day" => Some((&mut quote.ms_of_day, 1)),
                    "bid_size" => Some((&mut quote.bid_size, 2)),
                    "bid_exchange" => Some((&mut quote.bid_exchange, 4)),
                    "bid_condition" => Some((&mut quote.bid_condition, 8)),
                    "ask_size" => Some((&mut quote.ask_size, 16)),
                    "ask_exchange" => Some((&mut quote.ask_exchange, 32)),
                    "ask_condition" => Some((&mut quote.ask_condition, 64)),
                    _ => None,
                };
                match (slot, key) {
                    (Some((slot, bit)), _) => {
                        *slot = integer(n)?;
                        quote_fields |= bit;
                    }
                    (None, "bid") => {
                        quote.bid = price(n)?;
                        quote_fields |= 128;
                    }
                    (None, "ask") => {
                        quote.ask = price(n)?;
                        quote_fields |= 256;
                    }
                    (None, "date") => {
                        quote.date = u32::try_from(integer(n)?).ok()?;
                        quote_fields |= 512;
                    }
                    // Unknown numeric quote fields are ignored.
                    _ => {}
                }
            }
            _ => {}
        }
        Some(())
    })?;
    cursor.space();
    if cursor.at != cursor.bytes.len() {
        return None;
    }
    match kind_type? {
        "QUOTE" => {
            // Every documented quote field must be present.
            if quote_fields != 0x3ff {
                return None;
            }
            Some(Kind::Quote {
                contract: Contract {
                    root: root?,
                    expiration: expiration?,
                    strike: strike?,
                    right: right?,
                },
                quote,
            })
        }
        "TRADE" => {
            // Every documented trade field must be present.
            if trade_fields != 0x7f {
                return None;
            }
            Some(Kind::Trade {
                contract: Contract {
                    root: root?,
                    expiration: expiration?,
                    strike: strike?,
                    right: right?,
                },
                trade,
            })
        }
        "STATUS" => Some(Kind::Heartbeat { status }),
        "REQ_RESPONSE" => Some(Kind::Response {
            id: req_id?,
            response: match response? {
                "SUBSCRIBED" => Response::Subscribed,
                "MAX_STREAMS_REACHED" => Response::MaxStreamsReached,
                "INVALID_PERMS" => Response::InvalidPerms,
                "ERROR" => Response::Error,
                _ => Response::Other,
            },
        }),
        other => Some(Kind::Other {
            status,
            kind: TypeName::new(other),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vendor's documented quote example (Streaming / US Options / Quote
    /// Stream), field for field.
    const QUOTE: &str = r#"{"header":{"status":"CONNECTED","type":"QUOTE"},"contract":{"security_type":"OPTION","root":"SPXW","expiration":20240315,"strike":4800000,"right":"C"},"quote":{"ms_of_day":26622025,"bid_size":7,"bid_exchange":5,"bid":110.2,"bid_condition":50,"ask_size":7,"ask_exchange":5,"ask":110.5,"ask_condition":50,"date":20231219}}"#;

    #[test]
    fn the_documented_quote_parses_exactly() {
        let Some(Kind::Quote { contract, quote }) = parse(QUOTE) else {
            panic!("not a quote");
        };
        assert_eq!(contract.root.as_str(), "SPXW");
        assert_eq!(
            (contract.expiration, contract.strike, contract.right),
            (20240315, 4_800_000, Right::Call)
        );
        assert_eq!(
            quote.bid,
            Price {
                mantissa: 1102,
                scale: 1
            }
        );
        assert_eq!(quote.ask.to_scale_10(), Some(1_105_000_000_000));
        assert_eq!(
            (
                quote.ms_of_day,
                quote.bid_size,
                quote.bid_exchange,
                quote.bid_condition
            ),
            (26_622_025, 7, 5, 50)
        );
        assert_eq!(quote.date, 20231219);
    }

    #[test]
    fn heartbeats_other_types_and_whitespace() {
        assert_eq!(
            parse(r#"{ "header" : { "type" : "STATUS", "status" : "CONNECTED" } }"#),
            Some(Kind::Heartbeat {
                status: Status::Connected
            })
        );
        assert_eq!(
            parse(r#"{"header":{"type":"STATUS","status":"DISCONNECTED"}}"#),
            Some(Kind::Heartbeat {
                status: Status::Disconnected
            })
        );
        assert_eq!(
            parse(
                r#"{"header":{"type":"TRADE","status":"CONNECTED"},"trade":{"price":1.5,"flags":[1,2]}}"#
            ),
            None,
            "arrays are not part of this slice's grammar"
        );
        assert_eq!(
            parse(
                r#"{"header":{"type":"OHLC","status":"CONNECTED"},"ohlc":{"open":1.5,"ok":true,"x":null}}"#
            ),
            Some(Kind::Other {
                status: Status::Connected,
                kind: TypeName::new("OHLC")
            })
        );
    }

    /// The vendor's documented trade example (Streaming / US Options / Full
    /// Trade Stream), field for field.
    const TRADE: &str = r#"{"header":{"status":"CONNECTED","type":"TRADE"},"contract":{"security_type":"OPTION","root":"QQQ","expiration":20231110,"strike":360000,"right":"P"},"trade":{"ms_of_day":49531278,"sequence":-563040482,"size":5,"condition":18,"price":1.06,"exchange":65,"date":20231103}}"#;

    #[test]
    fn the_documented_trade_parses_exactly() {
        let Some(Kind::Trade { contract, trade }) = parse(TRADE) else {
            panic!("not a trade");
        };
        assert_eq!(contract.root.as_str(), "QQQ");
        assert_eq!(
            (contract.expiration, contract.strike, contract.right),
            (20231110, 360_000, Right::Put)
        );
        assert_eq!(
            (
                trade.ms_of_day,
                trade.sequence,
                trade.size,
                trade.condition,
                trade.exchange,
                trade.date
            ),
            (49_531_278, -563_040_482, 5, 18, 65, 20231103)
        );
        assert_eq!(
            trade.price,
            Price {
                mantissa: 106,
                scale: 2
            }
        );
        // A trade missing a documented field is refused.
        assert_eq!(parse(&TRADE.replacen(r#""size":5,"#, "", 1)), None);
    }

    #[test]
    fn stream_request_answers_and_other_type_names() {
        // The answer to a stream request, as the vendor documents it.
        for (code, response) in [
            ("SUBSCRIBED", Response::Subscribed),
            ("MAX_STREAMS_REACHED", Response::MaxStreamsReached),
            ("INVALID_PERMS", Response::InvalidPerms),
            ("ERROR", Response::Error),
            ("SOMETHING_NEW", Response::Other),
        ] {
            let text =
                format!(r#"{{"header":{{"type":"REQ_RESPONSE","response":"{code}","req_id":3}}}}"#);
            assert_eq!(parse(&text), Some(Kind::Response { id: 3, response }));
        }
        assert_eq!(
            parse(r#"{"header":{"type":"REQ_RESPONSE","response":"SUBSCRIBED"}}"#),
            None
        );
        // A type name that is not a short upper-case word is kept empty.
        let Some(Kind::Other { kind, .. }) = parse(r#"{"header":{"type":"some value 123"}}"#)
        else {
            panic!("not other")
        };
        assert_eq!(kind.as_str(), "");
    }

    #[test]
    fn anything_inexact_or_malformed_is_refused() {
        let swap = |from: &str, to: &str| QUOTE.replacen(from, to, 1);
        for bad in [
            swap("110.2", "1.102e2"),                 // exponent
            swap("110.2", "110.2000000000000000001"), // too many digits
            swap("\"C\"", "\"X\""),                   // unknown right
            swap("\"SPXW\"", "\"SP\\\"XW\""),         // escape
            swap("\"bid_size\":7,", ""),              // a missing quote field
            swap("26622025", "2662.2025"),            // fraction in an integer
            format!("{QUOTE} trailing"),
            QUOTE[..QUOTE.len() - 1].to_owned(),
            r#"{"header":{"type":"QUOTE","a":{"b":{"c":1}}}}"#.to_owned(), // too deep
            String::new(),
        ] {
            assert_eq!(parse(&bad), None, "{bad}");
        }
        assert_eq!(
            price("-0.05"),
            Some(Price {
                mantissa: -5,
                scale: 2
            })
        );
        assert_eq!(
            price("7"),
            Some(Price {
                mantissa: 7,
                scale: 0
            })
        );
        assert_eq!(price("."), None);
    }
}
