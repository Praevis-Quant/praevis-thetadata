# Praevis ThetaData

> Independently maintained Rust client libraries for ThetaData services. This project is not affiliated with, sponsored by, or endorsed by Theta Data Inc. “ThetaData” identifies the service with which this software interoperates.

Rust workspace derived from the ThetaData Python package, originally **1.0.12** and now tracking **1.0.17** ([update research](docs/research/thetadata-1.0.13-1.0.17.md)). The first application is an authentication CLI, backed by an authentication library that other projects can use independently.

See the [auth requirements and ADRs](docs/README.md), [roadmap](docs/ROADMAP.md), [vendor research](docs/research/thetadata-1.0.12.md), and [performance methodology](docs/performance/auth.md). Only authentication is a baselined feature scope; the market-data crates are foundations for later work.

**Users:** the [user guide](docs/user-guide.md) covers supported platforms,
native-store prerequisites, the resource policy and the differences from the
vendor Python package.

For implemented changes, see the [changelog](CHANGELOG.md). Coding agents should start with [AGENTS.md](AGENTS.md), which holds the project guidance and Git conventions.

Start with the [connection map and SVG](docs/connections.md) to distinguish our
direct HTTPS/gRPC client from optional Theta Terminal REST/WebSocket adapters.
The direct path needs no Java or JAR. See [release milestones](docs/releases.md),
[manual live checks](docs/live-verification.md) and the
[subscription test matrix](docs/subscription-testing.md) for verified scope and gaps.

