use clap::{Parser, Subcommand, ValueEnum};
use praevis_thetadata_auth::{
    AuthClient, AuthConfig, AuthError, Credentials, Environment, SessionStore,
};
use std::{
    error::Error,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "praevis-thetadata",
    version,
    about = "Praevis ThetaData authentication CLI (independently maintained)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authenticate (optionally saving the session), inspect a saved session, or remove it
    Auth(AuthArgs),
}

#[derive(clap::Args)]
struct AuthArgs {
    #[command(subcommand)]
    action: Option<AuthAction>,
    /// Shared session name, scoped to the current OS user and environment
    #[arg(long, global = true, default_value = "default")]
    profile: String,
    /// Two-line email/password file; highest credential precedence
    #[arg(long, global = true)]
    creds_file: Option<PathBuf>,
    /// Also save the session in the native credential store (Windows Credential
    /// Manager or Linux Secret Service). Without it the session is not stored.
    #[arg(long)]
    save: bool,
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

/// Vendor (Python 1.0.12-1.0.17) credential order, ADR-0020 / AUTH-L3-025:
/// explicit file, API key, complete email/password pair, then the fallback file
/// (`THETADATA_CREDENTIALS_FILE`, otherwise `./creds.txt`). An incomplete or blank
/// pair falls through like Python's; a selected file that fails does not.
/// THETADATA_EMAIL/THETADATA_PASSWORD are a Rust-only extra (Python takes the pair
/// only as constructor arguments). No `.env` loading and no secret-bearing flags.
fn select_credentials(
    creds_file: Option<&Path>,
    var: impl Fn(&str) -> Option<String>,
    default_file: &Path,
) -> Result<Credentials> {
    if let Some(path) = creds_file {
        return Ok(Credentials::from_file(path)?);
    }
    if let Some(key) = var("THETADATA_API_KEY").filter(|key| !key.trim().is_empty()) {
        return Ok(Credentials::api_key(key));
    }
    let email = var("THETADATA_EMAIL").filter(|email| !email.trim().is_empty());
    let password = var("THETADATA_PASSWORD").filter(|password| !password.is_empty());
    let incomplete_pair = email.is_some() != password.is_some();
    if let (Some(email), Some(password)) = (email, password) {
        return Ok(Credentials::email_password(email, password));
    }
    let selected = var("THETADATA_CREDENTIALS_FILE").filter(|path| !path.is_empty());
    let fallback = selected.as_deref().map(Path::new).unwrap_or(default_file);
    match Credentials::from_file(fallback) {
        Ok(credentials) => Ok(credentials),
        Err(AuthError::CredentialsFile(error))
            if error.kind() == io::ErrorKind::NotFound && selected.is_none() =>
        {
            Err(if incomplete_pair {
                "only one of THETADATA_EMAIL and THETADATA_PASSWORD is set, and no ./creds.txt was found".into()
            } else {
                "no credentials: supply --creds-file, set THETADATA_API_KEY, set THETADATA_EMAIL and THETADATA_PASSWORD, set THETADATA_CREDENTIALS_FILE, or create ./creds.txt".into()
            })
        }
        Err(error) => Err(error.into()),
    }
}

fn credentials(args: &AuthArgs) -> Result<Credentials> {
    select_credentials(
        args.creds_file.as_deref(),
        |name| std::env::var(name).ok(),
        Path::new("creds.txt"),
    )
}

fn run(cli: Cli) -> Result<ExitCode> {
    let Command::Auth(args) = cli.command;
    let environment = match args.environment {
        Env::Prod => Environment::Prod,
        Env::Stage => Environment::Stage,
    };
    let store = SessionStore::new(&args.profile, environment)?;
    if args.save && args.action.is_some() {
        return Err(
            UsageError("--save applies only to authentication, not status or logout").into(),
        );
    }
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
                            "No stored session for profile {} ({environment:?}); run praevis-thetadata auth --save",
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
    // Only HTTP authentication needs async execution. Construct the client inside
    // the runtime, and drop the runtime before synchronous persistence/output.
    let session = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async { AuthClient::new(config)?.authenticate(&credentials).await })?;
    // Only an explicit --save touches the native store (ADR-0020). Saving happens after
    // a successful login, so a failed login never modifies an existing record.
    if args.save {
        store.save(&session)?;
    }
    let mut out = io::stdout().lock();
    if args.json {
        let mut output = serde_json::json!({ "authenticated": true, "persisted": args.save,
            "profile": store.profile(), "environment": session.environment(), "user": session.user() });
        if args.save {
            output["storage"] = store.backend_name().into();
        }
        serde_json::to_writer_pretty(&mut out, &output)?;
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
        if args.save {
            writeln!(
                out,
                "Session saved in {} (profile {})",
                store.backend_name(),
                store.profile()
            )?;
        } else {
            writeln!(
                out,
                "Session not saved; use --save to store it in {} (profile {})",
                store.backend_name(),
                store.profile()
            )?;
        }
    }
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

/// Argument combinations Clap cannot express; reported with Clap's exit code 2.
#[derive(Debug)]
struct UsageError(&'static str);
impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for UsageError {}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("praevis-thetadata: {error}");
            if error.is::<UsageError>() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Synthetic files in a unique directory; never the user's real credentials.
    struct Files(PathBuf);
    impl Files {
        fn new() -> Self {
            // Tests run in parallel threads of one process. A clock reading
            // alone can repeat between threads and let two tests share a
            // directory (and each other's creds.txt); the counter cannot.
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "praevis-cli-creds-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            // A directory left by an earlier, killed run with the same id.
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }
        fn file(&self, name: &str, email: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, format!("{email}\nsynthetic-password\n")).unwrap();
            path
        }
    }
    impl Drop for Files {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Which source won: "key:<value>", "pair:<email>" or "file:<email>".
    fn pick(
        flag: Option<&Path>,
        vars: &[(&str, &str)],
        default: &Path,
    ) -> std::result::Result<String, String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        select_credentials(flag, |name| vars.get(name).cloned(), default)
            .map(|credentials| match credentials {
                Credentials::ApiKey(key) => format!("key:{key}"),
                Credentials::EmailPassword { email, password }
                    if password == "synthetic-password" =>
                {
                    format!("file:{email}")
                }
                Credentials::EmailPassword { email, .. } => format!("pair:{email}"),
            })
            .map_err(|error| error.to_string())
    }

