//! The emulator as a process, for the Python tests and manual use
//! (ADR-0022).
//!
//! ```text
//! praevis-thetadata-emulator [--profile free|options-pro] [--fault partial|slow|busy|delayed]
//!     [--time-scale F] [--recorded-sizes on] [--concurrency documented|observed]
//! ```
//!
//! It prints one JSON line, `{"auth_url": …, "endpoint": …}`, then serves
//! until its standard input closes. A line `requests` on standard input
//! prints `{"requests": N}`: the RPC requests received so far.
use praevis_thetadata_emulator::{Concurrency, Config, Emulator, Fault, Profile};
use std::io::{BufRead, Write};
use std::process::ExitCode;

const USAGE: &str = "usage: praevis-thetadata-emulator [--profile free|options-pro] [--fault partial|slow|busy|delayed] [--time-scale F] [--recorded-sizes on] [--concurrency documented|observed]";

fn main() -> ExitCode {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_default();
        let ok = match flag.as_str() {
            "--profile" => Profile::parse(&value).map(|p| config.profile = p),
            "--fault" => Fault::parse(&value).map(|f| config.fault = Some(f)),
            "--time-scale" => value
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite() && *f >= 0.0)
                .map(|f| config.time_scale = f),
            "--recorded-sizes" => (value == "on").then(|| config.recorded_sizes = true),
            "--concurrency" => match value.as_str() {
                "documented" => Some(Concurrency::Documented),
                "observed" => Some(Concurrency::Observed),
                _ => None,
            }
            .map(|c| config.concurrency = c),
            _ => None,
        };
        if ok.is_none() {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    }
    let emulator = Emulator::start(config);
    println!(
        "{}",
        serde_json::json!({"auth_url": emulator.auth_url, "endpoint": emulator.endpoint})
    );
    std::io::stdout().flush().expect("stdout");
    for line in std::io::stdin().lock().lines() {
        if line.as_deref().is_ok_and(|l| l.trim() == "requests") {
            println!(
                "{}",
                serde_json::json!({ "requests": emulator.requests().len() })
            );
            std::io::stdout().flush().expect("stdout");
        }
    }
    ExitCode::SUCCESS
}
