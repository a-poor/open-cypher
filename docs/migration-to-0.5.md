# Migration to 0.5

Version 0.5 adds read-only AST traversal and retains more expression structure
in recovering parses. Public AST shapes and strict syntax acceptance are unchanged.

## Traverse recovered trees

Use `ast::visit::Visitor` to visit every UNION branch and nested query, clause,
expression, pattern, and label expression. The default walkers report all six
error-node categories through `visit_error`, including empty insertion spans.

```rust
use open_cypher::{ast::visit::Visitor, parse_recovering, Span};

#[derive(Default)]
struct Errors(Vec<Span>);
impl<'ast> Visitor<'ast> for Errors {
    fn visit_error(&mut self, span: Span) {
        self.0.push(span);
    }
}

let parsed = parse_recovering("RETURN 1 +").value.unwrap();
let mut errors = Errors::default();
errors.visit_program(&parsed.program);
assert_eq!(errors.0, vec![Span::empty(10)]);
```

Override a node hook and call its corresponding `walk_*` function to descend,
or omit that call to prune its subtree. The visitor lifetime permits collecting
borrowed node references. Traversal is depth-first in AST field order, not sorted
span order; it uses stack space proportional to depth. Names, operators, types,
and other metadata are accessible on their owning nodes rather than through
separate hooks.

Continue using parse diagnostics to determine whether input is valid. Error-node
collection is useful for tooling but does not replace lexical/syntax diagnostics.

## Additional expression recovery

For `RETURN CASE WHEN true THEN 1 + ELSE 2 END`, 0.4 fell back to a statement
error. Version 0.5 retains the CASE expression and inserts an empty-span
`ExprKind::Error` as the right operand of `+`, at the start of `ELSE`.

The same repair applies immediately after an operator before contextual `WHEN`,
`THEN`, `END`, or comprehension `WHERE` boundaries. For example,
`RETURN [x IN [1,2] + WHERE x > 0 | x]` now retains the comprehension and its
predicate and projection, with an error operand before `WHERE`.

Valid contextual-name clauses still take precedence over repairs. The existing
four-insertion limit per candidate, shared retry budget, and 32-diagnostic cap
remain in place. Recovery does not invent names or close delimiters, and callers
must continue handling clause/statement fallback.
