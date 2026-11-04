# praevis-thetadata-arrow

Exact Arrow columns from ThetaData gRPC `ResponseData` messages, for the
praevis-thetadata clients (P1 PR 3 of the
[P1 design](../../docs/architecture/p1-design.md#4-columnar-decoder)).

> **Independent and experimental.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. It is not published yet, and its
> API may change before it is.

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

## Not yet

These parts of the design arrive with the query engine (P1 PR 4), where batch
schemas across a stream are settled:
- the flush thresholds (rows, bytes, latency, first message) and inline versus
  blocking-pool decoding;
- per-batch dictionary encoding of low-cardinality text;
- a column that is `Null` in early batches and typed later. Today its type
  changes between batches.

Results: [CPU per row](../../docs/performance/p1-columnar-results.md).
