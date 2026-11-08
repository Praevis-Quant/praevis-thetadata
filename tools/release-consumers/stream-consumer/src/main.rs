//! Parses a Terminal quote frame with the staged archive; no Terminal or
//! network is needed.
use praevis_thetadata_stream::{Kind, Right, parse};

const QUOTE: &str = r#"{"header":{"status":"CONNECTED","type":"QUOTE"},"contract":{"security_type":"OPTION","root":"SPY","expiration":20261106,"strike":570000,"right":"C"},"quote":{"ms_of_day":36000000,"bid_size":10,"bid_exchange":5,"bid":1.25,"bid_condition":50,"ask_size":12,"ask_exchange":5,"ask":1.27,"ask_condition":50,"date":20261106}}"#;

fn main() {
    let Some(Kind::Quote { contract, quote }) = parse(QUOTE) else {
        panic!("not a quote");
    };
    assert_eq!((contract.root.as_str(), contract.right), ("SPY", Right::Call));
    assert_eq!((quote.bid.mantissa, quote.bid.scale), (125, 2));
    println!("stream-consumer: {} quote parsed, exact bid", contract.root.as_str());
}
