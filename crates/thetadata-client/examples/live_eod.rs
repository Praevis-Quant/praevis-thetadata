//! Explicitly invoked service verification; never part of ordinary CI.
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod fixture;
mod live_support;

use clap::{Args, Parser, Subcommand};
use live_support::{Capture, Query, capture, replay, run_directory, verify_live};
use std::path::PathBuf;
use thetadata_client::{AuthClient, AuthConfig, Credentials, Environment};

#[derive(Parser)]
#[command(about = "Manual ThetaData service checks and EOD replay (private artifacts)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// One authentication and up to three EOD calls; no persistence or retries.
    Capture(Live),
    /// Assert PERMISSION_DENIED for one stock quote request before any data.
    ProbeStockQuote(QuoteProbe),
    /// Recheck hashes and replay a completed capture using loopback only.
    Replay {
        #[arg(long)]
        run_id: String,
    },
}

#[derive(Args)]
#[group(id = "credentials", required = true, multiple = false)]
struct CredentialSources {
    /// Read THETADATA_API_KEY from this process environment (no automatic fallback).
    #[arg(long)]
    api_key_env: bool,
    /// Existing two-line email/password file. Never pass secrets as arguments.
    #[arg(long)]
    credentials_file: Option<PathBuf>,
    /// Existing file containing only the API key.
    #[arg(long)]
    api_key_file: Option<PathBuf>,
}

#[derive(Args)]
struct Live {
    #[arg(long, required = true)]
    confirm_live: bool,
    #[arg(long, value_parser = ["PROD", "STAGE"])]
    environment: String,
    #[command(flatten)]
    credentials: CredentialSources,
    #[arg(long)]
    run_id: String,
    #[arg(long)]
    symbol: String,
    #[arg(long)]
    start: String,
    #[arg(long)]
    end: String,
}

#[derive(Args)]
struct QuoteProbe {
    #[arg(long, required = true)]
    confirm_live: bool,
    #[arg(long, required = true)]
    expect_permission_denied: bool,
    #[arg(long, value_parser = ["PROD", "STAGE"])]
    environment: String,
    #[command(flatten)]
    credentials: CredentialSources,
    #[arg(long)]
    run_id: String,
    #[arg(long)]
    symbol: String,
    /// Operator-declared stock tier; not inferred from opaque account metadata.
    #[arg(long, value_parser = ["FREE", "VALUE", "STANDARD", "PRO"])]
    stock_tier: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let result = run(Cli::parse()).await;
    match result {
        Ok(()) => {
            println!("Verification passed; private evidence is in artifacts/live/<run-id>.");
            std::process::ExitCode::SUCCESS
        }
        Err(category) => {
            // Only fixed categories produced by this runner. Never Debug vendor errors.
            eprintln!(
                "Verification failed: {category}. Inspect the private run manifest, if created."
            );
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), &'static str> {
    match cli.command {
        Command::ProbeStockQuote(options) => {
            check_live_consent(
                options.confirm_live && options.expect_permission_denied,
                std::env::var_os("CI").is_some(),
            )?;
            let mut report = live_support::probe::Report::new(
                &options.environment,
                &options.symbol,
                &options.stock_tier,
            )?;
            let directory = run_directory(&options.run_id)?;
            live_support::create_directory(&directory)?;
            live_support::write_report(&directory, "manifest.json", &report)?;
            let result = async {
                let session = authenticate(
                    options.credentials,
                    &options.environment,
                    &mut report.auth_http_status,
                )
                .await?;
                report.observe(session).await
            }
            .await;
            report.verification = match result {
                Ok(()) => "expected_denial_passed",
                Err(reason) => reason,
            }
            .into();
            live_support::write_report(&directory, "manifest.json", &report)?;
            result?;
        }
        Command::Replay { run_id } => {
            let directory = run_directory(&run_id)?;
            let capture = Capture::load(&directory)?;
            replay(&capture, &directory).await?;
        }
        Command::Capture(options) => {
            check_live_consent(options.confirm_live, std::env::var_os("CI").is_some())?;
            let query = Query {
                symbol: options.symbol,
                start: options.start,
                end: options.end,
            };
            query.typed()?;
            let directory = run_directory(&options.run_id)?;
            // Reserve a new directory before reading credentials or contacting a host.
            live_support::create_directory(&directory)?;
            let mut evidence = Capture::new(query, &options.environment)?;
            evidence.save(&directory)?;
            let result = async {
                let session = authenticate(
                    options.credentials,
                    &options.environment,
                    &mut evidence.auth_http_status,
                )
                .await?;
                capture(&mut evidence, &directory, session.clone()).await?;
                let report = replay(&evidence, &directory).await?;
                let live = verify_live(&evidence, session, &report).await?;
                live_support::write_report(&directory, "live.json", &live)?;
                if !live.matches {
                    return Err(
                        "live values differ; investigate service revisions or client behavior",
                    );
                }
                Ok(())
            }
            .await;
            evidence.verification = match result {
                Ok(()) => "passed".into(),
                Err(category) => category.into(),
            };
            evidence.save(&directory)?;
            result?;
        }
    }
    Ok(())
}

fn check_live_consent(confirmed: bool, in_ci: bool) -> Result<(), &'static str> {
    if !confirmed || in_ci {
        return Err("live execution requires explicit consent outside CI");
    }
    Ok(())
}

