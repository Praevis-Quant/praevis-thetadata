# praevis-thetadata-cli

Independent authentication application for ThetaData. This **binary package**
builds the `praevis-thetadata` executable; it is separate from the reusable `praevis-thetadata-auth`
library and the experimental market-data client. It is not an official ThetaData
product and has no market-data query commands yet.

The repository `praevis-thetadata` is a virtual Cargo workspace, not a package to
install. The package is `praevis-thetadata-cli`; the executable is
`praevis-thetadata`. Publication is currently disabled.

## Install from this checkout

From the repository root:

```text
cargo install --locked --path apps/praevis-thetadata-cli
praevis-thetadata --help
```

After setting `THETADATA_API_KEY` in your process environment:

```text
praevis-thetadata auth              # log in and report the account; nothing is saved
praevis-thetadata auth --save       # also save the session in the native credential store
praevis-thetadata auth status --json
praevis-thetadata auth logout
```

Like the vendor Python package, `auth` keeps the session in memory only. With
`--save` it is stored in Windows Credential Manager or Linux Secret Service;
on Linux that requires an available D-Bus session and an unlocked persistent
keyring. Credential order follows Python: `--creds-file`, then
`THETADATA_API_KEY`, then `THETADATA_EMAIL`/`THETADATA_PASSWORD` (a Rust-only
extra), then `THETADATA_CREDENTIALS_FILE` or `./creds.txt`. Status checks local storage only; logout removes the selected local
record without revoking the server session. API keys and passwords are not saved.
The CLI needs no Java, gRPC or market-data decoding dependencies.

See the [workspace guide](https://github.com/Praevis-Quant/praevis-thetadata#build-and-authenticate)
for setup, credential precedence, profiles, environments and exit codes. For
embedding authentication in Rust, use the library rather than invoking this CLI.
Do not use a crates.io install command until a reviewed release is published.

Licensed under Apache-2.0; see the repository
[LICENSE](https://github.com/Praevis-Quant/praevis-thetadata/blob/main/LICENSE) and
[NOTICE](https://github.com/Praevis-Quant/praevis-thetadata/blob/main/NOTICE).
