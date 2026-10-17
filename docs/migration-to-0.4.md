# Recovery changes in 0.4

The 0.4 release preserves more structure in `parse_recovering` results.
The public AST types are unchanged, but consumers must handle error nodes inside
expressions as well as statements and clauses. `parse` still rejects malformed
input; recovery does not make an invalid query valid.

## Before and after

These are abbreviated AST shapes, not serialized Rust values.

For `MATCH (n) RETURN 1 +`:

```text
0.3: Query → [Match, Clause::Error]
0.4: Query → [Match, Return(Binary(Add, Literal(1), Expr::Error))]
```

The error operand has an empty span immediately after `+`. With a following
separator, as in `RETURN [1 +, 2]`, its empty span is at the comma. Valid siblings,
operators, precedence, and original source tokens are retained.

For `RETURN 1 UNION ALL RETURN`:

```text
0.3: Statement::Error
0.4: RegularQuery
       head: Return(Literal(1))
       UNION ALL: Return(Expr::Error)
```

A completely missing UNION branch, as in `RETURN 1 UNION ALL`, instead contains
an empty-span `ClauseKind::Error`. Operator spans and quantifiers are preserved.

## Consumer checklist

- Use `outcome.diagnostics` to decide whether input has errors. A structured AST
  is useful for editor tooling, not evidence of successful syntax validation.
- Visit every UNION branch, every clause, and nested expression children.
  Handle `StatementKind::Error`, `ClauseKind::Error`, and `ExprKind::Error`.
- Treat an empty expression-error span as an insertion point; it contains no
  source text. Do not assume every expression spans at least one token.
- Keep handling clause/statement fallback. Recovery allows at most four operand
  insertions per candidate and charges retries to the shared recovery budget.
  Diagnostics remain capped at 32 per parse.

## Supported examples and boundaries

Missing operands can recover in map values (`RETURN {x: 1 +, y: 2}`), list
comprehension projections (`RETURN [x IN [1, 2] | x +]`), and projection modifiers
(`RETURN 1 ORDER BY 2 +`, or `SKIP`, `OFFSET`, and `LIMIT` in place of `ORDER BY`).
Parenthesized CASE results also recover:
`RETURN CASE WHEN true THEN (1 +) ELSE 2 END`.

Recovery is conservative around contextual names. For example,
`RETURN CASE WHEN true THEN 1 + ELSE 2 END` does not locally recover: `ELSE`
can be consumed as an operand name before the error is detected. The clause
falls back to an error node if other clauses survive, otherwise the statement
falls back to an error node. Missing names and unclosed delimiters likewise
remain fallback cases. Source tokens are never deleted to force a repair.
