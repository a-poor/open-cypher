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

`parse_invalid/clause_salvage` measures retention of valid clauses around a
broken SET. `recovery_budget` scales malformed suffixes, including the 125/126
boundary where the retry budget stops retaining WITH. These measure end-to-end
recovering parsing, including the initial strict parse.

`union_recovery` scales valid UNION ALL branches followed by a broken RETURN,
including inputs that exhaust the shared recovery budget. It also includes the
initial strict parse, rather than measuring just the recovery pass.

Strict parsing stops extending candidates after a definitive lookahead-token
error and skips UNION splits beyond that error. A unit test bounds clause-parser
attempts for this malformed-chain workload through 512 branches, independently of
wall-clock timing. This is a bound on parser attempts, not a claim of linear
end-to-end runtime: token scans and contextual-name retries still contribute work.

A local macOS arm64 comparison against `1254b68` (2026-10-16, no overlapping
checks, 10 samples, 1-second warmup and measurement targets) measured:

| UNION branches | Before | After |
| --- | ---: | ---: |
| 1 | 65.5 µs | 55.5 µs |
| 4 | 411 µs | 285 µs |
| 16 | 4.72 ms | 1.54 ms |
| 64 | 149 ms | 2.02 ms |
| 128 | 1.16 s | 2.39 ms |

These are Criterion point estimates for end-to-end recovering parsing. The
recovery retry budget still limits additional candidate parsing rather than
total latency. Save a baseline with `--save-baseline before-union-perf`, then
compare with `--baseline before-union-perf` using the same benchmark command.

Benchmark reports are evidence, not a portable throughput promise. Compare base
and candidate revisions on the same quiet machine. See
[`docs/benchmarking.md`](../docs/benchmarking.md) for the regression policy.

## Expression recovery (0.4 preparation)

Local macOS 26.5.1 arm64 measurements with rustc 1.99.0 compared `8f02ec1`
(UNION optimization, before expression recovery) with `b0162d9`. Both used the
same benchmark inputs, 30 samples, a 1-second warmup, and a 2-second measurement
target. No project checks ran concurrently. The baseline worktree was given the
two new expression benchmark inputs; the package's release artifacts were
cleaned when switching revisions to prevent shared-target artifact reuse.

| Input | Before, two runs | Expression recovery, two runs |
| --- | ---: | ---: |
| `RETURN 1 +` | 27.4–51.6 µs | 26.1–26.1 µs |
| Nested call/list missing operands | 46.9–59.9 µs | 42.0–42.5 µs |
| Five missing operands, exceeding repair limit | 48.0–76.6 µs | 79.1–79.4 µs |
| 64 UNION prefixes + incomplete RETURN | 2.98–3.74 ms | 1.97–1.98 ms |
| 128 UNION prefixes + incomplete RETURN | 2.47–2.49 ms | 2.34–2.35 ms |

Ranges above are Criterion point estimates across two runs, not confidence
intervals. Baseline variance was substantial, so these do **not** establish a
new speedup or precise overhead percentage. The failed-repair case costs up to
about 31 µs more than the faster baseline run: four attempted insertions do work
even when the final result still falls back to a clause error. Successful
repairs also produce richer ASTs than the baseline, so outputs differ.

The earlier UNION fix remains effective on these inputs: 128 prefixes take
about 2.35 ms, versus the previously recorded 1.16 seconds before pruning.
This is workload-specific evidence, not an end-to-end complexity guarantee.

Reproduce the selected cases on each revision with:

```console
cargo bench --bench parser --locked -- 'parse_invalid/(expression_nested|expression_limit|trailing_operator)|union_recovery/(64|128)$' --sample-size 30 --warm-up-time 1 --measurement-time 2 --save-baseline before-expression
```

On the candidate, replace `--save-baseline before-expression` with
`--baseline before-expression`.
