//! A fully owned, syntax-oriented openCypher abstract syntax tree.
//!
//! The tree describes what was written; it does not perform name resolution,
//! type checking, query planning, or execution. Every syntactic node carries a
//! half-open UTF-8 byte [`Span`]. Trivia remains available from the lossless
//! token stream and is not attached to AST nodes.

use crate::span::Span;

/// A syntax value and its source extent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Node<T> {
    /// The syntax represented by this node.
    pub kind: T,
    /// The full source extent, excluding surrounding trivia.
    pub span: Span,
}

impl<T> Node<T> {
    /// Creates a spanned syntax node.
    #[must_use]
    pub const fn new(kind: T, span: Span) -> Self {
        Self { kind, span }
    }

    /// Converts the contained value while preserving its span.
    #[must_use]
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Node<U> {
        Node::new(map(self.kind), self.span)
    }

    /// Borrows the contained value while preserving its span.
    #[must_use]
    pub const fn as_ref(&self) -> Node<&T> {
        Node::new(&self.kind, self.span)
    }
}

/// A complete source input containing zero or one query statement.
///
/// The parser accepts an optional trailing semicolon. The statement vector is
/// empty for empty or trivia-only input and otherwise contains one statement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Program {
    /// Statements in source order.
    pub statements: Vec<Statement>,
    /// The complete non-trivia source extent.
    pub span: Span,
}

impl Program {
    /// Creates a program.
    #[must_use]
    pub const fn new(statements: Vec<Statement>, span: Span) -> Self {
        Self { statements, span }
    }

    /// Returns `true` when the input contains no statements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}

/// A top-level statement.
pub type Statement = Node<StatementKind>;

/// The syntactic form of a top-level statement.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StatementKind {
    /// A regular query, optionally prefixed by an execution mode.
    Query(QueryStatement),
    /// A placeholder emitted by recovery.
    Error(ErrorNode),
}

/// A query statement and its optional execution prefix.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct QueryStatement {
    /// `EXPLAIN` or `PROFILE`, when present.
    pub mode: Option<Node<ExecutionMode>>,
    /// The query body.
    pub query: Query,
    /// A trailing semicolon, when one was written.
    pub terminator: Option<Span>,
}

/// A query execution prefix.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExecutionMode {
    Explain,
    Profile,
}

/// A regular query.
pub type Query = Node<QueryKind>;

/// The syntactic form of a query.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QueryKind {
    /// One single query followed by zero or more `UNION` branches.
    Regular(RegularQuery),
    /// A placeholder emitted by recovery.
    Error(ErrorNode),
}

/// A query connected by zero or more set unions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RegularQuery {
    /// The first single query.
    pub head: SingleQuery,
    /// Subsequent union branches.
    pub unions: Vec<UnionBranch>,
}

/// A `UNION` operator and the query to its right.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnionBranch {
    /// Union duplicate policy.
    pub operator: Node<UnionOperator>,
    /// Query on the right-hand side.
    pub query: SingleQuery,
}

/// Duplicate handling for a query union.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UnionOperator {
    /// Bare `UNION`, whose duplicate behavior is the openCypher default.
    Default,
    /// `UNION ALL`.
    All,
    /// `UNION DISTINCT`.
    Distinct,
}

/// Duplicate handling explicitly written for a projection or function call.
///
/// The absence of this node represents the grammar's default behavior. Keeping
/// `ALL` distinct from absence preserves the source-level syntax for tools that
/// inspect or rewrite queries.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SetQuantifier {
    All,
    Distinct,
}

/// A sequence of query clauses evaluated from left to right.
pub type SingleQuery = Node<SingleQueryKind>;

/// The contents of a single query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SingleQueryKind {
    /// Clauses in source order.
    pub clauses: Vec<Clause>,
}

/// A query clause.
pub type Clause = Node<ClauseKind>;

/// Any clause in an openCypher query.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClauseKind {
    Use(UseClause),
    Match(MatchClause),
    Unwind(UnwindClause),
    Let(LetClause),
    Create(CreateClause),
    Insert(InsertClause),
    Merge(MergeClause),
    Delete(DeleteClause),
    Set(SetClause),
    Remove(RemoveClause),
    Foreach(ForeachClause),
    LoadCsv(LoadCsvClause),
    Call(CallClause),
    With(WithClause),
    Return(ReturnClause),
    Select(ProjectionClause),
    Next(ProjectionClause),
    Filter(FilterClause),
    Finish,
    /// A placeholder emitted after synchronizing at a clause boundary.
    Error(ErrorNode),
}

