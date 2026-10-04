# Manual ThetaData service verification

Status: manual runner implemented; real-service evidence pending. This is a
Rust developer tool for auth and the existing stock EOD interfaces. It is not
a bulk downloader, live performance benchmark or claim of complete Python parity.
See [release gates](releases.md), [requirements](requirements/live-verification.md)
and [ADR-0017](adr/0017-manual-live-verification.md).

## Environment and credentials

Use **PROD** for the initial integration check. ThetaData describes STAGE as an
independent second connection for simultaneous workflows/streams; it does not
describe it as a credential-free sandbox or recommend it for load testing.
Both clients authenticate independently. This distinction was checked against
the [vendor staging documentation](https://www.thetadata.net/docs/Articles/Data-And-Requests/Staging-Environment.html)
on 2026-10-04. The runner supports an explicit STAGE choice for separately
selected checks, but does not silently switch environments or reuse a stored
session from another environment.

Select `--api-key-env` to read `THETADATA_API_KEY` from the process environment.
On Windows, an existing terminal may not inherit a recently saved user-level
variable. Load that one variable explicitly as shown below; never print its value.
No credential file is required for this mode.

Alternatively, `--api-key-file` reads a local file containing only an API key;
`--credentials-file` reads email on line one and password on line two. Keep files
outside the repository. Exactly one of these three credential sources is required;
there is no automatic discovery or fallback. The auth library still receives
explicit credentials and never reads environment variables itself. Never put a
key/password in command arguments, chat, Git, fixture metadata or a bug report.

The [current vendor getting-started documentation](https://docs.thetadata.us/Articles/Getting-Started/Getting-Started.html)
describes portal API keys and the two-line credential format. Actual account
entitlements, date coverage and direct-service acceptance remain live questions.
The Rust client uses the direct HTTPS authentication and MDDS gRPC mapping from
the tracked Python artifact; this check does not require a running Java Terminal.
Do not apply REST-specific column/date assumptions to direct protobuf responses
without observing them.

## Run one small capture

Run from this source checkout with a current Rust toolchain and Git available:

```powershell
cargo build --locked -p thetadata-client --features live-tests --example live_eod
$env:THETADATA_API_KEY = [Environment]::GetEnvironmentVariable('THETADATA_API_KEY', 'User')
if ([string]::IsNullOrWhiteSpace($env:THETADATA_API_KEY)) { throw 'User-level THETADATA_API_KEY is missing' }
.\target\debug\examples\live_eod.exe capture --confirm-live --environment PROD --api-key-env --run-id prod-aapl-001 --symbol AAPL --start 2024-01-02 --end 2024-01-02
```

On other platforms, inherit the variable through your usual local secret setup
and use the same `--api-key-env` flag. The value is never a CLI argument or report
field. Missing/empty/oversized/multiline keys fail without echoing the value.

Start with a historical **single trading day** within the account's coverage.
The example date is a candidate, not a promise of entitlement or returned rows.
Each run ID must be new and contain only ASCII letters, digits, `-` or `_`.
No existing run is overwritten. Choose another ID for every attempt.

The feature and example invocation are explicit opt-ins; the capture command
also requires `--confirm-live` and rejects an environment containing `CI`.
No workflow invokes this example, enables its feature, accepts live credentials,
or uploads its output. Ordinary workspace tests remain entirely synthetic.

One run authenticates once, keeps the session only in memory, and makes up to
three **sequential** EOD RPCs for the same query: raw envelope capture, typed
numeric delivery, and typed Table delivery. There is a one-second pause before
each additional live query. No retries, refresh, native-store writes, revocation,
wildcards, concurrency sweeps or account discovery occur. Stop on rejection and
investigate the safe status category before making another request. Performance
and fault/load tests continue against the mock service.

The raw capture permits at most 16 envelopes, 8 MiB per envelope and 32 MiB total.
The selected date endpoints must be at most 31 days apart and the symbol at most
128 UTF-8 bytes. Authentication retains its 10-second connect/30-second request
budgets. EOD uses a 10-second connect budget, 120-second query deadline and
30-second idle budget. Typed delivery keeps the client's finite default EOD
decode/resource policy and a 16-batch comparison cap. These developer-tool limits
are not a total RSS bound or a proposal to relax product resource requirements.

Exit zero means the complete capture, offline comparison and separate live
numeric/Table value comparisons passed. Exit nonzero means failure, mismatch or
incomplete work; inspect the manifest without copying private artifacts into
chat. Successful empty EOF is distinguishable by zero rows and is **not** the
nonempty live evidence required to accept EOD. NOT_FOUND/rejection is not success.

## Private evidence layout

All output is under ignored `artifacts/live/<run-id>/`:

| File | Contents and purpose |
| --- | --- |
| `manifest.json` | Capture format/time, PROD/STAGE, query, runtime checkout commit/dirty flag, executable and compiled-input hashes, compiler/platform, upstream version from the protocol manifest, descriptor/manifest hashes, ordered response lengths/digests, terminal phase/category and overall verification outcome; rejected auth HTTP status only |
| `batch-000.pb`, etc. | Protobuf `ResponseData` envelopes re-encoded by prost in arrival order; original compressed payload bytes and known envelope fields are retained |
| `replay-<time>-<pid>.json` | Fresh offline comparison report per invocation: compiled source fingerprint, capture-frame-list digest, headers, row/batch counts, value-kind counts and exact-value semantic digest for raw/numeric/Table paths |
| `live.json` | Separate live numeric/Table query summaries and equality with the captured reference; written once for this run |

This is **not a packet capture**. Prost drops unknown envelope fields and may
canonicalize encoding. HTTP headers/trailers, authentication response bodies,
requests, account details, tokens and remote error text are not serialized.
Flat-file manifests are rejected before writing because they can contain URLs
and are outside this slice. A numeric gRPC status can originate in tonic or the
server; the raw capture does not assert local/remote provenance.

The response payload is real licensed market data and remains **private and
unreviewed**, not automatically sanitized. Unexpected server content could still
contain sensitive text inside the payload. Do not commit, upload or share the
capture. Unix run directories/files are created with 0700/0600; Windows inherits
the parent directory's ACL, so use a private user checkout and credential file.
Git ignore is not access control. The runner never searches for other credentials.

Normal failures retain earlier complete envelopes and a safe terminal category.
Interruption can leave `in_progress`, a partially written manifest, or an orphan
batch; these are incomplete research inputs, never accepted results. Preserve
the directory for inspection and use a new run ID. Capture files are immutable;
the manifest records progress, and later offline replays create new reports.
For reproducible release evidence build from a clean committed checkout; dirty
runs remain labelled as exploratory. The compile-time source fingerprint covers
the listed runner/client/core/auth/fixture inputs and lockfile, not an attestation
of an entire machine or every generated byte.

## Replay without service access

```text
cargo run --locked -p thetadata-client --features live-tests --example live_eod -- replay --run-id prod-aapl-001
```

Replay requires no real credential file and contacts only fresh loopback mock
auth/gRPC listeners with synthetic identity. It checks capture format, descriptor,
file sizes, response hashes and complete EOF before serving those messages through
the existing fixture. Incomplete captures are retained for research but not
accepted by this initial replay command. Decoder/protocol failures remain failures.

It compares typed numeric batches (via explicit Table conversion), typed Tables
and the existing raw helper. Schema, row order, duplicates, nulls, integers and
exact price scale/mantissa are preserved; timestamps use the existing explicit
RFC 3339 adapter. SHA-256 covers canonical serialized headers and ordered typed
rows; it is insensitive to batch splitting. Separate live queries can differ
because the service revises history: a mismatch fails verification and requires
investigation; it is not automatically a decoder defect.

These are differential checks, not an independent oracle: paths share core
conversions, and replay cannot reproduce the original network's timing or prove
entitlements for other endpoints. Retain the existing independent synthetic
edge-value/fault tests. Compare observed headers, value kinds, compression and
empty/completion behavior with those tests; synthetic column names are deliberately
not an asserted vendor schema.

Use the [observation template](research/live-observations.md) to record findings.
Review raw response contents privately, record differences and requirement IDs,
and derive small synthetic reproductions for new cases. Before promoting actual
vendor rows to public fixtures, confirm redistribution permission, remove any
identity/secrets, record all transformations plus source/result hashes, and review
the exact artifact. Do not rewrite the original capture or relabel synthetic rows
as vendor evidence. No automatic fixture promotion is implemented.

## Verify the runner itself, without credentials

```text
cargo test --locked -p thetadata-client --features live-tests --example live_eod
cargo clippy --locked -p thetadata-client --features live-tests --example live_eod -- -D warnings
```

These explicit local checks exercise CLI opt-in, path/query validation, complete
and partial mock captures, safe diagnostics, hash rejection and differential
replay. They do not call ThetaData. The feature-gated example is deliberately
absent from existing CI commands; changes to it require these manual checks.
