# GitHub release draft: v0.1.3 (not published)

Status: **draft.** The user approved the fix for `0.1.3` on 2026-11-15;
publishing needs the user's final approval
([checklist](crates-io-release-plan.md#013-publication-checklist-and-order-prepared-2026-11-15)).
Nothing is tagged or published yet.

**To fill in at publication:**
- the date;
- the commit;
- the exact wheel file names, from that commit's CI artifacts
  `wheel-ubuntu-latest` and `wheel-windows-latest`. The same two files go
  to PyPI and to this release.

---

**Title:** `v0.1.3`. Not a pre-release: mark it as the latest release.

**Body:**

> **A patch release of `0.1.2`.** praevis-thetadata is an independent,
> unofficial client for ThetaData's market-data service. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.

## Fixed

**Option binomial Greeks EOD over every expiration is now planned one day
per request.** The vendor documents that any expiration `*` for this
endpoint must be requested day by day, as for Greeks EOD, which `0.1.1`
fixed after the service refused it over several days. On `0.1.2` and
earlier, `job()`, Python's planned calls (`query`, `batches`, `to_parquet`
and the vendor-named methods) and `describe()` sent it as one request over
the whole range.
- **Now:** the planner splits it per day. A single expiration keeps the
  366-day split
  ([known issues](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.3/docs/endpoints/known-issues.md)).
- **On `0.1.2` and earlier:** send one day per request, or one expiration.
- **Untested look-alikes:** at-time trade and quote also accept `*` with a
  366-day split. Nothing documents a limit for them, and they are
  unchanged.

**The gRPC client no longer panics when the application also enables
rustls's aws-lc-rs provider** (for example through `object_store`'s AWS
support): "Could not automatically determine the process-level
CryptoProvider". If the application installed a process-default provider,
the client uses it; otherwise it uses ring explicitly, as the auth client
already did.

**API:** no change. The full list, including unpublished tooling, is in
[CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.3/CHANGELOG.md).

## Install

```toml
praevis-thetadata-client = "0.1"       # resolves 0.1.3
praevis-thetadata-stream = "=0.1.3"    # optional, real time, experimental: pin it
```

```text
cargo install praevis-thetadata-cli --locked
pip install --upgrade praevis-thetadata
```

The wheels are for Windows x86_64 and Linux x86_64 (glibc 2.28 or newer),
CPython 3.10 or newer. The known gaps of `0.1.0` still apply
([list](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.3/docs/releases.md#known-gaps-shipped-in-010)),
less the one fixed in `0.1.1`.

## Assets

| File | Platform |
| --- | --- |
| `praevis_thetadata-0.1.3-cp310-abi3-win_amd64.whl` | Windows x86_64, CPython 3.10+ |
| `praevis_thetadata-0.1.3-cp310-abi3-manylinux_2_28_x86_64.whl` | Linux x86_64, glibc 2.28+, CPython 3.10+ |
