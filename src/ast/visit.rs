//! Read-only, depth-first traversal of the AST's structural nodes.
//!
//! Default hooks descend into children in AST field order (vector elements in
//! order), including UNION branches, nested queries, and pattern expressions.
//! This is structural order, not a promise of sorted source spans. Names,
//! literals, operators, types, and other metadata are available on their owning
//! nodes but have no separate hooks. Every error-bearing AST variant is visited.
//!
//! Override a hook and call its `walk_*` function to continue traversal; omit
//! that call to prune the subtree. Walkers visit children, not the supplied node
//! again. Traversal borrows the tree, performs no cloning, and uses the call stack
//! in proportion to tree depth. The lifetime permits retaining node references.
//!
//! ```
//! use open_cypher::{ast::visit::Visitor, parse_recovering, Span};
//!
//! #[derive(Default)]
//! struct Errors(Vec<Span>);
//! impl<'ast> Visitor<'ast> for Errors {
//!     fn visit_error(&mut self, span: Span) { self.0.push(span); }
//! }
//! let parsed = parse_recovering("RETURN 1 +").value.unwrap();
//! let mut errors = Errors::default();
//! errors.visit_program(&parsed.program);
//! assert_eq!(errors.0, vec![Span::empty(10)]);
//! ```

use super::*;

/// Hooks for structural AST nodes. Defaults recursively visit all children.
pub trait Visitor<'ast> {
    /// Visits a program.
    fn visit_program(&mut self, node: &'ast Program) {
        walk_program(self, node);
    }
    /// Visits a statement.
    fn visit_statement(&mut self, node: &'ast Statement) {
        walk_statement(self, node);
    }
    /// Visits a query, including every UNION branch.
    fn visit_query(&mut self, node: &'ast Query) {
        walk_query(self, node);
    }
    /// Visits a sequence of clauses.
    fn visit_single_query(&mut self, node: &'ast SingleQuery) {
        walk_single_query(self, node);
    }
    /// Visits a clause, including nested clauses and queries.
    fn visit_clause(&mut self, node: &'ast Clause) {
        walk_clause(self, node);
    }
    /// Visits an expression.
    fn visit_expr(&mut self, node: &'ast Expr) {
        walk_expr(self, node);
    }
    /// Visits a graph pattern.
    fn visit_pattern(&mut self, node: &'ast Pattern) {
        walk_pattern(self, node);
    }
    /// Visits a selected or bound path.
    fn visit_pattern_part(&mut self, node: &'ast PatternPart) {
        walk_pattern_part(self, node);
    }
    /// Visits a path's factors.
    fn visit_path_pattern(&mut self, node: &'ast PathPattern) {
        walk_path_pattern(self, node);
    }
    /// Visits a node, relationship, or grouped path factor.
    fn visit_path_factor(&mut self, node: &'ast PathFactor) {
        walk_path_factor(self, node);
    }
    /// Visits a label/type boolean expression.
    fn visit_label_expression(&mut self, node: &'ast LabelExpression) {
        walk_label_expression(self, node);
    }
    /// Visits an error placeholder's span, including empty insertion spans.
    ///
    /// Called by the default walker of each error-bearing node. Override that
    /// node's hook if the error's syntactic category is also needed.
    fn visit_error(&mut self, _span: Span) {}
}

/// Walks a program's statements.
pub fn walk_program<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast Program) {
    for statement in &node.statements {
        v.visit_statement(statement);
    }
}

/// Walks a statement's query or error.
pub fn walk_statement<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast Statement) {
    match &node.kind {
        StatementKind::Query(statement) => v.visit_query(&statement.query),
        StatementKind::Error(_) => v.visit_error(node.span),
    }
}

/// Walks a query's head and subsequent UNION branches, or its error.
pub fn walk_query<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast Query) {
    match &node.kind {
        QueryKind::Regular(query) => {
            v.visit_single_query(&query.head);
            for branch in &query.unions {
                v.visit_single_query(&branch.query);
            }
        }
        QueryKind::Error(_) => v.visit_error(node.span),
    }
}

/// Walks a single query's clauses.
pub fn walk_single_query<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast SingleQuery) {
    for clause in &node.kind.clauses {
        v.visit_clause(clause);
    }
}

fn optional_expr<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: Option<&'ast Expr>) {
    if let Some(node) = node {
        v.visit_expr(node);
    }
}

fn items<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, nodes: &'ast [ProjectionItem]) {
    for node in nodes {
        match &node.kind {
            ProjectionItemKind::Expression { expression, .. } => v.visit_expr(expression),
            ProjectionItemKind::Wildcard => {}
        }
    }
}

