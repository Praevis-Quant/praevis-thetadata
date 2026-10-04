use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn mock_server(
    status: &str,
    body: &str,
    delay: Duration,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let n = socket.read(&mut buffer).await.unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..n]);
            if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        tokio::time::sleep(delay).await;
        let _ = socket.write_all(response.as_bytes()).await;
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}/auth"), task)
}

fn config(url: String) -> AuthConfig {
    AuthConfig {
        auth_url: url,
        allow_insecure: true,
        environment: Environment::Stage,
        ..Default::default()
    }
}
const RESPONSE: &str = r#"{"sessionId":"fake-session-token","user":{"email":"test@example.com","stockSubscription":"PRO","optionsSubscription":null,"indicesSubscription":{"tier":"VALUE"}}}"#;

#[tokio::test]
async fn sends_api_key_and_parses_account_without_exposing_session() {
    let (url, task) = mock_server("200 OK", RESPONSE, Duration::ZERO).await;
    let auth = AuthClient::new(config(url)).unwrap();
    let session = auth
        .authenticate(&Credentials::api_key("fake-api-key"))
        .await
        .unwrap();
    assert_eq!(session.session_id(), "fake-session-token");
    assert_eq!(session.user().stock_subscription, "PRO");
    assert_eq!(session.environment(), Environment::Stage);
    assert!(!format!("{session:?}").contains("fake-session-token"));
    let request = task.await.unwrap();
    assert!(
        request
            .to_lowercase()
            .contains(&format!("td-terminal-key: {TERMINAL_KEY}"))
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"apiKey":"fake-api-key","authEnv":{"envType":"STAGE"}})
    );
}

#[tokio::test]
async fn email_password_auth_preserves_explicit_password_whitespace() {
    let (url, task) = mock_server("200 OK", RESPONSE, Duration::ZERO).await;
    AuthClient::new(config(url))
        .unwrap()
        .authenticate(&Credentials::email_password(
            " test@example.com ",
            " secret ",
        ))
        .await
        .unwrap();
    let request = task.await.unwrap();
    let body: serde_json::Value =
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["email"], "test@example.com");
    assert_eq!(body["password"], " secret ");
    assert!(body.get("apiKey").is_none());
}

#[tokio::test]
async fn rejects_http_errors_without_echoing_server_secrets() {
    for status in [
        "401 Unauthorized",
        "403 Forbidden",
        "302 Found",
        "500 Internal Server Error",
    ] {
        let (url, task) =
            mock_server(status, "fake-api-key secret error body", Duration::ZERO).await;
        let error = AuthClient::new(config(url))
            .unwrap()
            .authenticate(&Credentials::api_key("fake-api-key"))
            .await
            .unwrap_err();
        assert!(matches!(error, AuthError::Rejected(_)));
        assert!(!error.to_string().contains("fake-api-key"));
        task.await.unwrap();
    }
}

#[tokio::test]
async fn rejects_malformed_and_empty_sessions() {
    for body in [
        "not json",
        r#"{"sessionId":"","user":{"email":"test@example.com"}}"#,
        r#"{"sessionId":"token","user":{"email":""}}"#,
        "{}",
    ] {
        let (url, task) = mock_server("200 OK", body, Duration::ZERO).await;
        assert!(
            AuthClient::new(config(url))
                .unwrap()
                .authenticate(&Credentials::api_key("fake-key"))
                .await
                .is_err()
        );
        task.await.unwrap();
    }
}

#[tokio::test]
async fn enforces_http_timeout() {
    let (url, task) = mock_server("200 OK", RESPONSE, Duration::from_millis(250)).await;
    let mut config = config(url);
    config.request_timeout = Duration::from_millis(50);
    let error = AuthClient::new(config)
        .unwrap()
        .authenticate(&Credentials::api_key("fake-key"))
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::Http(ref error) if error.is_timeout()));
    task.abort();
}

#[test]
fn validates_urls_credentials_and_redaction() {
    assert!(
        AuthClient::new(AuthConfig {
            auth_url: "http://localhost".into(),
            ..Default::default()
        })
        .is_err()
    );
    assert!(
        AuthClient::new(AuthConfig {
            auth_url: "https://user:secret@example.com".into(),
            ..Default::default()
        })
        .is_err()
    );
    assert!(Credentials::from_file_contents("test@example.com\n").is_err());
    let credentials = Credentials::from_file_contents("test@example.com\r\nsecret\r\n").unwrap();
    assert!(!format!("{credentials:?}").contains("secret"));
    assert!(Credentials::api_key(" ").validate().is_err());
}