    #[test]
    fn credential_precedence_matches_the_vendor_order() {
        let files = Files::new();
        let flag = files.file("flag.txt", "flag@example.com");
        let env_file = files.file("env.txt", "env-file@example.com");
        let default = files.file("creds.txt", "default@example.com");
        let env_file_var = env_file.to_str().unwrap();
        let key = ("THETADATA_API_KEY", "synthetic-key");
        let email = ("THETADATA_EMAIL", "pair@example.com");
        let password = ("THETADATA_PASSWORD", "pair-password");
        let selected = ("THETADATA_CREDENTIALS_FILE", env_file_var);
        let everything = [key, email, password, selected];
        // 1. An explicit --creds-file wins over every other source.
        assert_eq!(
            pick(Some(&flag), &everything, &default),
            Ok("file:flag@example.com".into())
        );
        // 2. Then the API key.
        assert_eq!(
            pick(None, &everything, &default),
            Ok("key:synthetic-key".into())
        );
        // 3. Then a complete email/password pair.
        assert_eq!(
            pick(None, &[email, password, selected], &default),
            Ok("pair:pair@example.com".into())
        );
        // 4. Then THETADATA_CREDENTIALS_FILE, which beats ./creds.txt.
        assert_eq!(
            pick(None, &[selected], &default),
            Ok("file:env-file@example.com".into())
        );
        // 5. Finally ./creds.txt.
        assert_eq!(
            pick(None, &[], &default),
            Ok("file:default@example.com".into())
        );
    }

    #[test]
    fn blank_or_incomplete_sources_fall_through_like_python() {
        let files = Files::new();
        let default = files.file("creds.txt", "default@example.com");
        for vars in [
            &[("THETADATA_EMAIL", "pair@example.com")][..],
            &[("THETADATA_PASSWORD", "pair-password")],
            &[
                ("THETADATA_EMAIL", "pair@example.com"),
                ("THETADATA_PASSWORD", ""),
            ],
            &[
                ("THETADATA_EMAIL", "  "),
                ("THETADATA_PASSWORD", "pair-password"),
            ],
            &[
                ("THETADATA_API_KEY", "  "),
                ("THETADATA_CREDENTIALS_FILE", ""),
            ],
        ] {
            assert_eq!(
                pick(None, vars, &default),
                Ok("file:default@example.com".into()),
                "{vars:?}"
            );
        }
        // A blank key still lets a complete pair win.
        assert_eq!(
            pick(
                None,
                &[
                    ("THETADATA_API_KEY", ""),
                    ("THETADATA_EMAIL", "pair@example.com"),
                    ("THETADATA_PASSWORD", "p")
                ],
                &default
            ),
            Ok("pair:pair@example.com".into())
        );
    }

    #[test]
    fn a_selected_file_that_fails_does_not_fall_through() {
        let files = Files::new();
        let default = files.file("creds.txt", "default@example.com");
        let missing = files.0.join("missing.txt");
        let invalid = files.0.join("invalid.txt");
        std::fs::write(&invalid, "only-one-line\n").unwrap();
        let key = [("THETADATA_API_KEY", "synthetic-key")];
        for flag in [&missing, &invalid] {
            assert!(pick(Some(flag), &key, &default).is_err(), "{flag:?}");
        }
        // An env-selected fallback file is a selection too: ./creds.txt is not tried.
        let selected = [("THETADATA_CREDENTIALS_FILE", missing.to_str().unwrap())];
        let error = pick(None, &selected, &default).unwrap_err();
        assert!(error.contains("could not read credentials file"), "{error}");
        assert!(!error.contains("synthetic"), "{error}");
    }

    #[test]
    fn missing_default_file_explains_the_options() {
        let files = Files::new();
        let absent = files.0.join("creds.txt");
        let error = pick(None, &[], &absent).unwrap_err();
        assert!(
            error.contains("THETADATA_API_KEY") && error.contains("./creds.txt"),
            "{error}"
        );
        let error = pick(None, &[("THETADATA_EMAIL", "pair@example.com")], &absent).unwrap_err();
        assert!(
            error.contains("only one of THETADATA_EMAIL and THETADATA_PASSWORD"),
            "{error}"
        );
        assert!(!error.contains("pair@example.com"), "{error}");
    }

    #[test]
    fn save_is_rejected_for_local_actions() {
        for action in ["status", "logout"] {
            let cli = Cli::try_parse_from(["praevis-thetadata", "auth", "--save", action]).unwrap();
            let error = run(cli).unwrap_err();
            assert!(error.is::<UsageError>(), "{action}");
        }
    }
    #[test]
    fn parses_auth_options_and_rejects_zero_timeout() {
        let cli = Cli::try_parse_from([
            "praevis-thetadata",
            "auth",
            "--environment",
            "STAGE",
            "--json",
        ])
        .unwrap();
        let Command::Auth(args) = cli.command;
        assert!(args.json);
        assert!(!args.save);
        assert!(matches!(args.environment, Env::Stage));
        assert!(Cli::try_parse_from(["praevis-thetadata", "auth", "--timeout", "0"]).is_err());
    }
}
