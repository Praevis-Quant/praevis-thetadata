# GitHub release draft: v0.1.1 (not published)

Status: **draft.** Publishing a patch needs the user's approval; the
approval of 2026-11-13 was for `0.1.0`
([checklist](crates-io-release-plan.md#011-publication-checklist-and-order-prepared-2026-11-13)).
Nothing is tagged or published yet.

**To fill in at publication:**
- the date;
- the commit;
- the exact wheel file names, from that commit's CI artifacts
  `wheel-ubuntu-latest` and `wheel-windows-latest`. The same two files go
  to PyPI and to this release.

---

**Title:** `v0.1.1`. Not a pre-release: mark it as the latest release.

**Body:**

> **A patch release of `0.1.0`.** praevis-thetadata is an independent,
> unofficial client for ThetaData's market-data service. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.

## Fixed

**Option Greeks EOD over every expiration is now planned one day per
request.** On `0.1.0`, `job()`, Python's planned calls (`query`, `batches`,
`to_parquet` and the vendor-named methods) and `describe()` sent
`option_history_greeks_eod` with expiration `*` over several days as one
request. The service refuses that shape with `InvalidArgument`: two- and
five-day ranges were refused, while one day answered.
- **Now:** the planner splits it per day. A single expiration keeps the
  366-day split, and option EOD over every expiration, which the service
  answers whole, is unchanged
  ([known issues](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.1/docs/endpoints/known-issues.md)).
- **On `0.1.0`:** send one day per request, or one expiration.
- **Untested look-alikes:** binomial Greeks EOD and at-time trade and quote
  also accept `*` with a 366-day split. They have not been sent `*` over
  several days, and are unchanged.

**API:** `Endpoint` gains a provided method, `split(&self)`, defaulting to
`SPLIT`. `Endpoint` is sealed, so nothing that compiled against `0.1.0`
breaks. There are no other changes to the supported API.

The full list, including unpublished tooling, is in
[CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.1/CHANGELOG.md).

## Install

```toml
praevis-thetadata-client = "0.1"       # resolves 0.1.1
praevis-thetadata-stream = "=0.1.1"    # optional, real time, experimental: pin it
```

```text
cargo install praevis-thetadata-cli --locked
pip install --upgrade praevis-thetadata
```

The wheels are for Windows x86_64 and Linux x86_64 (glibc 2.28 or newer),
CPython 3.10 or newer. The known gaps of `0.1.0` still apply
([list](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.1/docs/releases.md#known-gaps-shipped-in-010)),
less the one fixed here.

## Assets

| File | Platform |
| --- | --- |
| `praevis_thetadata-0.1.1-cp310-abi3-win_amd64.whl` | Windows x86_64, CPython 3.10+ |
| `praevis_thetadata-0.1.1-cp310-abi3-manylinux_2_28_x86_64.whl` | Linux x86_64, glibc 2.28+, CPython 3.10+ |