fn projection<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast ProjectionClause) {
    items(v, &node.items);
    if let Some(order) = &node.order_by {
        for item in &order.items {
            v.visit_expr(&item.expression);
        }
    }
    optional_expr(v, node.offset.as_ref());
    optional_expr(v, node.skip.as_ref());
    optional_expr(v, node.limit.as_ref());
}

fn set<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast SetClause) {
    for item in &node.items {
        match &item.kind {
            SetItemKind::Property { target, value } => {
                v.visit_expr(target);
                v.visit_expr(value);
            }
            SetItemKind::ReplaceProperties { value, .. }
            | SetItemKind::MergeProperties { value, .. } => v.visit_expr(value),
            SetItemKind::Labels { labels, .. } => v.visit_label_expression(labels),
        }
    }
}

/// Walks all structural children of a clause.
pub fn walk_clause<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast Clause) {
    match &node.kind {
        ClauseKind::Use(c) => v.visit_expr(&c.graph),
        ClauseKind::Match(c) => {
            v.visit_pattern(&c.pattern);
            optional_expr(v, c.where_clause.as_ref());
        }
        ClauseKind::Unwind(c) => v.visit_expr(&c.expression),
        ClauseKind::Let(c) => {
            for binding in &c.bindings {
                v.visit_expr(&binding.expression);
            }
        }
        ClauseKind::Create(c) => v.visit_pattern(&c.pattern),
        ClauseKind::Insert(c) => v.visit_pattern(&c.pattern),
        ClauseKind::Merge(c) => {
            v.visit_pattern_part(&c.pattern);
            for action in &c.actions {
                set(v, &action.set);
            }
        }
        ClauseKind::Delete(c) => {
            for expr in &c.expressions {
                v.visit_expr(expr);
            }
        }
        ClauseKind::Set(c) => set(v, c),
        ClauseKind::Remove(c) => {
            for item in &c.items {
                match &item.kind {
                    RemoveItemKind::Property(expr) => v.visit_expr(expr),
                    RemoveItemKind::Labels { labels, .. } => v.visit_label_expression(labels),
                }
            }
        }
        ClauseKind::Foreach(c) => {
            v.visit_expr(&c.list);
            for clause in &c.clauses {
                v.visit_clause(clause);
            }
        }
        ClauseKind::LoadCsv(c) => v.visit_expr(&c.source),
        ClauseKind::Call(c) => {
            match &c.target {
                CallTarget::Procedure { arguments, .. } => {
                    if let Some(args) = arguments {
                        for arg in args {
                            v.visit_expr(arg);
                        }
                    }
                }
                CallTarget::Subquery {
                    query,
                    in_transactions,
                    ..
                } => {
                    v.visit_query(query);
                    if let Some(transaction) = in_transactions {
                        optional_expr(v, transaction.rows.as_ref());
                    }
                }
            }
            if let Some(yield_clause) = &c.yield_clause {
                items(v, &yield_clause.items);
                optional_expr(v, yield_clause.where_clause.as_ref());
            }
        }
        ClauseKind::With(c) => {
            projection(v, &c.projection);
            optional_expr(v, c.where_clause.as_ref());
        }
        ClauseKind::Return(c) => projection(v, &c.projection),
        ClauseKind::Select(c) | ClauseKind::Next(c) => projection(v, c),
        ClauseKind::Filter(c) => v.visit_expr(&c.predicate),
        ClauseKind::Finish => {}
        ClauseKind::Error(_) => v.visit_error(node.span),
    }
}

