# thetadata-cli

Independent authentication application for ThetaData. This **binary package**
builds the `theta` executable; it is separate from the reusable `thetadata-auth`
library and the experimental market-data client. It is not an official ThetaData
product and has no market-data query commands yet.

The repository `thetadata-rs` is a virtual Cargo workspace, not a package to
install. The current package/executable names are provisional pending a branding
decision before the first crates.io release. Publication is currently disabled.

## Install from this checkout

From the repository root:

```text
cargo install --locked --path apps/thetadata-cli
theta --help
```

After setting `THETADATA_API_KEY` in your process environment:

```text
theta auth
theta auth status --json
theta auth logout
```

Authentication saves the session in Windows Credential Manager or Linux Secret
Service. Linux requires an available D-Bus session and an unlocked persistent
keyring. Status checks local storage only; logout removes the selected local
record without revoking the server session. API keys and passwords are not saved.
The CLI needs no Java, gRPC or market-data decoding dependencies.

See the [workspace guide](https://github.com/Praevis-Quant/thetadata-rs#build-and-authenticate)
for setup, credential precedence, profiles, environments and exit codes. For
embedding authentication in Rust, use the library rather than invoking this CLI.
Do not use a crates.io install command until a reviewed release is published.

Licensed under Apache-2.0; see the repository
[LICENSE](https://github.com/Praevis-Quant/thetadata-rs/blob/main/LICENSE) and
[NOTICE](https://github.com/Praevis-Quant/thetadata-rs/blob/main/NOTICE).
