# Parser benchmarks

The Criterion harness separates lexing, strict parsing, recovery, a representative
batch, and size scaling. Fixture loading and generated-input construction happen
outside the timed loop.

Run all benchmarks with:

```console
cargo bench --bench parser
```

Run one group while iterating:

```console
cargo bench --bench parser -- parse_valid
```

These fixtures are a small workload corpus, not the openCypher TCK. Once the
version-pinned TCK query manifest is available, add a separately named `tck_batch`
group instead of relabeling the existing representative batch.

Benchmark reports are evidence, not a portable throughput promise. Compare base
and candidate revisions on the same quiet machine. See
[`docs/benchmarking.md`](../docs/benchmarking.md) for the regression policy.
