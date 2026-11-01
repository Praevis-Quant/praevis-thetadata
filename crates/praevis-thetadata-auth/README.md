# praevis-thetadata-auth

Explicit HTTPS authentication for ThetaData, plus optional native session
persistence so separate processes can reuse a saved session.

> **Independent and experimental.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-alpha.1` is an early
> prerelease: APIs may change between alphas, and it is **not** a full port of, or
> drop-in replacement for, the vendor Python package. The Rust version is
> independent of the tracked Python compatibility baseline (currently 1.0.17).

## What it does

- `AuthClient::authenticate` sends an API key or email/password to the same
  HTTPS endpoint, with the same request shape, as the vendor Python package.
- `Credentials` are always supplied by the caller. The library never reads
  environment variables, `.env` files or credential files on its own.
- `SessionStore` saves a versioned session record (session ID, account and
  subscription metadata, environment and save time) in the OS credential store.
  It never saves passwords or API keys. A saved session is **not** validated
  against the server; there is no refresh, expiry or revocation contract.

## Supported platforms

Authentication (HTTP) works on any platform supported by Rust and `reqwest` with
rustls. Native persistence supports **Windows** (Credential Manager) and
**Linux** (Secret Service over D-Bus, for example GNOME Keyring; the keyring
must be unlocked). Other platforms get an explicit unsupported-platform error
from persistence calls. CI covers Windows and Ubuntu. Linux builds compile a
vendored D-Bus library and need a C compiler and `pkg-config`.

## Example

```rust,no_run
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Environment, SessionStore};

async fn session() -> Result<praevis_thetadata_auth::Session, Box<dyn std::error::Error>> {
    let store = SessionStore::new("default", Environment::Prod)?;
    if let Some(saved) = store.load()? {
        return Ok(saved.into_session());
    }
    let auth = AuthClient::new(AuthConfig::default())?;
    let session = auth.authenticate(&Credentials::api_key("your-api-key")).await?;
    store.save(&session)?;
    Ok(session)
}
```

Never log `session.session_id()`. The `praevis-thetadata-cli` package provides
a ready-made `praevis-thetadata auth` command using this library.

## License

Apache-2.0. The package includes `LICENSE` and `NOTICE`. Source, issues,
requirements and design records: <https://github.com/Praevis-Quant/praevis-thetadata>.
