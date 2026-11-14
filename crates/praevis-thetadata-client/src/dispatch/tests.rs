use super::*;
use crate::endpoints::Stability;
use std::collections::BTreeMap;

/// A parameter source over a map, as a service reading JSON would write.
#[derive(Default)]
struct Map {
    values: BTreeMap<String, ParamValue>,
    taken: Vec<(String, ParamKind)>,
}
impl Map {
    fn with(mut self, name: &str, value: ParamValue) -> Self {
        self.values.insert(name.into(), value);
        self
    }
}
impl Params for Map {
    type Error = String;
    fn take(&mut self, name: &str, kind: ParamKind) -> Result<Option<ParamValue>, String> {
        self.taken.push((name.into(), kind));
        Ok(self.values.remove(name))
    }
    fn finish(self) -> Result<(), String> {
        match self.values.keys().next() {
            None => Ok(()),
            Some(name) => Err(format!("unknown parameter: {name}")),
        }
    }
}

/// Returns the built request's RPC name and its wire query, for checking.
struct Describe;
impl Runner for Describe {
    type Output = (&'static str, String);
    fn run<E>(self, request: E) -> Self::Output
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        (E::RPC, format!("{:?}", request.to_query()))
    }
}

fn day(text: &str) -> ParamValue {
    ParamValue::Date(NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap())
}

fn eod() -> Map {
    Map::default()
        .with("symbol", ParamValue::Str("AAPL".into()))
        .with("start_date", day("2024-06-03"))
        .with("end_date", day("2024-06-07"))
}

#[test]
fn the_vendor_name_and_the_rpc_name_build_the_same_request() {
    let by_name = dispatch("stock_history_eod", eod(), Describe).unwrap();
    let by_rpc = dispatch("GetStockHistoryEod", eod(), Describe).unwrap();
    assert_eq!(by_name, by_rpc);
    assert_eq!(by_name.0, "GetStockHistoryEod");
    assert!(by_name.1.contains("2024-06-03"), "{}", by_name.1);
}

#[test]
fn errors_name_what_is_wrong_and_nothing_runs() {
    let missing = dispatch("stock_history_eod", Map::default(), Describe).unwrap_err();
    assert!(
        matches!(missing, DispatchError::Param(ParamError::Missing { ref name }) if name == "symbol")
    );
    let unknown = dispatch("stock_history_eodd", eod(), Describe).unwrap_err();
    assert!(matches!(unknown, DispatchError::UnknownEndpoint(ref n) if n == "stock_history_eodd"));
    let extra = dispatch(
        "stock_history_eod",
        eod().with("venue", ParamValue::Str("x".into())),
        Describe,
    );
    assert!(matches!(extra, Err(DispatchError::Source(ref e)) if e == "unknown parameter: venue"));
    let wrong = dispatch(
        "stock_history_eod",
        eod().with("symbol", ParamValue::Int(1)),
        Describe,
    );
    assert!(matches!(
        wrong,
        Err(DispatchError::Param(ParamError::Kind { .. }))
    ));
}

#[test]
fn integers_are_range_checked_never_truncated() {
    let request = || {
        Map::default()
            .with("symbol", ParamValue::Str("SPY".into()))
            .with("expiration", ParamValue::Str("*".into()))
            .with("start_date", day("2024-06-03"))
            .with("end_date", day("2024-06-07"))
    };
    let ok = dispatch(
        "option_history_eod",
        request().with("max_dte", ParamValue::Int(30)),
        Describe,
    );
    assert!(ok.unwrap().1.contains("max_dte: Some(30)"));
    let big = dispatch(
        "option_history_eod",
        request().with("max_dte", ParamValue::Int(i64::from(i32::MAX) + 1)),
        Describe,
    );
    assert!(
        matches!(big, Err(DispatchError::Param(ParamError::OutOfRange { ref name })) if name == "max_dte")
    );
}

#[test]
fn an_explicit_null_clears_an_optional_default() {
    // The vendor default for option EOD's strike is '*'.
    let base = || {
        Map::default()
            .with("symbol", ParamValue::Str("SPY".into()))
            .with("expiration", ParamValue::Str("*".into()))
            .with("start_date", day("2024-06-03"))
            .with("end_date", day("2024-06-07"))
    };
    let kept = dispatch("option_history_eod", base(), Describe).unwrap().1;
    let cleared = dispatch(
        "option_history_eod",
        base().with("strike", ParamValue::Null),
        Describe,
    )
    .unwrap()
    .1;
    assert_ne!(kept, cleared);
}

