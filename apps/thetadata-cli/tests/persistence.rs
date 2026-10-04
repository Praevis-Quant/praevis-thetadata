//! Explicit test against the current user's native credential store.
//! Uses a unique profile and synthetic credentials; never contacts ThetaData.
#![cfg(any(windows, target_os = "linux"))]

use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn cli(profile: &str, args: &[&str], credentials: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_theta"));
    command.args(["auth", "--profile", profile, "--environment", "stage"]);
    command.args(args);
    for variable in [
        "THETADATA_API_KEY",
        "THETADATA_EMAIL",
        "THETADATA_PASSWORD",
        "THETADATA_CREDENTIALS_FILE",
        "THETADATA_AUTH_URL",
        "THETADATA_MDDS_TYPE",
    ] {
        command.env_remove(variable);
    }
    if credentials {
        command.env("THETADATA_API_KEY", "synthetic-test-api-key");
    }
    command.output().unwrap()
}

struct Cleanup(String);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = cli(&self.0, &["logout"], false);
    }
}

#[test]
#[ignore = "writes a temporary synthetic session to the native credential store"]
fn session_survives_process_exit_without_login_credentials() {
    let profile = format!(
        "test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let _cleanup = Cleanup(profile.clone());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        assert!(String::from_utf8_lossy(&bytes).contains("synthetic-test-api-key"));
        let body = r#"{"sessionId":"synthetic-persisted-session","user":{"email":"test@example.com","stockSubscription":"TEST"}}"#;
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let login = cli(
        &profile,
        &[
            "--auth-url",
            &format!("http://{address}/auth"),
            "--insecure",
            "--json",
        ],
        true,
    );
    assert!(
        login.status.success(),
        "{}",
        String::from_utf8_lossy(&login.stderr)
    );
    server.join().unwrap();
    // The HTTP server and login process have exited, and this child has no credentials.
    let status = cli(&profile, &["status", "--json"], false);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(data["persisted"], true);
    assert_eq!(data["user"]["email"], "test@example.com");
    assert_eq!(data["validity"], "not_checked");
    assert_eq!(
        data["storage"],
        if cfg!(windows) {
            "Windows Credential Manager"
        } else {
            "Linux Secret Service"
        }
    );
    for output in [&login.stdout, &login.stderr, &status.stdout, &status.stderr] {
        let text = String::from_utf8_lossy(output);
        assert!(!text.contains("synthetic-test-api-key"));
        assert!(!text.contains("synthetic-persisted-session"));
    }
    // Same profile in PROD cannot see the STAGE session.
    let other = Command::new(env!("CARGO_BIN_EXE_theta"))
        .args([
            "auth",
            "status",
            "--profile",
            &profile,
            "--environment",
            "prod",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(other.status.code(), Some(3));
    assert!(cli(&profile, &["logout"], false).status.success());
    let missing = cli(&profile, &["status", "--json"], false);
    assert_eq!(missing.status.code(), Some(3));
    let data: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(data["persisted"], false);
}

#[cfg(target_os = "linux")]
#[test]
fn missing_secret_service_is_an_error_not_a_missing_session() {
    let output = Command::new(env!("CARGO_BIN_EXE_theta"))
        .args([
            "auth",
            "status",
            "--profile",
            "unavailable-service-test",
            "--json",
        ])
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent-thetadata-test-bus",
        )
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Linux Secret Service"));
    assert!(output.stdout.is_empty());
}
