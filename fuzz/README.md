# Fuzzing

The fuzz package is an independent Cargo workspace so normal library builds do
not require nightly Rust or libFuzzer.

Install and build the harnesses:

```console
cargo install cargo-fuzz --locked
cargo check --manifest-path fuzz/Cargo.toml --bins --locked
cargo +nightly fuzz build
```

The checked-in `fuzz/Cargo.lock` pins harness dependencies. The explicit
`cargo check --locked` command fails if either fuzz manifest would change it.

Run a target with the checked-in Cypher dictionary:

```console
cargo +nightly fuzz run parse_strict fuzz/corpus/parse_strict -- \
  -dict=fuzz/cypher.dict -max_len=65536 -timeout=5 -rss_limit_mb=2048
```

Replay only the checked-in corpus, without generating new inputs:

```console
cargo +nightly fuzz run parse_strict fuzz/corpus/parse_strict -- \
  -dict=fuzz/cypher.dict -runs=0 -max_len=65536 -timeout=5
```

Targets cover the lexer, strict parser, recovery, a small independent
grammar-aware valid-query generator, and mutations based on every unique query
in the generated TCK syntax projection. The TCK mutation target validates the
projection header and loads all 4,131 unique records, including both accepted
and rejected syntax expectations. The checked-in corpus contains stable seed
and regression cases; CI-generated corpus growth is uploaded as an artifact and
is not committed automatically.

Minimize a failure before turning it into a deterministic integration test:

```console
cargo +nightly fuzz tmin parse_strict fuzz/artifacts/parse_strict/crash-...
```

Passing a timed fuzz run means only that no failure was found in that campaign.
It is not proof of language conformance or memory safety.
