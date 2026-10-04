//! Process startup must not depend on Tokio's multithread worker configuration.
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_theta"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("THETADATA_")
        {
            command.env_remove(name);
        }
    }
    // Tokio's multithread builder rejects zero workers; synchronous commands and
    // the current-thread auth runtime must not consult this setting.
    command
        .env("TOKIO_WORKER_THREADS", "0")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_version_and_argument_errors_do_not_start_a_worker_pool() {
    for args in [&["--help"][..], &["auth", "--help"], &["--version"]] {
        let output = cli(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let output = cli(&["auth", "--timeout", "0"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn invalid_profile_is_an_operational_error_without_a_worker_pool() {
    let output = cli(&["auth", "status", "--profile", "../invalid", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("profile must be"));
}
