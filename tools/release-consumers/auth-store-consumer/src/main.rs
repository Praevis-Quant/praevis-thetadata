//! Builds the README `native-store` example against the staged archive. It only constructs
//! a store and reads its identity; it never reads or writes the OS credential store.
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Environment, Session, SessionStore};

#[allow(dead_code)]
async fn readme_example() -> Result<Session, Box<dyn std::error::Error>> {
    let store = SessionStore::new("default", Environment::Prod)?;
    if let Some(saved) = store.load()? {
        return Ok(saved.into_session());
    }
    let auth = AuthClient::new(AuthConfig::default())?;
    let session = auth.authenticate(&Credentials::api_key("your-api-key")).await?;
    store.save(&session)?;
    Ok(session)
}

fn main() {
    let store = SessionStore::new("release-consumer", Environment::Stage).unwrap();
    assert_eq!(store.target(), "praevis.thetadata.auth.v1:STAGE:release-consumer");
    assert!(SessionStore::new("../invalid", Environment::Prod).is_err());
    println!("auth-store-consumer: native-store archive builds; identity {}", store.target());
}