Newly authored code and tooling are licensed under [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution and [the preserved upstream license](crates/praevis-thetadata-proto/LICENSE.upstream) for the bundled protocol source.

## Workspace

`praevis-thetadata` is the repository and **virtual workspace**, not a publishable root
package. Each row below is a separate Cargo package. The CLI lives in
[`apps/praevis-thetadata-cli`](apps/praevis-thetadata-cli/README.md), publishes under its own
package name, and builds the `praevis-thetadata` executable. Cargo places build output in
the shared root `target/` directory; that does not make the CLI a root package.
The selected naming and compatibility boundaries are recorded in
[the naming decision](docs/project-naming.md). The packages are released
together on crates.io as **experimental `0.1.0-alpha.3` prereleases**. Alpha.3
adds `praevis-thetadata-arrow` and `praevis-thetadata-stream`; the Python
package is not on PyPI, and its wheels are attached to the GitHub release. It is
independent and unofficial, APIs may change between alphas, and it is not full
Python parity. Prereleases need an explicit version, for example
`cargo install praevis-thetadata-cli --version 0.1.0-alpha.3 --locked` or
`praevis-thetadata-auth = "=0.1.0-alpha.3"`. Publication is manual and
owner-approved ([release plan](docs/crates-io-release-plan.md)); no workflow
publishes.

| Package | Responsibility | Status |
| --- | --- | --- |
| `praevis-thetadata-auth` | HTTP authentication, credentials, in-memory sessions; optional (`native-store`) Windows/Linux session persistence | First deliverable |
| `praevis-thetadata-cli` (`praevis-thetadata`) | Credential discovery, `auth` (in memory) or `auth --save`, status and logout | First deliverable |
| `praevis-thetadata-proto` | Generated protobuf types and gRPC bindings for all 100 RPCs | Foundation |
| `praevis-thetadata-core` | Transport-independent prices, timestamps, values, tables | Foundation |
| `praevis-thetadata-client` | gRPC requests, ZSTD decoding, batch streaming with limits on accounted decode allocations (not total memory) | Experimental typed EOD; one PROD query verified, broader acceptance open |

Dependency direction:

```text
praevis-thetadata CLI ───────────────> praevis-thetadata-auth
praevis-thetadata-client ────────> praevis-thetadata-auth
                ├───────> praevis-thetadata-core
                └───────> praevis-thetadata-proto
```

Authentication needs no gRPC, dataframe, or market-data dependencies. The CLI currently depends only on authentication. Stocks, options, indices, calendars, and interest rates belong in modules of the same client library; they do not need individual crates. Add other applications under `apps/` as needed.

## Build and authenticate

Requires a current stable Rust toolchain. On Windows, install the Visual Studio C++ build tools. On Linux, install a C/C++ compiler, make, and pkg-config; the Linux backend builds its bundled D-Bus library. A full workspace build also compiles ZSTD. The CLI alone does not build ZSTD or protobuf. Normal builds need neither Python nor `protoc`.

```powershell
cargo build -p praevis-thetadata-cli
# With THETADATA_API_KEY already set in your environment: log in and report the account.
# The session is not stored (in memory only, like the vendor Python package).
.\target\debug\praevis-thetadata.exe auth

# Log in and also save the session in Windows Credential Manager:
.\target\debug\praevis-thetadata.exe auth --save

# Or use a Theta Terminal compatible two-line credentials file:
.\target\debug\praevis-thetadata.exe auth --save --creds-file C:\private\creds.txt

# After --save, run this in a second terminal/process without supplying login credentials:
.\target\debug\praevis-thetadata.exe auth status
.\target\debug\praevis-thetadata.exe auth status --json

# Delete the saved local session:
.\target\debug\praevis-thetadata.exe auth logout
```

On Linux (Ubuntu/Debian):

```bash
sudo apt-get install build-essential pkg-config dbus gnome-keyring
cargo build --locked -p praevis-thetadata-cli
./target/debug/praevis-thetadata auth --save --creds-file /private/creds.txt
./target/debug/praevis-thetadata auth status --json
./target/debug/praevis-thetadata auth logout
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
./target/debug/praevis-thetadata auth --save --creds-file /private/creds.txt
./target/debug/praevis-thetadata auth status
```

Services and containers need access to an unlocked Secret Service and persistent keyring storage under the same OS account. GNOME Keyring normally stores its encrypted keyrings under `$XDG_DATA_HOME/keyrings` (default `~/.local/share/keyrings`); preserve that storage across container replacements. The CLI does not save the keyring unlock password or automatically start/unlock the service. An unavailable service returns an actionable error, distinct from “no saved session”; there is no plaintext or process-only fallback.

The D-Bus/keyring setup above is needed only for `--save`, `status` and `logout`.
Plain `auth` never touches the native store.

Credential precedence for `praevis-thetadata auth` follows the vendor Python
package ([ADR-0020](docs/adr/0020-in-memory-sessions-and-optional-persistence.md)):

1. `--creds-file PATH`, a two-line file (email on line one, password on line two).
2. `THETADATA_API_KEY`.
3. Both `THETADATA_EMAIL` and `THETADATA_PASSWORD`. These variables are a
   Rust-only extra; Python takes this pair only as constructor arguments. If only
   one is set, or either is blank, discovery falls through, as Python's does.
4. `THETADATA_CREDENTIALS_FILE`, otherwise `./creds.txt`, as the final fallback.

A file selected by `--creds-file` or `THETADATA_CREDENTIALS_FILE` that is
missing or invalid is an error; later sources are not tried. There are no
password or API-key command-line flags. The CLI does not load `.env` files
automatically. Credentials and tokens are never printed. `--json` emits
account/subscription details and whether the session was saved (`persisted`). Exit codes: **0** success, **1** operational/authentication error, **2** invalid command arguments, **3** no persisted session when checking status.

`--environment prod|stage` defaults to PROD; `THETADATA_MDDS_TYPE` is also supported. `--profile NAME` defaults to `default`. These options also work with `status` and `logout`:

```powershell
.\target\debug\praevis-thetadata.exe auth --save --profile research --environment stage
.\target\debug\praevis-thetadata.exe auth status --profile research --environment stage --json
```

## How authentication persists

By default a session lives only in memory, as in the vendor Python package,
which reuses it through `existing_authorized_client` and has no native session
store ([reviewed artifact](docs/research/thetadata-1.0.12.md#what-the-wheel-establishes-about-authentication)).
`praevis-thetadata auth` logs in, reports the account and exits without storing
anything. Library callers authenticate and pass the session directly to
`ThetaClient::with_session`. The manual
[`live_eod --api-key-env` workflow](docs/live-verification.md) does the same.

Saving a session across processes is an optional Rust enhancement
([ADR-0020](docs/adr/0020-in-memory-sessions-and-optional-persistence.md),
superseding ADR-0003). In the library it requires the `native-store` feature; in
the CLI it requires `auth --save`. The credential store holds the **session
returned by login**, so another process can reuse it without the API key or a
new login. The API key and password are never copied into the saved record.

1. `praevis-thetadata auth --save` submits credentials to the same HTTPS authentication endpoint and with the same request shape as Python 1.0.12, unchanged through 1.0.17.
2. After a successful response, `SessionStore::save` stores a versioned JSON record **inside the native credential store**, not in a project file. The record contains the session ID, account/subscription metadata, environment, and save time. The API key and password are not stored.
3. The stable identifier is `praevis.thetadata.auth.v1:PROD:default` for the default production profile. Windows uses it as the credential target. Linux uses it as the Secret Service `service` attribute, with the profile as `username`, in the default collection. Changing the profile or environment selects a different entry. All applications using this library and identifier share that record under the same OS user and credential service. Windows and Linux stores are independent; sessions are not automatically synchronized between them.
4. `praevis-thetadata auth status` starts independently and reads the record from the native store. It does not authenticate, require login credentials, or contact ThetaData. It reports the account, save time, and storage location without exposing the token.
5. `praevis-thetadata auth logout` deletes only the selected local record. It does not revoke the token on the server or clear copies already loaded by other processes.

The Windows backend uses the keyring crate's native `CRED_PERSIST_ENTERPRISE` storage. Records survive process exit and subsequent Windows logons; Windows account/roaming policy controls whether credentials can also roam. The store is scoped to the Windows user, not isolated from other applications running as that user. See [Windows credential persistence](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentiala).

**Persistence does not establish server validity.** The supplied Python package provides no token expiry, refresh, validation, or revocation contract. Status therefore reports `validity: "not_checked"`. Re-running `praevis-thetadata auth --save` obtains and saves a new session; a failed login, with or without `--save`, leaves the old record intact. Records saved by earlier versions, which saved on every `auth`, keep working: the identity and v1 format are unchanged. Applications load an existing session explicitly and should prompt for authentication again if the server rejects it. There is no background refresh or saved password. Concurrent successful logins to the same target replace the stored record; the last write wins.

Native persistence supports Windows and Linux. Linux uses a persistent Secret Service collection, so records survive CLI exits and keyring daemon restarts; the collection must be unlocked again when required by the provider. The HTTP authentication API can be used on other platforms; persistence operations return an explicit unsupported-platform error there.

## How long requests take

Large option requests can be silent for a minute or more before the first row
arrives. In our measurements more than half of every request passed before any
data reached the client. That is normal
and is not a hung client. See [where the time goes](docs/performance/request-timing.md)
for the measurements and how to get the best results. One login per account: a
second login ends the first session.

## Reuse in another Rust project

Add `praevis-thetadata-auth = "=0.1.0-alpha.3"` from crates.io (experimental prerelease), or a path dependency on `crates/praevis-thetadata-auth`. Authentication is explicit; the library never discovers environment variables or credentials on its own. By default the session lives in memory, as in the vendor Python package: clone it to reuse it within your process ([ADR-0020](docs/adr/0020-in-memory-sessions-and-optional-persistence.md)). The default build has no keyring or D-Bus dependencies.

```rust,no_run
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials};

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let auth = AuthClient::new(AuthConfig::default())?;
    let session = auth.authenticate(&Credentials::from_file("creds.txt")?).await?;
    // Pass session.session_id() to your authenticated transport; do not log it.
    println!("Account: {}", session.user().email);
    Ok(())
}
```

Persisting a session across processes is an optional Rust enhancement. Enable
`features = ["native-store"]` to use `SessionStore` (Windows Credential Manager
or Linux Secret Service; same identity and record as the CLI):

```rust,ignore
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Environment, SessionStore};

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let store = SessionStore::new("default", Environment::Prod)?;
    let session = match store.load()? {
        Some(stored) => stored.into_session(),
        None => {
            let auth = AuthClient::new(AuthConfig::default())?;
            let session = auth.authenticate(&Credentials::from_file("creds.txt")?).await?;
            store.save(&session)?;
            session
        }
    };
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
cargo test -p praevis-thetadata-cli --test persistence -- --ignored
```

On Linux, use `bash tools/test-linux-persistence.sh` for the native-store test. It creates a private D-Bus session and disposable GNOME Keyring with synthetic data, then runs auth/status/logout in separate processes. It does not touch the user's normal keyring. Requires `dbus-run-session`, `gnome-keyring-daemon`, and `timeout`. The ordinary Linux test suite also checks that an unavailable D-Bus service produces an error rather than falsely reporting an absent session.

Tests cover API key and email/password request bodies, malformed/rejected authentication, timeouts, secret redaction, persistence isolation and process boundaries, and the initial data decoder. Live ThetaData credentials are not needed for these tests.

## Protocol provenance

The Python 1.0.17 client exposes **81 methods**; its descriptor includes **100 RPCs**, including two corporate-action and seventeen futures methods without Python wrappers (1.0.12 had 80 methods and 82 RPCs). `tools/extract_protocol.py` reads the serialized descriptors using Python AST parsing without executing the vendor package. The recovered descriptor, source hashes, and RPC inventory are in `crates/praevis-thetadata-proto/schema/`.

To regenerate after deliberately updating the bundled package, run `python tools/extract_protocol.py --archive <wheel>` with `google.protobuf` installed. It reads the wheel in memory and refuses any wheel whose hash differs from the reviewed `docs/research/vendor-<version>.json`; `--check` compares instead of writing. Rust builds compile the checked-in descriptor through [tonic-prost-build's descriptor API](https://docs.rs/tonic-prost-build/0.14.6/tonic_prost_build/struct.Builder.html#method.compile_fds). The upstream Apache-2.0 license is retained at `crates/praevis-thetadata-proto/LICENSE.upstream`. The original Python distribution is unchanged.

The original `thetadata-1.0.12-py3-none-any.zip` and extracted `thetadata-1.0.12-py3-none-any/` directory are local research inputs and are **not committed**. Normal builds do not require them. CI checks exclusions, descriptor hashes, requirement IDs, and documentation links, then runs the Windows/Linux test and native-persistence matrix. A separate workflow records synthetic auth performance without live credentials.

The local vendor ZIP/extraction have been removed after hash verification.
Use [the recovery instructions](docs/research/upstream-source.md#recovering-the-removed-local-artifacts)
only when source regeneration is needed. [Proposed ADRs 0007-0012](docs/adr/README.md)
define the first market-data design. Its [stock EOD requirements](docs/requirements/L1-stock-eod.md)
are defined, with [acceptance gates](docs/requirements/stock-eod-evidence.md);
runtime implementation and acceptance verification remain planned.
