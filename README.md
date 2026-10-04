# ThetaData Rust

Rust workspace derived from the bundled ThetaData Python **1.0.12** package. The first application is an authentication CLI, backed by an authentication library that other projects can use independently.

See the [auth requirements and ADRs](docs/README.md), [roadmap](docs/ROADMAP.md), [vendor research](docs/research/thetadata-1.0.12.md), and [performance methodology](docs/performance/auth.md). Only authentication is a baselined feature scope; the market-data crates are foundations for later work.

For implemented changes, see the [changelog](CHANGELOG.md). Coding agents should start with [AGENTS.md](AGENTS.md) and the shared [project guidance and Git conventions](CLAUDE.md).

Newly authored code and tooling are licensed under [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution and [the preserved upstream license](crates/thetadata-proto/LICENSE.upstream) for the bundled protocol source.

## Workspace

| Package | Responsibility | Status |
| --- | --- | --- |
| `thetadata-auth` | HTTP authentication, credentials, sessions, Windows/Linux session persistence | First deliverable |
| `thetadata-cli` (`theta`) | Credential discovery and auth/status/logout commands | First deliverable |
| `thetadata-proto` | Generated protobuf types and gRPC bindings for all 82 RPCs | Foundation |
| `thetadata-core` | Transport-independent prices, timestamps, values, tables | Foundation |
| `thetadata-client` | gRPC requests, ZSTD decoding, bounded batch streaming | Foundation; not verified against the live service |

Dependency direction:

```text
theta CLI ───────────────> thetadata-auth
thetadata-client ────────> thetadata-auth
                ├───────> thetadata-core
                └───────> thetadata-proto
```

Authentication needs no gRPC, dataframe, or market-data dependencies. The CLI currently depends only on authentication. Stocks, options, indices, calendars, and interest rates belong in modules of the same client library; they do not need individual crates. Add other applications under `apps/` as needed.

## Build and authenticate

Requires a current stable Rust toolchain. On Windows, install the Visual Studio C++ build tools. On Linux, install a C/C++ compiler, make, and pkg-config; the Linux backend builds its bundled D-Bus library. A full workspace build also compiles ZSTD. The CLI alone does not build ZSTD or protobuf. Normal builds need neither Python nor `protoc`.

```powershell
cargo build -p thetadata-cli
# With THETADATA_API_KEY already set in your environment:
.\target\debug\theta.exe auth

# Or use a Theta Terminal compatible two-line credentials file:
.\target\debug\theta.exe auth --creds-file C:\private\creds.txt

# Run this in a second terminal/process without supplying login credentials:
.\target\debug\theta.exe auth status
.\target\debug\theta.exe auth status --json

# Delete the saved local session:
.\target\debug\theta.exe auth logout
```

On Linux (Ubuntu/Debian):

```bash
sudo apt-get install build-essential pkg-config dbus gnome-keyring
cargo build --locked -p thetadata-cli
./target/debug/theta auth --creds-file /private/creds.txt
./target/debug/theta auth status --json
./target/debug/theta auth logout
```

Linux persistence uses **Secret Service** over the user's D-Bus session, backed by a persistent keyring such as GNOME Keyring. The default keyring must be unlocked. GNOME desktop login commonly handles this through PAM; see [GNOME Keyring login integration](https://wiki.gnome.org/Projects/GnomeKeyring/Pam). Status reports `Linux Secret Service` on Linux and `Windows Credential Manager` on Windows. Both platforms use the same commands and library API.

For a headless machine or WSL without a desktop session, start a user bus and unlock a persistent keyring before running the CLI:

```bash
dbus-run-session -- bash
# Run the remaining commands inside that shell. Choose a password when creating
# the keyring; use that same password when unlocking it on later logins.
read -r -s -p 'Keyring password: ' keyring_password; echo
printf '%s' "$keyring_password" | gnome-keyring-daemon --unlock --components=secrets
unset keyring_password
./target/debug/theta auth --creds-file /private/creds.txt
./target/debug/theta auth status
```

Services and containers need access to an unlocked Secret Service and persistent keyring storage under the same OS account. GNOME Keyring normally stores its encrypted keyrings under `$XDG_DATA_HOME/keyrings` (default `~/.local/share/keyrings`); preserve that storage across container replacements. The CLI does not save the keyring unlock password or automatically start/unlock the service. An unavailable service returns an actionable error, distinct from “no saved session”; there is no plaintext or process-only fallback.

Credential precedence for `theta auth`:

1. `--creds-file` or `THETADATA_CREDENTIALS_FILE` (email on line one, password on line two).
2. `THETADATA_API_KEY`.
3. Both `THETADATA_EMAIL` and `THETADATA_PASSWORD`.
4. `./creds.txt`.

The CLI does not load `.env` files automatically. Credentials and tokens are never printed. `--json` emits account/subscription details and storage status. Exit codes: **0** success, **1** operational/authentication error, **2** invalid command arguments, **3** no persisted session when checking status.

`--environment prod|stage` defaults to PROD; `THETADATA_MDDS_TYPE` is also supported. `--profile NAME` defaults to `default`. These options also work with `status` and `logout`:

```powershell
.\target\debug\theta.exe auth --profile research --environment stage
.\target\debug\theta.exe auth status --profile research --environment stage --json
```

## How authentication persists

1. `theta auth` submits credentials to the same HTTPS authentication endpoint and with the same request shape as Python 1.0.12.
2. After a successful response, `SessionStore::save` stores a versioned JSON record **inside the native credential store**, not in a project file. The record contains the session ID, account/subscription metadata, environment, and save time. The API key and password are not stored.
3. The stable identifier is `praevis.thetadata.auth.v1:PROD:default` for the default production profile. Windows uses it as the credential target. Linux uses it as the Secret Service `service` attribute, with the profile as `username`, in the default collection. Changing the profile or environment selects a different entry. All applications using this library and identifier share that record under the same OS user and credential service. Windows and Linux stores are independent; sessions are not automatically synchronized between them.
4. `theta auth status` starts independently and reads the record from the native store. It does not authenticate, require login credentials, or contact ThetaData. It reports the account, save time, and storage location without exposing the token.
5. `theta auth logout` deletes only the selected local record. It does not revoke the token on the server or clear copies already loaded by other processes.

The Windows backend uses the keyring crate's native `CRED_PERSIST_ENTERPRISE` storage. Records survive process exit and subsequent Windows logons; Windows account/roaming policy controls whether credentials can also roam. The store is scoped to the Windows user, not isolated from other applications running as that user. See [Windows credential persistence](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentiala).

**Persistence does not establish server validity.** The supplied Python package provides no token expiry, refresh, validation, or revocation contract. Status therefore reports `validity: "not_checked"`. Re-running `theta auth` obtains and saves a new session; failed login leaves the old record intact. Applications load an existing session explicitly and should prompt for authentication again if the server rejects it. There is no background refresh or saved password. Concurrent successful logins to the same target replace the stored record; the last write wins.

Native persistence supports Windows and Linux. Linux uses a persistent Secret Service collection, so records survive CLI exits and keyring daemon restarts; the collection must be unlocked again when required by the provider. The HTTP authentication API can be used on other platforms; persistence operations return an explicit unsupported-platform error there.

## Reuse in another Rust project

Add a path dependency on `crates/thetadata-auth` (or a Git dependency once this workspace is hosted). Authentication is explicit; the library never discovers environment variables or credentials on its own.

```rust,no_run
use thetadata_auth::{AuthClient, AuthConfig, Credentials, Environment, SessionStore};

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let store = SessionStore::new("default", Environment::Prod)?;
    let session = match store.load()? {
        Some(stored) => stored.into_session(),
        None => {
            let auth = AuthClient::new(AuthConfig::default())?;
            let credentials = Credentials::from_file("creds.txt")?;
            let session = auth.authenticate(&credentials).await?;
            store.save(&session)?;
            session
        }
    };
    // Pass session.session_id() to your authenticated transport; do not log it.
    println!("Account: {}", session.user().email);
    Ok(())
}
```

The market-data foundation accepts that exact session with `ThetaClient::with_session(config, session)`, so it does not authenticate again. Generated query structs use the vendor wire types. Callers must currently set Python's optional defaults explicitly. Flat-file manifests are rejected by the table decoder until a dedicated downloader is implemented. No market-data commands are exposed by this CLI yet.

## Verification

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
# Explicit native-store test: fake HTTP auth, then status/logout in separate processes.
# Uses a unique test profile and removes it afterward. Does not contact ThetaData.
cargo test -p thetadata-cli --test persistence -- --ignored
```

On Linux, use `bash tools/test-linux-persistence.sh` for the native-store test. It creates a private D-Bus session and disposable GNOME Keyring with synthetic data, then runs auth/status/logout in separate processes. It does not touch the user's normal keyring. Requires `dbus-run-session`, `gnome-keyring-daemon`, and `timeout`. The ordinary Linux test suite also checks that an unavailable D-Bus service produces an error rather than falsely reporting an absent session.

Tests cover API key and email/password request bodies, malformed/rejected authentication, timeouts, secret redaction, persistence isolation and process boundaries, and the initial data decoder. Live ThetaData credentials are not needed for these tests.

## Protocol provenance

The Python client exposes **80 methods**; its descriptor includes **82 RPCs**, including two additional corporate-action methods. `tools/extract_protocol.py` reads the serialized descriptors using Python AST parsing without executing the vendor package. The recovered descriptor, source hashes, and RPC inventory are in `crates/thetadata-proto/schema/`.

To regenerate after deliberately updating the bundled package, run `python tools/extract_protocol.py` with `google.protobuf` installed. Rust builds compile the checked-in descriptor through [tonic-prost-build's descriptor API](https://docs.rs/tonic-prost-build/0.14.6/tonic_prost_build/struct.Builder.html#method.compile_fds). The upstream Apache-2.0 license is retained at `crates/thetadata-proto/LICENSE.upstream`. The original Python distribution is unchanged.

The original `thetadata-1.0.12-py3-none-any.zip` and extracted `thetadata-1.0.12-py3-none-any/` directory are local research inputs and are **not committed**. Normal builds do not require them. CI checks exclusions, descriptor hashes, requirement IDs, and documentation links, then runs the Windows/Linux test and native-persistence matrix. A separate workflow records synthetic auth performance without live credentials.

The local vendor ZIP/extraction have been removed after hash verification.
Use [the recovery instructions](docs/research/upstream-source.md#recovering-the-removed-local-artifacts)
only when source regeneration is needed. [Proposed ADRs 0007-0012](docs/adr/README.md)
define the first market-data design; its requirements and implementation remain future work.