/// Selects the graph against which the following query runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UseClause {
    pub graph: Expr,
}

/// A `MATCH` clause.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MatchClause {
    pub optional: bool,
    pub mode: Option<Node<PathMode>>,
    pub pattern: Pattern,
    pub hints: Vec<MatchHint>,
    pub where_clause: Option<Expr>,
}

/// A planner hint attached to `MATCH`.
pub type MatchHint = Node<MatchHintKind>;

/// The form of a planner hint.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MatchHintKind {
    Index {
        variable: Name,
        label: Name,
        properties: Vec<Name>,
        seek: bool,
    },
    Scan {
        variable: Name,
        label: Name,
    },
    Join {
        variables: Vec<Name>,
    },
}

/// Expands a list expression into rows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnwindClause {
    pub expression: Expr,
    pub variable: Name,
}

/// Introduces one or more variable bindings.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LetClause {
    pub bindings: Vec<Binding>,
}

/// An expression bound to a variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Binding {
    pub variable: Name,
    pub expression: Expr,
    pub span: Span,
}

/// Creates graph elements described by a pattern.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CreateClause {
    pub pattern: Pattern,
}

/// Inserts graph elements using GQL-aligned pattern syntax.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InsertClause {
    pub pattern: Pattern,
}

/// Matches or creates a single pattern and applies conditional updates.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MergeClause {
    pub pattern: PatternPart,
    pub actions: Vec<MergeAction>,
}

/// An `ON MATCH` or `ON CREATE` action.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MergeAction {
    pub trigger: Node<MergeTrigger>,
    pub set: SetClause,
    pub span: Span,
}

/// The event that triggers a merge action.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MergeTrigger {
    Match,
    Create,
}

/// Deletes values or graph elements.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeleteClause {
    pub mode: Node<DeleteMode>,
    pub expressions: Vec<Expr>,
}

/// Relationship handling for a delete operation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeleteMode {
    Normal,
    Detach,
    Nodetach,
}

/// A collection of assignments following `SET`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SetClause {
    pub items: Vec<SetItem>,
}

/// A single `SET` assignment.
pub type SetItem = Node<SetItemKind>;

/// The form of an assignment.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SetItemKind {
    Property {
        target: Expr,
        value: Expr,
    },
    ReplaceProperties {
        target: Name,
        value: Expr,
    },
    MergeProperties {
        target: Name,
        value: Expr,
    },
    Labels {
        target: Name,
        labels: LabelExpression,
    },
}

/// A collection of removals following `REMOVE`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RemoveClause {
    pub items: Vec<RemoveItem>,
}

/// A single `REMOVE` operation.
pub type RemoveItem = Node<RemoveItemKind>;

/// The form of a removal.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RemoveItemKind {
    Property(Expr),
    Labels {
        target: Name,
        labels: LabelExpression,
    },
}

/// Applies updating clauses for each item in a list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ForeachClause {
    pub variable: Name,
    pub list: Expr,
    pub clauses: Vec<Clause>,
}

/// Loads delimited records from a URL expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoadCsvClause {
    pub with_headers: bool,
    pub source: Expr,
    pub variable: Name,
    pub field_terminator: Option<Node<StringLiteral>>,
}

/// Invokes a procedure or a subquery.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CallClause {
    pub optional: bool,
    pub target: CallTarget,
    pub yield_clause: Option<YieldClause>,
}

/// The target of a `CALL` clause.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CallTarget {
    Procedure {
        name: QualifiedName,
        arguments: Option<Vec<Expr>>,
    },
    Subquery {
        scope: Option<SubqueryScope>,
        query: Box<Query>,
        in_transactions: Option<Box<InTransactions>>,
    },
}

/// Variables imported into a `CALL { ... }` subquery.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SubqueryScope {
    pub import_all: bool,
    pub variables: Vec<Name>,
    pub span: Span,
}

