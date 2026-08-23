# Migrating from 0.1

Version 0.2 replaces the Pest parse tree with a project-owned lexer, AST, and
diagnostic model. It is an intentionally breaking rewrite targeting the
openCypher 2024.3 grammar.

## Parsing

The 0.1 API returned Pest's generated `Pairs<Rule>`:

```rust,ignore
let pairs = open_cypher::parser::parse(source)?;
```

The strict 0.2 entry point returns an owned `ParsedProgram`:

```rust
let parsed = open_cypher::parse("MATCH (n) RETURN n").expect("valid query");
println!("{} tokens", parsed.tokens.len());
```

`ParsedProgram::program` is the typed AST root. `ParsedProgram::tokens` retains
the public token stream and source spans. Neither value borrows the source
string.

## Lexing and recovery

Use `lex` when an application needs tokens even when the complete statement is
not yet valid:

```rust
let outcome = open_cypher::lex("MATCH (n) RETURN n");
assert!(outcome.diagnostics.is_empty());
for token in outcome.tokens {
    println!("{:?} at {:?}", token.kind, token.span);
}
```

Use `parse_recovering` when editors and interactive tools need diagnostics and
a token stream even for incomplete input:

```rust
let outcome = open_cypher::parse_recovering("MATCH (n RETURN n");
for diagnostic in &outcome.diagnostics {
    eprintln!("{:?}: {}", diagnostic.code, diagnostic.message);
}
if let Some(partial) = outcome.value {
    println!("recovered {} tokens", partial.tokens.len());
}
```

The strict API returns `Err(ParseErrors)` if any lexical or syntactic error is
present. The recovering API currently performs whole-input recovery: it always
returns a program root, but a syntax failure replaces the query with an error
statement rather than recovering individual clauses or expressions. Callers
must not treat a present recovered AST as a valid query.

## Removed Pest details

The generated `Rule`, `Pairs`, `Pair`, `CypherParser`, `print_pairs`, and
`parse_string_literal` interfaces are no longer part of the public API. Match
on the typed AST instead of grammar-generator internals. If an application only
needs to validate one statement, use `parse(source).is_ok()`.

## Scope of validation

The parser validates lexical and syntactic structure. Name resolution, variable
scope, function signatures, type checking, aggregation rules, and graph-runtime
behavior are semantic concerns and are not established merely by a successful
parse.
