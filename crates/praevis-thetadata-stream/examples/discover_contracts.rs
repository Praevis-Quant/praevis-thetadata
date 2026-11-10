//! Contract discovery for the W10 latency runs (manual, `--features
//! live-tests`, `--confirm-live`, with the operator's Terminal running; no
//! login: the Terminal holds the credentials).
//!
//! The latency harness needs contracts that trade on the feed's day, and the
//! dev feed replays a past day the vendor does not name. This reads the full
//! trade stream (options PRO) for `--seconds`, counts trades per contract,
//! and writes the most-traded contracts as `contracts-top-<N>.txt` files in
//! the harness's format (`ROOT,YYYYMMDD,STRIKE_DOLLARS,C|P`). The trade
//! dates seen (the replay day on the dev feed) go in each file's header and
//! on the console.
//!
//! The files are local working lists under `--out-dir` (default
//! `artifacts/w10/contracts`, ignored): contract identifiers and counts,
//! never prices.
use clap::Parser;
use praevis_thetadata_stream::{Contract, Kind, Options, Right, Stream};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Cli {
    #[arg(long, required = true)]
    confirm_live: bool,
    /// How long to read the full trade stream.
    #[arg(long, default_value_t = 20)]
    seconds: u64,
    /// List sizes to write.
    #[arg(long, default_value = "10,100,1000", value_delimiter = ',')]
    top: Vec<usize>,
    /// Only contracts on this root (for example SPY).
    #[arg(long)]
    root: Option<String>,
    #[arg(long)]
    out_dir: Option<std::path::PathBuf>,
}

/// A strike in tenths of a cent as dollars, without trailing zeros
/// (`570000` is `570`, `5800500` is `5800.5`).
fn dollars(strike: i64) -> String {
    let (whole, frac) = (strike / 1000, (strike % 1000).abs());
    if frac == 0 {
        return whole.to_string();
    }
    let text = format!("{whole}.{frac:03}");
    text.trim_end_matches('0').to_owned()
}

fn line(contract: &Contract) -> String {
    format!(
        "{},{},{},{}",
        contract.root.as_str(),
        contract.expiration,
        dollars(contract.strike),
        match contract.right {
            Right::Call => "C",
            Right::Put => "P",
        }
    )
}

/// The `n` most-traded contracts: by trade count, then by their line, so the
/// order is deterministic.
fn ranked(counts: &HashMap<Contract, u64>, n: usize) -> Vec<(String, u64)> {
    let mut all: Vec<(String, u64)> = counts.iter().map(|(c, n)| (line(c), *n)).collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    all.truncate(n);
    all
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    if !cli.confirm_live || std::env::var_os("CI").is_some() {
        eprintln!("live execution requires --confirm-live, outside CI");
        return std::process::ExitCode::FAILURE;
    }
    let out_dir = cli.out_dir.clone().unwrap_or_else(|| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/w10/contracts")
    });
    let stream = match Stream::connect(Options::default()) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("stream: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    if let Err(error) = stream.subscribe_full_trades() {
        eprintln!("subscribe: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let start = Instant::now();
    let end = start + Duration::from_secs(cli.seconds);
    let mut counts: HashMap<Contract, u64> = HashMap::new();
    let mut dates: BTreeMap<u32, u64> = BTreeMap::new();
    let mut first_trade_ms = None;
    while Instant::now() < end {
        let Some(event) = stream.recv_timeout(end.saturating_duration_since(Instant::now())) else {
            continue;
        };
        match event.kind {
            Kind::Trade { contract, trade } => {
                first_trade_ms.get_or_insert(event.received.duration_since(start).as_millis());
                if cli
                    .root
                    .as_deref()
                    .is_none_or(|r| r == contract.root.as_str())
                {
                    *counts.entry(contract).or_default() += 1;
                    *dates.entry(trade.date).or_default() += 1;
                }
            }
            Kind::Disconnected => break,
            _ => {}
        }
    }
    // The Terminal's answer to the unsubscribe, if it sends one within 2 s
    // (live 2026-11-10: the full trade stream outlived this program).
    let unsubscribe = match stream.unsubscribe_full_trades() {
        Ok(id) => {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut answer = "none within 2 s".to_owned();
            while let Some(event) =
                stream.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                if let Kind::Response { id: got, response } = event.kind
                    && got == id as i64
                {
                    answer = format!("{response:?}");
                    break;
                }
            }
            answer
        }
        Err(error) => format!("not sent: {error}"),
    };
    eprintln!("unsubscribe answer: {unsubscribe}");
    let stats = stream.close();
    let roots: BTreeSet<&str> = counts.keys().map(|c| c.root.as_str()).collect();
    eprintln!(
        "{} trades on {} contracts ({} roots) in {} s; first trade after {:?} ms; trade dates {:?}; dropped {}",
        counts.values().sum::<u64>(),
        counts.len(),
        roots.len(),
        cli.seconds,
        first_trade_ms,
        dates,
        stats.dropped
    );
    if counts.is_empty() {
        eprintln!(
            "no trades seen: is the Terminal running, and does the account have options PRO?"
        );
        return std::process::ExitCode::FAILURE;
    }
    if let Err(error) = std::fs::create_dir_all(&out_dir) {
        eprintln!("output directory: {:?}", error.kind());
        return std::process::ExitCode::FAILURE;
    }
    let date_text = dates
        .keys()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    for n in &cli.top {
        let list = ranked(&counts, *n);
        let mut text = format!(
            "# {} most-traded option contracts over {} s of the full trade stream; trade date(s): {date_text}\n",
            list.len(),
            cli.seconds
        );
        for (contract, _) in &list {
            text.push_str(contract);
            text.push('\n');
        }
        let path = out_dir.join(format!("contracts-top-{n}.txt"));
        if let Err(error) = std::fs::write(&path, text) {
            eprintln!("write {}: {:?}", path.display(), error.kind());
            return std::process::ExitCode::FAILURE;
        }
        println!("Wrote {} ({} contracts)", path.display(), list.len());
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use praevis_thetadata_stream::Root;

    fn contract(root: &str, strike: i64) -> Contract {
        Contract {
            root: Root::new(root).unwrap(),
            expiration: 20250428,
            strike,
            right: Right::Call,
        }
    }

    #[test]
    fn strikes_round_trip_to_the_latency_harness_format() {
        assert_eq!(dollars(570_000), "570");
        assert_eq!(dollars(5_800_500), "5800.5");
        assert_eq!(dollars(12_345), "12.345");
        assert_eq!(line(&contract("SPY", 570_000)), "SPY,20250428,570,C");
    }

    #[test]
    fn ranking_is_by_count_then_by_line() {
        let counts: HashMap<Contract, u64> = [
            (contract("SPY", 570_000), 5),
            (contract("QQQ", 400_000), 9),
            (contract("AAPL", 200_000), 5),
        ]
        .into_iter()
        .collect();
        let top = ranked(&counts, 2);
        assert_eq!(
            top,
            [
                ("QQQ,20250428,400,C".to_owned(), 9),
                ("AAPL,20250428,200,C".to_owned(), 5)
            ]
        );
        assert_eq!(ranked(&counts, 10).len(), 3);
    }
}