/// Batched transaction behavior for a subquery call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InTransactions {
    pub rows: Option<Expr>,
    pub on_error: Option<Node<TransactionErrorBehavior>>,
    pub report_status_as: Option<Name>,
    pub span: Span,
}

/// Error handling for `CALL { ... } IN TRANSACTIONS`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TransactionErrorBehavior {
    Continue,
    Break,
    Fail,
}

/// Selects values produced by a procedure call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct YieldClause {
    pub all: bool,
    pub items: Vec<ProjectionItem>,
    pub where_clause: Option<Expr>,
    pub span: Span,
}

/// Projects intermediate values and continues a query.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WithClause {
    pub projection: ProjectionClause,
    pub where_clause: Option<Expr>,
}

/// Produces the final rows from a query.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReturnClause {
    pub projection: ProjectionClause,
}

/// A projection body shared by `RETURN`, `WITH`, and GQL-aligned `SELECT`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProjectionClause {
    /// An explicitly written `ALL` or `DISTINCT` modifier.
    pub quantifier: Option<Node<SetQuantifier>>,
    pub items: Vec<ProjectionItem>,
    pub order_by: Option<OrderBy>,
    pub offset: Option<Expr>,
    pub skip: Option<Expr>,
    pub limit: Option<Expr>,
    pub span: Span,
}

/// One expression or wildcard in a projection.
pub type ProjectionItem = Node<ProjectionItemKind>;

/// The form of a projection item.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ProjectionItemKind {
    Wildcard,
    Expression {
        expression: Expr,
        alias: Option<Name>,
    },
}

/// Sort specifications following `ORDER BY`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrderBy {
    pub items: Vec<SortItem>,
    pub span: Span,
}

/// One expression and its requested ordering.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SortItem {
    pub expression: Expr,
    pub direction: Option<Node<SortDirection>>,
    pub nulls: Option<Node<NullOrder>>,
    pub span: Span,
}

/// Sort direction.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Placement of null values in sorted output.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NullOrder {
    First,
    Last,
}

/// Keeps rows satisfying an expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FilterClause {
    pub predicate: Expr,
}

/// A graph pattern.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pattern {
    pub parts: Vec<PatternPart>,
    pub span: Span,
}

/// A selected path pattern and optional path-variable binding.
pub type PatternPart = Node<PatternPartKind>;

/// The contents of a pattern part.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PatternPartKind {
    pub binding: Option<Name>,
    pub selector: Option<Node<PathSelector>>,
    pub path: PathPattern,
}

/// A path pattern composed from node, relationship, and grouped factors.
pub type PathPattern = Node<PathPatternKind>;

/// Ordered factors in a path pattern.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathPatternKind {
    pub factors: Vec<PathFactor>,
}

/// One factor in a path pattern.
pub type PathFactor = Node<PathFactorKind>;

/// The form of a path factor.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PathFactorKind {
    Node(NodePattern),
    /// A node pattern followed by a graph-pattern quantifier.
    QuantifiedNode {
        pattern: NodePattern,
        quantifier: Quantifier,
    },
    Relationship(RelationshipPattern),
    Parenthesized {
        pattern: Box<PathPattern>,
        where_clause: Option<Box<Expr>>,
        quantifier: Option<Quantifier>,
    },
    /// A parenthesized path factor with a subpath-variable binding.
    Subpath {
        binding: Name,
        pattern: Box<PathPattern>,
        where_clause: Option<Box<Expr>>,
        quantifier: Option<Quantifier>,
    },
    /// A legacy `shortestPath(...)` or `allShortestPaths(...)` path factor.
    LegacyShortest {
        all: bool,
        pattern: Box<PathPattern>,
    },
    Error(ErrorNode),
}

/// A node pattern `(variable:Label {properties})`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodePattern {
    pub variable: Option<Name>,
    pub labels: Option<LabelExpression>,
    pub properties: Option<Box<Expr>>,
    pub where_clause: Option<Box<Expr>>,
}

