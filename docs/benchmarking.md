# Benchmarking policy

The Criterion harness measures six independent concerns:

- `lexer`: tokenization without parsing;
- `parse_valid`: end-to-end strict parsing and AST construction;
- `parse_invalid`: diagnostic recovery on malformed inputs;
- `corpus`: the checked-in representative batch;
- `tck_corpus`: every unique accepted query in the generated syntax projection;
- `scaling`: approximately 1 KiB, 10 KiB, and 100 KiB generated inputs, plus
  valid and malformed 64 KiB contextual-keyword stress cases.

Run it in release mode through Cargo:

```console
cargo bench --bench parser
```

Criterion reports wall time and byte throughput. Input construction and fixture
loading are outside the timed loop, and both source and result pass through
`black_box`.

## Comparing revisions

Compare base and candidate revisions on the same quiet machine. Shared hosted CI
runners are too noisy for a blocking latency threshold, so their weekly report is
informational. Once a dedicated stable runner is available:

- warn when a primary benchmark regresses by more than 5%;
- rerun base and candidate automatically when it regresses by more than 15%;
- fail only if that greater-than-15% regression remains statistically significant;
- separately fail if the full external-corpus aggregate regresses by more than
  10% after confirmation.

Allocation counts and bytes allocated per parse should be collected in a
separate benchmark binary once an allocation-counter dependency is selected.
They should be reported before being gated.

## Legacy comparison

The retired Pest implementation did not have a recorded, reproducible Criterion
baseline before it was removed. Its parse result was also not equivalent: it
returned Pest pairs, while this rewrite constructs an owned typed AST. For those
reasons the project does not claim a speedup over `0.1`; future performance
claims must compare equivalent outputs on the same corpus and machine.

No absolute throughput SLA is defined yet. The first complete, conforming parser
run becomes the internal baseline; representative downstream workloads should
drive any later SLA.