#[test]
fn metadata_describes_every_endpoint_and_matches_what_dispatch_reads() {
    assert_eq!(ENDPOINTS.len(), 100);
    for endpoint in ENDPOINTS {
        let rpc = endpoint.rpc;
        let names: Vec<&str> = endpoint.wrapper.into_iter().chain([rpc]).collect();
        assert_eq!(endpoint.name(), names[0]);
        for name in &names {
            assert!(params(name).is_some(), "{name}");
            assert!(stability(name).is_some(), "{name}");
        }
        // A value of the described kind for every required parameter is
        // enough for dispatch, which takes exactly the described names.
        let info = params(rpc).unwrap();
        let mut source = Map::default();
        for p in info.iter().filter(|p| p.required) {
            let value = match p.kind {
                ParamKind::Str => ParamValue::Str("X".into()),
                ParamKind::Int => ParamValue::Int(1),
                ParamKind::Float => ParamValue::Float(1.0),
                ParamKind::Bool => ParamValue::Bool(true),
                ParamKind::Date => day("2024-06-03"),
                ParamKind::Time => ParamValue::Str("09:30:00".into()),
                ParamKind::Expiration => ParamValue::Str("*".into()),
                ParamKind::Symbols | ParamKind::StrList => ParamValue::Strings(vec!["X".into()]),
            };
            source = source.with(p.name, value);
        }
        let mut probe = Map::default();
        std::mem::swap(&mut probe.values, &mut source.values);
        let built = dispatch(rpc, probe, Describe);
        assert!(built.is_ok(), "{rpc}: {:?}", built.err());
        // Required parameters have no vendor default; optional ones have
        // it both as text and as a value, or neither.
        for p in info {
            assert!(!p.required || p.vendor_default.is_none(), "{rpc}");
            assert_eq!(
                p.vendor_default.is_some(),
                p.default_value().is_some(),
                "{rpc}"
            );
        }
    }
}

#[test]
fn defaults_are_the_vendor_text_and_descriptor_only_rpcs_stay_unstable() {
    let quote = params("option_history_quote").unwrap();
    let interval = quote.iter().find(|p| p.name == "interval").unwrap();
    assert_eq!(
        (interval.kind, interval.required, interval.vendor_default),
        (ParamKind::Str, false, Some("'1s'"))
    );
    assert_eq!(interval.default_value(), Some(ParamValue::Str("1s".into())));
    let unwrapped: Vec<_> = ENDPOINTS.iter().filter(|e| e.wrapper.is_none()).collect();
    assert_eq!(unwrapped.len(), 19);
    for endpoint in unwrapped {
        let rpc = endpoint.name();
        assert_eq!(rpc, endpoint.rpc);
        assert_eq!(stability(rpc), Some(Stability::Unstable), "{rpc}");
        assert!(
            params(rpc)
                .unwrap()
                .iter()
                .all(|p| !p.required && p.vendor_default.is_none())
        );
    }
    assert_eq!(
        stability("stock_history_eod"),
        Some(Stability::VendorWrapper)
    );
}

/// A value of `kind` for a required parameter.
fn sample(kind: ParamKind) -> ParamValue {
    match kind {
        ParamKind::Str => ParamValue::Str("X".into()),
        ParamKind::Int => ParamValue::Int(1),
        ParamKind::Float => ParamValue::Float(1.0),
        ParamKind::Bool => ParamValue::Bool(true),
        ParamKind::Date => day("2024-06-03"),
        ParamKind::Time => ParamValue::Str("09:30:00".into()),
        ParamKind::Expiration => ParamValue::Str("*".into()),
        _ => ParamValue::Strings(vec!["X".into()]),
    }
}

#[test]
fn every_typed_default_builds_the_same_request_as_omitting_it() {
    let mut checked = 0;
    for endpoint in ENDPOINTS {
        let info = params(endpoint.rpc).unwrap();
        let base = || {
            info.iter()
                .filter(|p| p.required)
                .fold(Map::default(), |m, p| m.with(p.name, sample(p.kind)))
        };
        let omitted = dispatch(endpoint.rpc, base(), Describe).unwrap();
        for p in info {
            let Some(value) = p.default_value() else {
                continue;
            };
            let explicit = dispatch(endpoint.rpc, base().with(p.name, value), Describe).unwrap();
            assert_eq!(explicit, omitted, "{} {}", endpoint.rpc, p.name);
            checked += 1;
        }
    }
    assert_eq!(checked, 583);
}

#[test]
fn time_defaults_are_resolved_text_and_times_are_sent_as_given() {
    let info = params("stock_history_quote").unwrap();
    let start = info.iter().find(|p| p.name == "start_time").unwrap();
    assert_eq!(start.vendor_default, Some("_fmt_time('09:30:00')"));
    assert_eq!(
        start.default_value(),
        Some(ParamValue::Time(TimeOfDay::Text("09:30:00".into())))
    );
    let request = |time: &str| {
        Map::default()
            .with("symbol", ParamValue::Str("AAPL".into()))
            .with("date", day("2024-06-03"))
            .with("interval", ParamValue::Str("1m".into()))
            .with("start_time", ParamValue::Str(time.into()))
    };
    let short = dispatch("stock_history_quote", request("09:30"), Describe)
        .unwrap()
        .1;
    let long = dispatch("stock_history_quote", request("09:30:00"), Describe)
        .unwrap()
        .1;
    assert!(short.contains("\"09:30\""), "{short}");
    assert_ne!(short, long);
}

/// Splits the request as `job()` would and reports the parts.
struct Planned;
impl Runner for Planned {
    type Output = Vec<String>;
    fn run<E>(self, request: E) -> Self::Output
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        crate::plan(&request)
            .iter()
            .map(|part| format!("{:?}", part.to_query()))
            .collect()
    }
}

#[test]
fn a_runner_can_plan_a_dispatched_request_as_a_job_would() {
    let tick = Map::default()
        .with("symbol", ParamValue::Str("SPY".into()))
        .with("expiration", ParamValue::Str("2024-06-21".into()))
        .with("start_date", day("2024-06-03"))
        .with("end_date", day("2024-06-07"))
        .with("interval", ParamValue::Str("tick".into()));
    let parts = dispatch("option_history_quote", tick, Planned).unwrap();
    assert_eq!(parts.len(), 5);
}