/// A relationship pattern between two nodes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RelationshipPattern {
    pub direction: Node<RelationshipDirection>,
    pub variable: Option<Name>,
    pub labels: Option<LabelExpression>,
    /// A legacy variable-length relationship quantifier written inside `[]`.
    pub legacy_quantifier: Option<Quantifier>,
    /// A graph-pattern quantifier written after the relationship pattern.
    pub graph_quantifier: Option<Quantifier>,
    pub properties: Option<Box<Expr>>,
    pub where_clause: Option<Box<Expr>>,
}

/// Direction indicated by a relationship pattern's arrows.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RelationshipDirection {
    Left,
    Right,
    Undirected,
    Both,
}

/// Repetition applied to a node, relationship, or parenthesized path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Quantifier {
    pub kind: QuantifierKind,
    pub span: Span,
}

/// A path repetition form. Bounds retain their original decimal spelling.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QuantifierKind {
    ZeroOrMore,
    OneOrMore,
    Optional,
    Fixed(String),
    Range {
        lower: Option<String>,
        upper: Option<String>,
    },
}

/// Restrictions on repeated elements in a path.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PathMode {
    Walk,
    Trail,
    Simple,
    Acyclic,
}

/// Selection applied to matching paths.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PathSelector {
    All,
    Any { count: Option<String> },
    AnyShortest,
    AllShortest,
    Shortest { count: Option<String>, groups: bool },
}

/// A label/type boolean expression.
pub type LabelExpression = Node<LabelExpressionKind>;

/// The form of a label/type boolean expression.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LabelExpressionKind {
    Name(Name),
    Wildcard,
    Not(Box<LabelExpression>),
    And(Vec<LabelExpression>),
    Or(Vec<LabelExpression>),
    Parenthesized(Box<LabelExpression>),
    Error(ErrorNode),
}

/// An expression.
pub type Expr = Node<ExprKind>;

/// Any expression in openCypher 2024.3.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExprKind {
    Literal(Literal),
    Variable(Name),
    Parameter(Parameter),
    Wildcard,
    List(Vec<Expr>),
    Map(Vec<MapEntry>),
    MapProjection(MapProjection),
    Unary {
        operator: Node<UnaryOperator>,
        operand: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: Node<BinaryOperator>,
        right: Box<Expr>,
    },
    Is {
        expression: Box<Expr>,
        negated: bool,
        predicate: IsPredicate,
    },
    Property {
        expression: Box<Expr>,
        key: Name,
    },
    DynamicProperty {
        expression: Box<Expr>,
        key: Box<Expr>,
    },
    Index {
        expression: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        expression: Box<Expr>,
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
    },
    Function(FunctionInvocation),
    Case(CaseExpression),
    ListComprehension(ListComprehension),
    PatternComprehension(PatternComprehension),
    Reduce(ReduceExpression),
    QuantifiedPredicate(QuantifiedPredicate),
    Subquery(SubqueryExpression),
    Pattern(Pattern),
    Parenthesized(Box<Expr>),
    Error(ErrorNode),
}

/// A prefix operator.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UnaryOperator {
    Plus,
    Minus,
    Not,
}

/// An infix operator, ordered by the grammar's precedence rules.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BinaryOperator {
    Or,
    Xor,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    RegexMatch,
    In,
    StartsWith,
    EndsWith,
    Contains,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Power,
    Concat,
}

/// A predicate following `IS` or `IS NOT`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IsPredicate {
    Null,
    Typed(TypeRef),
    Normalized(Option<Node<NormalizationForm>>),
    Label(LabelExpression),
}

/// Unicode normalization requested by an expression predicate.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NormalizationForm {
    Nfc,
    Nfd,
    Nfkc,
    Nfkd,
}

/// A function call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FunctionInvocation {
    pub name: QualifiedName,
    /// An explicitly written `ALL` or `DISTINCT` modifier.
    pub quantifier: Option<Node<SetQuantifier>>,
    pub arguments: Vec<Expr>,
}

/// A searched or simple `CASE` expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CaseExpression {
    pub operand: Option<Box<Expr>>,
    pub alternatives: Vec<CaseAlternative>,
    pub else_expression: Option<Box<Expr>>,
}

/// One `WHEN ... THEN ...` arm.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CaseAlternative {
    /// One or more operands following `WHEN`.
    pub when: Vec<Expr>,
    pub then: Expr,
    pub span: Span,
}

