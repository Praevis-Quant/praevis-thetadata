//! Plain `auth` keeps the session in memory and never touches the native store (ADR-0020).
//! Runs in ordinary CI: synthetic identity server, no keyring, never contacts ThetaData.
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
};

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_praevis-thetadata"));
    for (name, _) in std::env::vars() {
        if name.starts_with("THETADATA_") {
            command.env_remove(name);
        }
    }
    // Any Secret Service access would fail against this bus, so success proves no store access.
    command.env(
        "DBUS_SESSION_BUS_ADDRESS",
        "unix:path=/nonexistent-thetadata-test-bus",
    );
    command
}

#[test]
fn auth_without_save_reports_the_account_and_stores_nothing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        while !String::from_utf8_lossy(&request).contains("synthetic-no-save-key") {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0, "request ended without the synthetic API key");
            request.extend_from_slice(&buffer[..count]);
        }
        let body =
            r#"{"sessionId":"synthetic-memory-session","user":{"email":"memory@example.com"}}"#;
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let output = command()
        .args([
            "auth",
            "--profile",
            "no-save-test",
            "--environment",
            "stage",
            "--auth-url",
            &format!("http://{address}/auth"),
            "--insecure",
            "--json",
        ])
        .env("THETADATA_API_KEY", "synthetic-no-save-key")
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["authenticated"], true);
    assert_eq!(data["persisted"], false);
    assert!(data.get("storage").is_none());
    assert_eq!(data["user"]["email"], "memory@example.com");
    for stream in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(stream);
        assert!(!text.contains("synthetic-no-save-key"));
        assert!(!text.contains("synthetic-memory-session"));
    }
}

#[test]
fn save_with_a_local_action_is_an_argument_error() {
    for action in ["status", "logout"] {
        let output = command().args(["auth", "--save", action]).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{action}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("--save"));
        assert!(output.stdout.is_empty());
    }
}
