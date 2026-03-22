/// Keln Abstract Syntax Tree
///
/// All node types derived from keln-grammar-v0.9.ebnf.
/// Span tracks source location for error reporting.

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

// =============================================================================
// 1. Top-level program
// =============================================================================

#[derive(Debug, Clone)]
pub struct Program {
    pub declarations: Vec<TopLevelDecl>,
}

#[derive(Debug, Clone)]
pub enum TopLevelDecl {
    TypeDecl(TypeDecl),
    FnDecl(FnDecl),
    ModuleDecl(ModuleDecl),
    TrustedModuleDecl(TrustedModuleDecl),
    EffectDecl(EffectDecl),
    LetBinding(LetBinding),
}

// =============================================================================
// 2. Type expressions
// =============================================================================

#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// Int, Float, Bool, String, Bytes, Unit
    Primitive(PrimitiveType, Span),
    /// Never
    Never(Span),
    /// A named type reference (UpperCamelCase)
    Named(String, Span),
    /// Generic type: Result<T, E>, List<T>, etc.
    Generic {
        name: String,
        args: Vec<TypeExpr>,
        span: Span,
    },
    /// Structural anonymous record type: { field: Type, ... }
    Product(Vec<FieldTypeDecl>, Span),
    /// FunctionRef<E, In, Out>
    FunctionRef {
        effect: EffectSet,
        input: Box<TypeExpr>,
        output: Box<TypeExpr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveType {
    Int,
    Float,
    Bool,
    String,
    Bytes,
    Unit,
}

#[derive(Debug, Clone)]
pub struct FieldTypeDecl {
    pub name: String,
    pub type_expr: TypeExpr,
    pub refinement: Option<RefinementConstraint>,
    pub span: Span,
}

// =============================================================================
// 3. Effects
// =============================================================================

#[derive(Debug, Clone)]
pub struct EffectSet {
    pub effects: Vec<String>,
    pub span: Span,
}

// =============================================================================
// 4. Type declarations
// =============================================================================

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub def: TypeDef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeDef {
    Sum(Vec<VariantDecl>),
    Product(Vec<FieldTypeDecl>),
    Refinement {
        base: TypeExpr,
        constraint: RefinementConstraint,
    },
    Alias(TypeExpr),
}

#[derive(Debug, Clone)]
pub struct VariantDecl {
    pub name: String,
    pub payload: VariantPayload,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum VariantPayload {
    /// Unit variant: no fields (e.g., GET, None)
    Unit,
    /// Tuple variant: single wrapped type (e.g., Some(T), Ok(T))
    Tuple(TypeExpr),
    /// Record variant: named fields (e.g., Running { job_id: JobId, ... })
    Record(Vec<FieldTypeDecl>),
}

// =============================================================================
// 5. Refinement constraints
// =============================================================================

#[derive(Debug, Clone)]
pub enum RefinementConstraint {
    /// e.g., 1..65535
    Range(Number, Number),
    /// e.g., >= 1, < 100
    Comparison(ComparisonOp, Number),
    /// e.g., len > 0, len == 36
    Length(ComparisonOp, i64),
    /// e.g., matches(RFC5322)
    Format(String),
}

#[derive(Debug, Clone)]
pub enum Number {
    Int(i64),
    Float(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

// =============================================================================
// 6. Function declarations
// =============================================================================

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name: String,
    pub signature: FnSignature,
    pub in_clause: Pattern,
    pub out_clause: Expr,
    pub confidence: Option<Confidence>,
    pub reason: Option<String>,
    pub proves: Option<Vec<LogicExpr>>,
    pub provenance: Option<Provenance>,
    pub verify: Option<Vec<VerifyStmt>>,
    pub helpers: Option<Vec<HelperDecl>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnSignature {
    pub effects: EffectSet,
    pub input_type: TypeExpr,
    pub output_type: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Confidence {
    Auto,
    Simple(f64),
    Structured {
        value: f64,
        variance: f64,
        sources: Vec<ConfidenceSource>,
    },
}

#[derive(Debug, Clone)]
pub enum ConfidenceSource {
    ManualOverride { value: f64, reason: String },
    VerifyCoverage { score: f64, cases: i64, gaps: i64 },
    TrustedBoundary { module: String },
}

#[derive(Debug, Clone)]
pub struct Provenance {
    pub description: String,
    pub pattern_id: Option<String>,
    pub version: Option<i64>,
    pub source: Option<PatternSource>,
    pub uses: Option<i64>,
    pub failures: Option<i64>,
    pub failure_ref: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum PatternSource {
    Verified,
    Unverified,
    Experimental,
}

// =============================================================================
// 7. Verify blocks
// =============================================================================

#[derive(Debug, Clone)]
pub enum VerifyStmt {
    Mock(MockDecl),
    Given(GivenCase),
    ForAll(ForAllProperty),
}

#[derive(Debug, Clone)]
pub struct MockDecl {
    pub name: String,
    pub clauses: Vec<MockClause>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum MockClause {
    /// Module mock: method(patterns) -> expr
    Method {
        method: String,
        patterns: Vec<Pattern>,
        result: Expr,
    },
    /// FunctionRef mock: call(pattern) -> expr
    Call {
        pattern: Pattern,
        result: Expr,
    },
}

#[derive(Debug, Clone)]
pub struct GivenCase {
    pub input: Expr,
    pub expected: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForAllProperty {
    pub bindings: Vec<ForAllBinding>,
    pub body: LogicExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForAllBinding {
    pub name: String,
    pub type_expr: TypeExpr,
    pub refinement: Option<RefinementConstraint>,
    pub span: Span,
}

// =============================================================================
// 8. Logic expressions (forall / proves only)
// =============================================================================

#[derive(Debug, Clone)]
pub enum LogicExpr {
    Comparison {
        left: Expr,
        op: ComparisonOp,
        right: Expr,
    },
    /// "does not crash" form — just an expression with no comparison
    DoesNotCrash(Expr),
    Not(Box<LogicExpr>),
    And(Box<LogicExpr>, Box<LogicExpr>),
    Or(Box<LogicExpr>, Box<LogicExpr>),
    Implies(Box<LogicExpr>, Box<LogicExpr>),
}

// =============================================================================
// 9. Helper declarations
// =============================================================================

#[derive(Debug, Clone)]
pub enum HelperDecl {
    /// name :: effects In -> Out => expr
    Compact {
        name: String,
        effects: EffectSet,
        input_type: TypeExpr,
        output_type: TypeExpr,
        body: Expr,
        promote_threshold: Option<i64>,
        span: Span,
    },
    /// Full fn declaration nested inside helpers block
    Full(FnDecl),
}

// =============================================================================
// 10. Expressions
// =============================================================================

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    IntLiteral(i64, Span),
    FloatLiteral(f64, Span),
    StringLiteral(String, Span),
    BoolLiteral(bool, Span),
    UnitLiteral(Span),

    // Identifiers
    /// lower_snake_case variable reference
    Var(String, Span),
    /// UpperCamelCase type/constructor/module reference
    UpperVar(String, Span),
    /// Qualified name: Module.function, Type.Variant, etc.
    QualifiedName(Vec<String>, Span),
    /// Wildcard _
    Wildcard(Span),

    // Function call: callable(args)
    Call {
        function: Box<Expr>,
        args: Vec<Arg>,
        span: Span,
    },

    // Pipeline: expr |> fn1 |> fn2
    Pipeline {
        left: Box<Expr>,
        steps: Vec<Expr>,
        span: Span,
    },

    // Match
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },

    // Record construction: { field: expr, ... } or Name { field: expr, ... }
    Record {
        name: Option<Box<Expr>>,
        fields: Vec<FieldValue>,
        span: Span,
    },

    // List: [expr, expr, ...]
    List(Vec<Expr>, Span),

    // Do block
    DoBlock {
        stmts: Vec<DoStmt>,
        final_expr: Box<Expr>,
        span: Span,
    },

    // Select
    Select {
        arms: Vec<SelectArm>,
        timeout: Option<TimeoutArm>,
        span: Span,
    },

    // Channel operations
    ChannelSend {
        channel: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    ChannelRecv(Box<Expr>, Span),
    ChannelNew {
        element_type: TypeExpr,
        span: Span,
    },

    // Clone
    Clone(Box<Expr>, Span),

    // Partial application: fn.with(param: value) or fn.with({ ... })
    With {
        function: Box<Expr>,
        binding: WithBinding,
        span: Span,
    },

    // Let-in (inside do blocks and match arms)
    Let(LetBinding),

    // Arithmetic
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },

    // Field access: expr.field
    FieldAccess {
        object: Box<Expr>,
        field: String,
        span: Span,
    },

    // Parenthesized expression
    Paren(Box<Expr>, Span),
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
pub enum Arg {
    Positional(Box<Expr>),
    Named(String, Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct FieldValue {
    pub name: String,
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum DoStmt {
    Expr(Box<Expr>),
    Let(LetBinding),
    ChannelSend { channel: Box<Expr>, value: Box<Expr> },
}

#[derive(Debug, Clone)]
pub struct SelectArm {
    pub binding: String,
    pub channel: Box<Expr>,
    pub body: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TimeoutArm {
    pub duration: Box<Expr>,
    pub body: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum WithBinding {
    /// Named: fn.with(param: value)
    Named(String, Box<Expr>),
    /// Record: fn.with({ field: value, ... })
    Record(Vec<FieldValue>),
}

// =============================================================================
// 11. Patterns
// =============================================================================

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard(Span),
    Binding(String, Span),
    Literal(Box<Expr>),
    /// Unit variant match: e.g., None, GET
    UnitVariant(String, Span),
    /// Tuple variant: e.g., Some(inner), Ok(value)
    TupleVariant {
        name: String,
        inner: Box<Pattern>,
        span: Span,
    },
    /// Record variant: e.g., Running { job_id, ... }
    RecordVariant {
        name: String,
        fields: Vec<FieldPattern>,
        span: Span,
    },
    /// Anonymous record destructure: { field: pattern, ... }
    Record {
        fields: Vec<FieldPattern>,
        span: Span,
    },
    /// List pattern: [a, b, c]
    List(Vec<Pattern>, Span),
    /// Typed binding: name: Type
    Typed {
        name: String,
        type_expr: TypeExpr,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum FieldPattern {
    /// field: pattern
    Named(String, Pattern),
    /// shorthand: binds field to same name
    Shorthand(String),
    /// _ ignore field
    Wildcard,
}

// =============================================================================
// 12. Let bindings
// =============================================================================

#[derive(Debug, Clone)]
pub struct LetBinding {
    pub pattern: Pattern,
    pub type_annotation: Option<TypeExpr>,
    pub value: Box<Expr>,
    pub span: Span,
}

// =============================================================================
// 13. Module declarations
// =============================================================================

#[derive(Debug, Clone)]
pub struct ModuleDecl {
    pub name: String,
    pub requires: Option<Vec<FieldTypeDecl>>,
    pub provides: Vec<ModuleFnSig>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TrustedModuleDecl {
    pub name: String,
    pub provides: Vec<ModuleFnSig>,
    pub reason: String,
    pub fuzz: Option<Vec<FuzzDecl>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ModuleFnSig {
    pub name: String,
    pub effects: EffectSet,
    pub input_type: TypeExpr,
    pub output_type: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FuzzDecl {
    pub fn_name: String,
    pub input_types: Vec<TypeExpr>,
    pub invariant: FuzzInvariant,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum FuzzInvariant {
    CrashesNever,
    ReturnsResult,
    Deterministic,
}

// =============================================================================
// 14. Effect declarations
// =============================================================================

#[derive(Debug, Clone)]
pub struct EffectDecl {
    pub name: String,
    pub methods: Vec<ModuleFnSig>,
    pub span: Span,
}