/// A list comprehension.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ListComprehension {
    pub variable: Name,
    pub list: Box<Expr>,
    pub predicate: Option<Box<Expr>>,
    pub projection: Option<Box<Expr>>,
}

/// A pattern comprehension.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PatternComprehension {
    pub binding: Option<Name>,
    pub pattern: Pattern,
    pub predicate: Option<Box<Expr>>,
    pub projection: Box<Expr>,
}

/// A left fold over a list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReduceExpression {
    pub accumulator: Name,
    pub initial: Box<Expr>,
    pub variable: Name,
    pub list: Box<Expr>,
    pub expression: Box<Expr>,
}

/// An `ALL`, `ANY`, `NONE`, or `SINGLE` predicate over a list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct QuantifiedPredicate {
    pub kind: Node<QuantifiedPredicateKind>,
    pub variable: Name,
    pub list: Box<Expr>,
    pub predicate: Box<Expr>,
}

/// Quantification applied to a list predicate.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QuantifiedPredicateKind {
    All,
    Any,
    None,
    Single,
}

/// A query nested inside an expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SubqueryExpression {
    pub kind: Node<SubqueryExpressionKind>,
    pub query: Box<Query>,
}

/// The value produced by a subquery expression.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SubqueryExpressionKind {
    Exists,
    Count,
    Collect,
}

/// A map entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MapEntry {
    pub key: Name,
    pub value: Expr,
    pub span: Span,
}

/// A projection derived from an existing map or graph element.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MapProjection {
    pub base: Box<Expr>,
    pub items: Vec<MapProjectionItem>,
}

/// One item in a map projection.
pub type MapProjectionItem = Node<MapProjectionItemKind>;

/// The form of a map projection item.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MapProjectionItemKind {
    Property(Name),
    AllProperties,
    Variable(Name),
    Entry(MapEntry),
}

/// A literal expression.
pub type Literal = Node<LiteralKind>;

/// A literal value as represented in source.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LiteralKind {
    Null,
    Boolean(bool),
    Integer(IntegerLiteral),
    Float(FloatLiteral),
    String(StringLiteral),
}

/// An integer spelling and its radix.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IntegerLiteral {
    /// Digits exactly as written, including any radix prefix.
    pub text: String,
    pub radix: IntegerRadix,
}

/// The radix of an integer literal.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IntegerRadix {
    Decimal,
    Hexadecimal,
    Octal,
}

/// A floating-point spelling preserved without rounding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FloatLiteral {
    pub text: String,
}

/// A decoded string value and its delimiter style.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StringLiteral {
    pub value: String,
    pub quote: QuoteStyle,
}

/// Quote style used for a string literal.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QuoteStyle {
    Single,
    Double,
}

/// A parameter reference without its leading `$`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Parameter {
    pub name: ParameterName,
}

/// The form of a parameter name.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ParameterName {
    Named(String),
    Positional(String),
}

/// An identifier and whether it was backtick escaped.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Identifier {
    /// Decoded, case-preserving identifier text.
    pub text: String,
    /// Whether the source used backtick delimiters.
    pub escaped: bool,
}

impl Identifier {
    /// Creates an identifier.
    #[must_use]
    pub fn new(text: impl Into<String>, escaped: bool) -> Self {
        Self {
            text: text.into(),
            escaped,
        }
    }
}

/// A spanned identifier.
pub type Name = Node<Identifier>;

/// A dot-separated name such as a procedure or function name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct QualifiedName {
    pub parts: Vec<Name>,
    pub span: Span,
}

/// A value type used by typed predicates.
pub type TypeRef = Node<TypeRefKind>;

/// A type expression and its nullability marker.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TypeRefKind {
    pub value: ValueType,
    pub nullable: Option<bool>,
}

/// A value type named by the grammar.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValueType {
    Nothing,
    Null,
    Boolean,
    String,
    Integer,
    Float,
    Decimal,
    Number,
    Date,
    LocalTime,
    ZonedTime,
    LocalDateTime,
    ZonedDateTime,
    Duration,
    Point,
    Node,
    Relationship,
    Map,
    Path,
    List(Box<TypeRef>),
    Any,
    Named(QualifiedName),
}

/// A marker inserted where error recovery skipped malformed syntax.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ErrorNode;
