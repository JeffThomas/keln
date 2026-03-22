pub mod env;
pub mod check;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::fmt;

use crate::ast::Span;

// =============================================================================
// Resolved Type — the internal type representation used by the checker
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Bytes,
    Unit,
    Never,

    /// Named user-defined type (after resolving type aliases).
    Named(String),

    /// Generic type application: Result<Int, PortError> => Generic("Result", [Int, Named("PortError")])
    Generic {
        name: String,
        args: Vec<Type>,
    },

    /// Anonymous product (record) type: { x: Int, y: Int }
    Record(Vec<(String, Type)>),

    /// FunctionRef<Effects, Input, Output>
    FunctionRef {
        effects: EffectSet,
        input: Box<Type>,
        output: Box<Type>,
    },

    /// Sum type (not directly constructed; stored in TypeDef registry).
    /// Used when pattern matching to know available variants.
    /// The type checker refers to the named type, not the sum directly.

    /// A type variable (used during generic instantiation).
    TypeVar(String),

    /// Channel<T>
    Channel(Box<Type>),

    /// Task<T>
    Task(Box<Type>),

    /// List<T> — sugar for Generic("List", [T]) but useful to have explicit
    List(Box<Type>),
}

impl Type {
    pub fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Float => write!(f, "Float"),
            Type::Bool => write!(f, "Bool"),
            Type::String => write!(f, "String"),
            Type::Bytes => write!(f, "Bytes"),
            Type::Unit => write!(f, "Unit"),
            Type::Never => write!(f, "Never"),
            Type::Named(n) => write!(f, "{}", n),
            Type::Generic { name, args } => {
                write!(f, "{}<", name)?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", a)?;
                }
                write!(f, ">")
            }
            Type::Record(fields) => {
                write!(f, "{{ ")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", name, ty)?;
                }
                write!(f, " }}")
            }
            Type::FunctionRef { effects, input, output } => {
                write!(f, "FunctionRef<{}, {}, {}>", effects, input, output)
            }
            Type::TypeVar(v) => write!(f, "{}", v),
            Type::Channel(t) => write!(f, "Channel<{}>", t),
            Type::Task(t) => write!(f, "Task<{}>", t),
            Type::List(t) => write!(f, "List<{}>", t),
        }
    }
}

// =============================================================================
// Effect set — resolved set of effect names
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSet {
    pub effects: BTreeSet<String>,
}

impl EffectSet {
    pub fn pure_set() -> Self {
        EffectSet { effects: BTreeSet::new() }
    }

    pub fn from_names(names: &[String]) -> Self {
        let effects: BTreeSet<String> = names.iter()
            .filter(|n| n.as_str() != "Pure")
            .cloned()
            .collect();
        EffectSet { effects }
    }

    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }

    /// Check if `self` is a subset of (compatible with) `other`.
    /// Pure (empty set) is a subset of every set automatically.
    pub fn is_subset_of(&self, other: &EffectSet) -> bool {
        self.effects.is_subset(&other.effects)
    }

    pub fn union(&self, other: &EffectSet) -> EffectSet {
        let effects: BTreeSet<String> =
            self.effects.union(&other.effects).cloned().collect();
        EffectSet { effects }
    }
}

impl fmt::Display for EffectSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.effects.is_empty() {
            return write!(f, "Pure");
        }
        let names: Vec<&String> = self.effects.iter().collect();
        for (i, n) in names.iter().enumerate() {
            if i > 0 { write!(f, " & ")?; }
            write!(f, "{}", n)?;
        }
        Ok(())
    }
}

// =============================================================================
// Type errors
// =============================================================================

#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

impl TypeError {
    pub fn new(msg: impl Into<String>, span: &Span) -> Self {
        TypeError { message: msg.into(), span: span.clone() }
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "type error at {}:{}: {}", self.span.line, self.span.column, self.message)
    }
}

impl std::error::Error for TypeError {}

// =============================================================================
// Stored type definitions — what a `type Foo = ...` expands to
// =============================================================================

#[derive(Debug, Clone)]
pub enum TypeDef {
    /// Sum type: type Maybe<T> = Some(T) | None
    Sum {
        type_params: Vec<String>,
        variants: Vec<VariantDef>,
    },
    /// Product type: type Point = { x: Int, y: Int }
    Product {
        type_params: Vec<String>,
        fields: Vec<(String, Type)>,
    },
    /// Refinement: type Port = Int where 1..65535
    Refinement {
        type_params: Vec<String>,
        base: Type,
    },
    /// Alias: type Ports = List<Port>
    Alias {
        type_params: Vec<String>,
        target: Type,
    },
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub payload: VariantPayload,
}

#[derive(Debug, Clone)]
pub enum VariantPayload {
    Unit,
    Tuple(Type),
    Record(Vec<(String, Type)>),
}

// =============================================================================
// Stored function signatures
// =============================================================================

#[derive(Debug, Clone)]
pub struct FnSig {
    pub effects: EffectSet,
    pub input: Type,
    pub output: Type,
}

// =============================================================================
// Public API
// =============================================================================

/// Type-check a parsed Keln program. Returns a list of type errors (empty = success).
pub fn check_program(program: &crate::ast::Program) -> Vec<TypeError> {
    let mut checker = check::Checker::new();
    checker.check(program);
    checker.errors
}

/// Parse and type-check Keln source in one call. Returns errors from either phase.
pub fn check_source(source: &str) -> Result<Vec<TypeError>, String> {
    let program = crate::parser::parse(source)
        .map_err(|e| format!("{}", e))?;
    Ok(check_program(&program))
}
