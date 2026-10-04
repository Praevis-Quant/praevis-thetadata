use clap::{Parser, Subcommand, ValueEnum};
use std::{
    error::Error,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};
use thetadata_auth::{AuthClient, AuthConfig, AuthError, Credentials, Environment, SessionStore};

#[derive(Parser)]
#[command(name = "theta", version, about = "ThetaData CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authenticate and persist a session, inspect it, or remove it
    Auth(AuthArgs),
}

#[derive(clap::Args)]
struct AuthArgs {
    #[command(subcommand)]
    action: Option<AuthAction>,
    /// Shared session name, scoped to the current OS user and environment
    #[arg(long, global = true, default_value = "default")]
    profile: String,
    /// Two-line email/password file; takes precedence over THETADATA_API_KEY
    #[arg(long, global = true, env = "THETADATA_CREDENTIALS_FILE")]
    creds_file: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "prod",
        env = "THETADATA_MDDS_TYPE",
        ignore_case = true
    )]
    environment: Env,
    #[arg(long, env = "THETADATA_AUTH_URL")]
    auth_url: Option<String>,
    /// Allow an HTTP authentication URL for local testing
    #[arg(long)]
    insecure: bool,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
    timeout: u64,
    /// Output account and subscription details as JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum AuthAction {
    /// Load the persisted session in this process; does not contact ThetaData
    Status,
    /// Delete this profile/environment's local session; does not revoke it remotely
    Logout,
}

#[derive(Clone, Copy, ValueEnum)]
enum Env {
    Prod,
    Stage,
}

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn credentials(args: &AuthArgs) -> Result<Credentials> {
    if let Some(path) = &args.creds_file {
        return Ok(Credentials::from_file(path)?);
    }
    if let Ok(key) = std::env::var("THETADATA_API_KEY")
        && !key.trim().is_empty()
    {
        return Ok(Credentials::api_key(key));
    }
    match (
        std::env::var("THETADATA_EMAIL"),
        std::env::var("THETADATA_PASSWORD"),
    ) {
        (Ok(email), Ok(password)) if !email.trim().is_empty() && !password.is_empty() => {
            return Ok(Credentials::email_password(email, password));
        }
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => {}
        _ => return Err("set both THETADATA_EMAIL and THETADATA_PASSWORD, or remove both".into()),
    }
    match Credentials::from_file("creds.txt") {
        Ok(credentials) => Ok(credentials),
        Err(AuthError::CredentialsFile(error)) if error.kind() == io::ErrorKind::NotFound => Err("set THETADATA_API_KEY, set THETADATA_EMAIL and THETADATA_PASSWORD, or supply --creds-file".into()),
        Err(error) => Err(error.into()),
    }
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let Command::Auth(args) = cli.command;
    let environment = match args.environment {
        Env::Prod => Environment::Prod,
        Env::Stage => Environment::Stage,
    };
    let store = SessionStore::new(&args.profile, environment)?;
    if let Some(action) = args.action {
        let mut out = io::stdout().lock();
        return match action {
            AuthAction::Status => match store.load()? {
                Some(stored) => {
                    if args.json {
                        serde_json::to_writer_pretty(
                            &mut out,
                            &serde_json::json!({
                                "persisted": true, "profile": store.profile(), "environment": environment,
                                "storage": store.backend_name(), "target": store.target(),
                                "savedAtUnix": stored.saved_at_unix(), "user": stored.session().user(), "validity": "not_checked"
                            }),
                        )?;
                        writeln!(out)?;
                    } else {
                        writeln!(
                            out,
                            "Stored session loaded for {} ({environment:?}, profile {})",
                            stored.session().user().email,
                            store.profile()
                        )?;
                        writeln!(out, "Storage: {}", store.backend_name())?;
                        writeln!(out, "Saved at: {} (Unix seconds)", stored.saved_at_unix())?;
                        writeln!(out, "Server validity: not checked")?;
                    }
                    out.flush()?;
                    Ok(ExitCode::SUCCESS)
                }
                None => {
                    if args.json {
                        writeln!(
                            out,
                            "{}",
                            serde_json::json!({"persisted":false,"profile":store.profile(),"environment":environment})
                        )?;
                    } else {
                        writeln!(
                            out,
                            "No stored session for profile {} ({environment:?}); run theta auth",
                            store.profile()
                        )?;
                    }
                    out.flush()?;
                    Ok(ExitCode::from(3))
                }
            },
            AuthAction::Logout => {
                let removed = store.clear()?;
                if args.json {
                    writeln!(
                        out,
                        "{}",
                        serde_json::json!({"removed":removed,"profile":store.profile(),"environment":environment})
                    )?;
                } else {
                    writeln!(
                        out,
                        "{}",
                        if removed {
                            "Stored session removed"
                        } else {
                            "No stored session to remove"
                        }
                    )?;
                }
                out.flush()?;
                Ok(ExitCode::SUCCESS)
            }
        };
    }
    let credentials = credentials(&args)?;
    let mut config = AuthConfig {
        environment,
        request_timeout: Duration::from_secs(args.timeout),
        allow_insecure: args.insecure,
        ..Default::default()
    };
    if let Some(url) = args.auth_url {
        config.auth_url = url;
    }
    let session = AuthClient::new(config)?.authenticate(&credentials).await?;
    store.save(&session)?;
    let mut out = io::stdout().lock();
    if args.json {
        serde_json::to_writer_pretty(
            &mut out,
            &serde_json::json!({ "authenticated": true, "persisted":true,
            "profile":store.profile(), "storage":store.backend_name(), "environment": session.environment(), "user": session.user() }),
        )?;
        writeln!(out)?;
    } else {
        writeln!(
            out,
            "Authenticated as {} ({:?})",
            session.user().email,
            session.environment()
        )?;
        writeln!(out, "Stocks: {}", session.user().stock_subscription)?;
        writeln!(out, "Options: {}", session.user().options_subscription)?;
        writeln!(out, "Indices: {}", session.user().indices_subscription)?;
        writeln!(
            out,
            "Session saved in {} (profile {})",
            store.backend_name(),
            store.profile()
        )?;
    }
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("theta: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_auth_options_and_rejects_zero_timeout() {
        let cli =
            Cli::try_parse_from(["theta", "auth", "--environment", "STAGE", "--json"]).unwrap();
        let Command::Auth(args) = cli.command;
        assert!(args.json);
        assert!(matches!(args.environment, Env::Stage));
        assert!(Cli::try_parse_from(["theta", "auth", "--timeout", "0"]).is_err());
    }
}
