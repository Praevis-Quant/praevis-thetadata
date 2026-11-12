# Release consumer crates

These are external consumers used by `tools/staging_registry.py`. They are
copied to a temporary directory outside the workspace and resolve the
`praevis-thetadata-*` packages only from the local `praevis-staging` registry,
so a passing build exercises the actual archives rather than workspace path
dependencies. Each is its own workspace and is not a workspace member. The code
mirrors each package README example; nothing contacts ThetaData or reads
credentials. The client consumer also compiles the frozen beta.1 client surface
(`query`/`job` with `RequestOptions`, the tier's job concurrency and Parquet
output).