async fn authenticate(
    sources: CredentialSources,
    environment: &str,
    http_status: &mut Option<u16>,
) -> Result<thetadata_client::Session, &'static str> {
    let credentials = if sources.api_key_env {
        environment_credentials(std::env::var("THETADATA_API_KEY").ok())?
    } else if let Some(path) = sources.api_key_file {
        let bytes = live_support::read_limited(&path, 16384)?;
        let key = std::str::from_utf8(&bytes).map_err(|_| "credential file")?;
        if key.trim().is_empty() || key.trim().chars().any(char::is_control) {
            return Err("credential file");
        }
        Credentials::api_key(key.trim())
    } else {
        let path = sources.credentials_file.ok_or("credential file")?;
        let bytes = live_support::read_limited(&path, 16384)?;
        Credentials::from_file_contents(std::str::from_utf8(&bytes).map_err(|_| "credential file")?)
            .map_err(|_| "credential file")?
    };
    let auth = AuthClient::new(AuthConfig {
        environment: if environment == "PROD" {
            Environment::Prod
        } else {
            Environment::Stage
        },
        ..Default::default()
    })
    .map_err(|_| "authentication configuration")?;
    auth.authenticate(&credentials).await.map_err(|error| {
        if let thetadata_auth::AuthError::Rejected(status) = error {
            *http_status = Some(status.as_u16());
            "authentication rejected; see manifest HTTP status"
        } else {
            "authentication transport or response"
        }
    })
}

fn environment_credentials(value: Option<String>) -> Result<Credentials, &'static str> {
    let key = value.ok_or("THETADATA_API_KEY is missing or not Unicode")?;
    if key.len() > 16384 || key.trim().is_empty() || key.trim().chars().any(char::is_control) {
        return Err("THETADATA_API_KEY is invalid");
    }
    Ok(Credentials::api_key(key.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_command_requires_consent_environment_and_exactly_one_credential_source() {
        assert!(check_live_consent(true, true).is_err());
        assert!(check_live_consent(false, false).is_err());
        assert!(check_live_consent(true, false).is_ok());
        let args = [
            "live_eod",
            "capture",
            "--environment",
            "PROD",
            "--run-id",
            "test",
            "--symbol",
            "AAPL",
            "--start",
            "2024-01-02",
            "--end",
            "2024-01-02",
            "--api-key-file",
            "not-read.credentials",
        ];
        assert!(Cli::try_parse_from(args).is_err());
        let mut consent = args.to_vec();
        consent.push("--confirm-live");
        assert!(Cli::try_parse_from(&consent).is_ok());
        let mut environment = consent[..consent.len() - 3].to_vec();
        environment.extend(["--api-key-env", "--confirm-live"]);
        assert!(Cli::try_parse_from(&environment).is_ok());
        environment.extend(["--api-key-file", "not-read.credentials"]);
        assert!(Cli::try_parse_from(&environment).is_err());
        consent.extend(["--credentials-file", "also-not-read.credentials"]);
        assert!(Cli::try_parse_from(&consent).is_err());
        assert!(Cli::try_parse_from(["live_eod", "replay", "--run-id", "test"]).is_ok());
        let mut probe = vec![
            "live_eod",
            "probe-stock-quote",
            "--confirm-live",
            "--environment",
            "PROD",
            "--api-key-env",
            "--stock-tier",
            "FREE",
            "--symbol",
            "AAPL",
            "--run-id",
            "test",
        ];
        assert!(Cli::try_parse_from(&probe).is_err());
        probe.push("--expect-permission-denied");
        assert!(Cli::try_parse_from(&probe).is_ok());
    }

    #[test]
    fn environment_key_validation_never_echoes_the_value() {
        for value in [
            None,
            Some(String::new()),
            Some("  ".into()),
            Some("SECRET\nSECOND-LINE".into()),
            Some("X".repeat(16385)),
        ] {
            assert!(environment_credentials(value).is_err());
        }
        let key = environment_credentials(Some("synthetic-env-key".into())).unwrap();
        assert_eq!(format!("{key:?}"), "Credentials([REDACTED])");
    }
}