/// Walks an expression's structural children, or its error.
pub fn walk_expr<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast Expr) {
    match &node.kind {
        ExprKind::Literal(_)
        | ExprKind::Variable(_)
        | ExprKind::Parameter(_)
        | ExprKind::Wildcard => {}
        ExprKind::List(exprs) => {
            for expr in exprs {
                v.visit_expr(expr);
            }
        }
        ExprKind::Map(entries) => {
            for entry in entries {
                v.visit_expr(&entry.value);
            }
        }
        ExprKind::MapProjection(map) => {
            v.visit_expr(&map.base);
            for item in &map.items {
                match &item.kind {
                    MapProjectionItemKind::Entry(entry) => v.visit_expr(&entry.value),
                    MapProjectionItemKind::Property(_)
                    | MapProjectionItemKind::AllProperties
                    | MapProjectionItemKind::Variable(_) => {}
                }
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => v.visit_expr(operand),
        ExprKind::Binary { left, right, .. } => {
            v.visit_expr(left);
            v.visit_expr(right);
        }
        ExprKind::Is {
            expression,
            predicate,
            ..
        } => {
            v.visit_expr(expression);
            match predicate {
                IsPredicate::Label(labels) => v.visit_label_expression(labels),
                IsPredicate::Null | IsPredicate::Typed(_) | IsPredicate::Normalized(_) => {}
            }
        }
        ExprKind::Property { expression, .. } => v.visit_expr(expression),
        ExprKind::DynamicProperty { expression, key } => {
            v.visit_expr(expression);
            v.visit_expr(key);
        }
        ExprKind::Index { expression, index } => {
            v.visit_expr(expression);
            v.visit_expr(index);
        }
        ExprKind::Slice {
            expression,
            lower,
            upper,
        } => {
            v.visit_expr(expression);
            optional_expr(v, lower.as_deref());
            optional_expr(v, upper.as_deref());
        }
        ExprKind::Function(call) => {
            for arg in &call.arguments {
                v.visit_expr(arg);
            }
        }
        ExprKind::Case(case) => {
            optional_expr(v, case.operand.as_deref());
            for alternative in &case.alternatives {
                for expr in &alternative.when {
                    v.visit_expr(expr);
                }
                v.visit_expr(&alternative.then);
            }
            optional_expr(v, case.else_expression.as_deref());
        }
        ExprKind::ListComprehension(c) => {
            v.visit_expr(&c.list);
            optional_expr(v, c.predicate.as_deref());
            optional_expr(v, c.projection.as_deref());
        }
        ExprKind::PatternComprehension(c) => {
            v.visit_pattern(&c.pattern);
            optional_expr(v, c.predicate.as_deref());
            v.visit_expr(&c.projection);
        }
        ExprKind::Reduce(c) => {
            v.visit_expr(&c.initial);
            v.visit_expr(&c.list);
            v.visit_expr(&c.expression);
        }
        ExprKind::QuantifiedPredicate(c) => {
            v.visit_expr(&c.list);
            v.visit_expr(&c.predicate);
        }
        ExprKind::Subquery(c) => v.visit_query(&c.query),
        ExprKind::Pattern(pattern) => v.visit_pattern(pattern),
        ExprKind::Error(_) => v.visit_error(node.span),
    }
}

/// Walks a pattern's parts.
pub fn walk_pattern<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast Pattern) {
    for part in &node.parts {
        v.visit_pattern_part(part);
    }
}

/// Walks a pattern part's path.
pub fn walk_pattern_part<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast PatternPart) {
    v.visit_path_pattern(&node.kind.path);
}

/// Walks a path's factors.
pub fn walk_path_pattern<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast PathPattern) {
    for factor in &node.kind.factors {
        v.visit_path_factor(factor);
    }
}

/// Walks a path factor's labels, properties, predicates, and nested paths.
pub fn walk_path_factor<'ast, V: Visitor<'ast> + ?Sized>(v: &mut V, node: &'ast PathFactor) {
    match &node.kind {
        PathFactorKind::Node(pattern) | PathFactorKind::QuantifiedNode { pattern, .. } => {
            if let Some(labels) = &pattern.labels {
                v.visit_label_expression(labels);
            }
            optional_expr(v, pattern.properties.as_deref());
            optional_expr(v, pattern.where_clause.as_deref());
        }
        PathFactorKind::Relationship(pattern) => {
            if let Some(labels) = &pattern.labels {
                v.visit_label_expression(labels);
            }
            optional_expr(v, pattern.properties.as_deref());
            optional_expr(v, pattern.where_clause.as_deref());
        }
        PathFactorKind::Parenthesized {
            pattern,
            where_clause,
            ..
        }
        | PathFactorKind::Subpath {
            pattern,
            where_clause,
            ..
        } => {
            v.visit_path_pattern(pattern);
            optional_expr(v, where_clause.as_deref());
        }
        PathFactorKind::LegacyShortest { pattern, .. } => v.visit_path_pattern(pattern),
        PathFactorKind::Error(_) => v.visit_error(node.span),
    }
}

/// Walks a label expression's children, or its error.
pub fn walk_label_expression<'ast, V: Visitor<'ast> + ?Sized>(
    v: &mut V,
    node: &'ast LabelExpression,
) {
    match &node.kind {
        LabelExpressionKind::Name(_) | LabelExpressionKind::Wildcard => {}
        LabelExpressionKind::Not(child) | LabelExpressionKind::Parenthesized(child) => {
            v.visit_label_expression(child)
        }
        LabelExpressionKind::And(children) | LabelExpressionKind::Or(children) => {
            for child in children {
                v.visit_label_expression(child);
            }
        }
        LabelExpressionKind::Error(_) => v.visit_error(node.span),
    }
}
