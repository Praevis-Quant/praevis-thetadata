# praevis-thetadata-auth

Explicit HTTPS authentication for ThetaData, plus optional native session
persistence so separate processes can reuse a saved session.

> **Independent and experimental.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-alpha.2` is an early
> prerelease: APIs may change between alphas, and it is **not** a full port of, or
> drop-in replacement for, the vendor Python package. The Rust version is
> independent of the tracked Python compatibility baseline (currently 1.0.17).

## What it does

- `AuthClient::authenticate` sends an API key or email/password to the same
  HTTPS endpoint, with the same request shape, as the vendor Python package.
- `Credentials` are always supplied by the caller. The library never reads
  environment variables, `.env` files or credential files on its own.
- By default the returned `Session` lives **only in memory**, as in the vendor
  package; clone it to reuse it within your process. The default build has
  **no keyring, D-Bus or Windows credential dependencies**.
- **Optional `native-store` feature** (a Rust enhancement, off by default):
  `SessionStore` saves a versioned session record (session ID, account and
  subscription metadata, environment and save time) in the OS credential
  store, so other processes can reuse it. It never saves passwords or API keys.
  A saved session is **not** validated against the server; there is no
  refresh, expiry or revocation contract. See
  [ADR-0020](https://github.com/Praevis-Quant/praevis-thetadata/blob/main/docs/adr/0020-in-memory-sessions-and-optional-persistence.md).

## Install

Prereleases are never selected implicitly, so pin the version:

```toml
[dependencies]
praevis-thetadata-auth = "=0.1.0-alpha.2"
```

## Supported platforms

The default build (HTTP authentication and in-memory sessions) is pure Rust
with `reqwest`/rustls and works on any platform supported by Rust 1.88+.
`native-store` supports **Windows** (Credential Manager) and **Linux** (Secret
Service over D-Bus, for example GNOME Keyring; the keyring must be unlocked).
Other platforms get an explicit unsupported-platform error from its calls. On
Linux, `native-store` compiles a vendored D-Bus library and needs a C compiler
and `pkg-config`. CI covers Windows and Ubuntu.

## Example

```rust,no_run
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Session};

async fn session() -> Result<Session, Box<dyn std::error::Error>> {
    let auth = AuthClient::new(AuthConfig::default())?;
    Ok(auth.authenticate(&Credentials::api_key("your-api-key")).await?)
}
```

With `praevis-thetadata-auth = { version = "=0.1.0-alpha.2", features = ["native-store"] }`:

```rust,ignore
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Environment, Session, SessionStore};

async fn saved_session() -> Result<Session, Box<dyn std::error::Error>> {
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
`praevis-thetadata auth` (in memory) and `auth --save` (native store) on top of
this library.

## License

Apache-2.0. The package includes `LICENSE` and `NOTICE`. Source, issues,
requirements and design records: <https://github.com/Praevis-Quant/praevis-thetadata>.
