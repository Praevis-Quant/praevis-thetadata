//! Builds against the staged archive with default features. Mirrors the README in-memory
//! example without contacting ThetaData; the default build must not pull native stores.
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Session};

#[allow(dead_code)]
async fn readme_example() -> Result<Session, Box<dyn std::error::Error>> {
    let auth = AuthClient::new(AuthConfig::default())?;
    Ok(auth.authenticate(&Credentials::api_key("your-api-key")).await?)
}

fn main() {
    AuthClient::new(AuthConfig::default()).expect("default config is valid");
    assert!(Credentials::from_file_contents("only-one-line").is_err());
    println!("auth-consumer: default (in-memory) praevis-thetadata-auth archive builds and links");
}
