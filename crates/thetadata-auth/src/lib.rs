//! Standalone ThetaData authentication. No gRPC, market-data, or CLI dependencies.
//!
//! ```no_run
//! use thetadata_auth::{AuthClient, AuthConfig, Credentials};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let auth = AuthClient::new(AuthConfig::default())?;
//! let session = auth.authenticate(&Credentials::api_key("your-key")).await?;
//! println!("Authenticated as {}", session.user().email);
//! // session.session_id() is available for authenticated API requests.
//! # Ok(()) }
//! ```

mod store;
pub use store::{SessionStore, StoreError, StoredSession};

use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

pub const DEFAULT_AUTH_URL: &str = "https://nexus-api.thetadata.us/identity/terminal/auth_user";
// Public client identifier from Python 1.0.12, not a user's secret.
const TERMINAL_KEY: &str = "cf58ada4-4175-11f0-860f-1e2e95c79e64";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid authentication configuration: {0}")]
    Config(&'static str),
    #[error("credentials must contain a nonempty API key or email and password")]
    InvalidCredentials,
    #[error("could not read credentials file: {0}")]
    CredentialsFile(#[from] std::io::Error),
    #[error("authentication failed (HTTP {0})")]
    Rejected(reqwest::StatusCode),
    #[error("authentication request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("authentication response is missing a session ID or account email")]
    InvalidResponse,
}

/// Debug always redacts credentials. No implicit environment or file discovery.
#[derive(Clone)]
pub enum Credentials {
    ApiKey(String),
    EmailPassword { email: String, password: String },
}

impl Credentials {
    pub fn api_key(key: impl Into<String>) -> Self {
        Self::ApiKey(key.into())
    }
    pub fn email_password(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self::EmailPassword {
            email: email.into(),
            password: password.into(),
        }
    }
    /// Python/Theta Terminal format: email on line one, password on line two.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, AuthError> {
        Self::from_file_contents(&std::fs::read_to_string(path)?)
    }
    pub fn from_file_contents(contents: &str) -> Result<Self, AuthError> {
        let mut lines = contents.lines();
        let credentials = Self::email_password(
            lines.next().unwrap_or("").trim(),
            lines.next().unwrap_or("").trim(),
        );
        credentials.validate()?;
        Ok(credentials)
    }
    fn validate(&self) -> Result<(), AuthError> {
        match self {
            Self::ApiKey(key) if !key.trim().is_empty() => Ok(()),
            Self::EmailPassword { email, password }
                if !email.trim().is_empty() && !password.is_empty() =>
            {
                Ok(())
            }
            _ => Err(AuthError::InvalidCredentials),
        }
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Environment {
    #[serde(rename = "PROD")]
    Prod,
    #[serde(rename = "STAGE")]
    Stage,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub environment: Environment,
    pub auth_url: String,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    /// Opt in only for local HTTP test servers.
    pub allow_insecure: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            environment: Environment::Prod,
            auth_url: DEFAULT_AUTH_URL.into(),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            allow_insecure: false,
        }
    }
}

/// Subscription payloads remain JSON because the bundled protocol does not define their shape.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub email: String,
    #[serde(default)]
    pub stock_subscription: serde_json::Value,
    #[serde(default)]
    pub options_subscription: serde_json::Value,
    #[serde(default)]
    pub indices_subscription: serde_json::Value,
}

/// Share or clone in memory. Debug redacts the token; no automatic persistence or serialization.
#[derive(Clone)]
pub struct Session {
    session_id: String,
    user: User,
    environment: Environment,
}

impl Session {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn user(&self) -> &User {
        &self.user
    }
    pub fn environment(&self) -> Environment {
        self.environment
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Session([REDACTED])")
    }
}

#[derive(Clone)]
pub struct AuthClient {
    http: reqwest::Client,
    url: reqwest::Url,
    environment: Environment,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
    auth_env: AuthEnvironment,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthEnvironment {
    env_type: Environment,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    session_id: String,
    user: User,
}

impl AuthClient {
    pub fn new(config: AuthConfig) -> Result<Self, AuthError> {
        if config.connect_timeout.is_zero() || config.request_timeout.is_zero() {
            return Err(AuthError::Config("timeouts must be positive"));
        }
        let url = reqwest::Url::parse(&config.auth_url)
            .map_err(|_| AuthError::Config("invalid auth URL"))?;
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AuthError::Config(
                "URL requires a host and cannot contain credentials, a query, or a fragment",
            ));
        }
        if url.scheme() != "https" && !(url.scheme() == "http" && config.allow_insecure) {
            return Err(AuthError::Config(
                "HTTPS required; use allow_insecure only for local testing",
            ));
        }
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            url,
            environment: config.environment,
        })
    }
    pub async fn authenticate(&self, credentials: &Credentials) -> Result<Session, AuthError> {
        credentials.validate()?;
        let mut body = AuthRequest {
            api_key: None,
            email: None,
            password: None,
            auth_env: AuthEnvironment {
                env_type: self.environment,
            },
        };
        match credentials {
            Credentials::ApiKey(key) => body.api_key = Some(key),
            Credentials::EmailPassword { email, password } => {
                body.email = Some(email.trim());
                body.password = Some(password);
            }
        }
        let response = self
            .http
            .post(self.url.clone())
            .header("TD-TERMINAL-KEY", TERMINAL_KEY)
            .json(&body)
            .send()
            .await?;
        // Never echo response bodies: a server can include credentials or tokens in errors.
        if !response.status().is_success() {
            return Err(AuthError::Rejected(response.status()));
        }
        let response: AuthResponse = response.json().await?;
        if response.session_id.trim().is_empty() || response.user.email.trim().is_empty() {
            return Err(AuthError::InvalidResponse);
        }
        Ok(Session {
            session_id: response.session_id,
            user: response.user,
            environment: self.environment,
        })
    }
}

#[cfg(test)]
mod tests;
