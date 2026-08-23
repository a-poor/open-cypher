# Parser benchmarks

The Criterion harness separates lexing, strict parsing, recovery, a
representative batch, all unique accepted TCK-projection queries, and size
scaling. Fixture loading, projection decoding, validation, and generated-input
construction happen outside the timed loop.

Run all benchmarks with:

```console
cargo bench --bench parser
```

Run one group while iterating:

```console
cargo bench --bench parser -- parse_valid
```

The `tck_corpus` group is a syntax-only workload derived from the version-pinned
TCK. It parses all 4,109 unique accepted queries in one measured batch. It is
not a benchmark of query execution and does not imply official TCK certification.

Benchmark reports are evidence, not a portable throughput promise. Compare base
and candidate revisions on the same quiet machine. See
[`docs/benchmarking.md`](../docs/benchmarking.md) for the regression policy.
