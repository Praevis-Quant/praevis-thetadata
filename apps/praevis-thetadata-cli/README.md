# praevis-thetadata-cli

Independent authentication application for ThetaData. This **binary package**
builds the `praevis-thetadata` executable; it is separate from the reusable `praevis-thetadata-auth`
library and the experimental market-data client. It has no market-data query commands yet.

> **Independent and experimental.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-alpha.3` is an early
> prerelease and **not** a full port of the vendor Python package.

**Supported platforms:** Windows (Credential Manager) and Linux (Secret Service
over D-Bus, with an unlocked persistent keyring). On other platforms,
authentication runs, but saving, status and logout report an unsupported-platform
error. Requires Rust 1.88+. Linux builds compile a vendored D-Bus library and
need a C compiler and `pkg-config`.

The repository `praevis-thetadata` is a virtual Cargo workspace, not a package to
install. The package is `praevis-thetadata-cli`; the executable is
`praevis-thetadata`.

## Install

The experimental `0.1.0-alpha.3` prerelease is on crates.io. Prereleases are
never selected implicitly, so name the version:

```text
cargo install praevis-thetadata-cli --version 0.1.0-alpha.3 --locked
praevis-thetadata --help
```

Alternatively, from a checkout, run this from the repository root:

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
This alpha is experimental: behavior and output may change between prereleases.

Licensed under Apache-2.0. The package includes `LICENSE` and `NOTICE`; see also the repository
[LICENSE](https://github.com/Praevis-Quant/praevis-thetadata/blob/main/LICENSE) and
[NOTICE](https://github.com/Praevis-Quant/praevis-thetadata/blob/main/NOTICE).
