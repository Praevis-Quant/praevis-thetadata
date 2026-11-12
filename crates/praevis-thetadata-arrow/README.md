# praevis-thetadata-arrow

Exact Arrow columns from ThetaData gRPC `ResponseData` messages, for the
praevis-thetadata clients (P1 PR 3 of the
[P1 design](../../docs/architecture/p1-design.md#4-columnar-decoder)).

> **Independent and unofficial.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-beta.2` is a prerelease
> whose public API is **frozen for beta.1** ([scope](../../docs/releases.md#beta1-scope-record-user-2026-11-10)): no breaking
> change without a new prerelease version and release notes.
> It is not a full port of the vendor Python package.

```toml
praevis-thetadata-arrow = "=0.1.0-beta.2"
```

## What it does

`ColumnarDecoder` appends the rows of many messages into the same Arrow
builders and returns a `RecordBatch` on `flush`:

| Wire value | Arrow type |
| --- | --- |
| text | `Utf8` |
| number | `Int64` |
| price (`value`, `scale`) | `Decimal128(38, 10)`, exact: unscaled = `value × 10^scale`; scale 0 is null |
| timestamp | `Timestamp(Millisecond, "America/New_York")` for zone 0, `"UTC"` for zone 1 |
| boolean | `Boolean` |
| null or unset | null; a column with no value yet in a batch is `Null` |

A column's type is fixed by its first non-null value and kept across batches.
A value of another kind, or a timestamp in another zone, is an error, never a
coercion. Timestamps finer than milliseconds are unsupported (ADR-0019).

Once warm, it allocates nothing per message: headers are compared in place,
and the ZSTD context and the decompression buffer are reused. Per-message limits
bound the encoded, declared and decompressed sizes, the ZSTD window, header
count and length, text length and rows. The buffered size is an accounted upper
bound of what the builders hold, not a measure of process memory.

If a message fails, nothing after it is accepted, and `flush` returns only the
rows of the messages before it. A full buffer (`BufferFull`) appends nothing:
flush, then push the same message again. Each decoder keeps one ZSTD context
and one decompression buffer, sized to the largest message it has seen.

## Streaming rules (P1 PR 4a)

The client's `query()` streams settle what this decoder leaves open:
- **Flush thresholds** (`BatchPolicy`): the first rows as soon as they arrive,
  then at 65,536 rows, 8 MiB accounted, or 50 ms after the oldest buffered
  row. Messages up to 64 KiB are decoded inline, larger ones on the blocking
  pool.
- **Text stays `Utf8`.** Per-batch dictionary encoding was dropped (decided
  2026-11-04), because it would change a column's type between batches. A
  fixed, opt-in choice per query may come later.
- **Schema rule:** the only change between batches is a `Null` column becoming
  typed. `collect()` and `unify` give earlier batches the final type.

## Parquet (feature `parquet`)

`ParquetSink` streams batches into one Parquet file:
- The file is written as `<name>.partial` and renamed into place by `finish`.
  An unfinished sink removes its partial file, and an existing target is kept
  unless `overwrite` is set.
- Compression is ZSTD (level 1 by default) or none, in row groups of up to
  131,072 rows. The Arrow schema, including decimal precision and time zones,
  is stored in the file, so readers get the exact types back.
- Columns that are still `Null` are held back until typed, up to 64 MiB, so
  the file gets the final types. A column written as `Null` that is typed
  later is a `SinkError::Schema`.

Results: [CPU per row](../../docs/performance/p1-columnar-results.md).
