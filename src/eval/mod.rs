pub mod env;
pub mod eval;
pub mod fingerprint;
pub mod stdlib;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod integration_tests;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;

pub use eval::Evaluator;

// =============================================================================
// Value — runtime representation of Keln values
// =============================================================================

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Unit,
    List(Vec<Value>),
    /// Product type or anonymous record: ordered field list
    Record(Vec<(String, Value)>),
    /// Sum type variant: Ok(5), None, Running { attempt: 1 }
    Variant { name: String, payload: VariantPayload },
    /// First-class reference to a named function
    FnRef(String),
    /// Partially applied function via .with
    PartialFn { name: String, bound: Vec<(String, Value)> },
    /// Synchronous channel (single-threaded; swap to Arc/Mutex for Tokio later)
    Channel(Rc<RefCell<VecDeque<Value>>>),
    /// Duration in milliseconds
    Duration(i64),
    /// Unix timestamp in milliseconds
    Timestamp(i64),
    /// Completed task result (sync model)
    Task(Box<Value>),
    /// Ordered key-value map (linear scan; keys compared by PartialEq)
    Map(Vec<(Value, Value)>),
    /// Ordered unique set (linear scan; elements compared by PartialEq)
    Set(Vec<Value>),
}

#[derive(Debug, Clone)]
pub enum VariantPayload {
    Unit,
    Tuple(Box<Value>),
    Record(Vec<(String, Value)>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Duration(a), Value::Duration(b)) => a == b,
            (Value::Timestamp(a), Value::Timestamp(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
            }
            (Value::Set(a), Value::Set(b)) => {
                a.len() == b.len() && a.iter().all(|x| b.contains(x))
            }
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Record(a), Value::Record(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
            }
            (
                Value::Variant { name: n1, payload: p1 },
                Value::Variant { name: n2, payload: p2 },
            ) => n1 == n2 && p1 == p2,
            _ => false,
        }
    }
}

impl PartialEq for VariantPayload {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (VariantPayload::Unit, VariantPayload::Unit) => true,
            (VariantPayload::Tuple(a), VariantPayload::Tuple(b)) => a == b,
            (VariantPayload::Record(a), VariantPayload::Record(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
            }
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Str(s) => write!(f, "{}", s),
            Value::Bytes(b) => write!(f, "<bytes:{}>", b.len()),
            Value::Unit => write!(f, "Unit"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Record(fields) => {
                write!(f, "{{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, " }}")
            }
            Value::Variant { name, payload } => match payload {
                VariantPayload::Unit => write!(f, "{}", name),
                VariantPayload::Tuple(v) => write!(f, "{}({})", name, v),
                VariantPayload::Record(fields) => {
                    write!(f, "{} {{ ", name)?;
                    for (i, (k, v)) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}: {}", k, v)?;
                    }
                    write!(f, " }}")
                }
            },
            Value::FnRef(name) => write!(f, "<fn:{}>", name),
            Value::PartialFn { name, .. } => write!(f, "<partial:{}>", name),
            Value::Channel(_) => write!(f, "<channel>"),
            Value::Duration(ms) => write!(f, "<duration:{}ms>", ms),
            Value::Timestamp(ms) => write!(f, "<timestamp:{}>", ms),
            Value::Task(v) => write!(f, "<task:{}>", v),
            Value::Map(pairs) => {
                write!(f, "Map{{")?;
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Set(items) => {
                write!(f, "Set{{")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

// =============================================================================
// RuntimeError
// =============================================================================

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    pub span: Option<crate::ast::Span>,
}

impl RuntimeError {
    pub fn new(msg: impl Into<String>) -> Self {
        RuntimeError { message: msg.into(), span: None }
    }

    pub fn at(msg: impl Into<String>, span: &crate::ast::Span) -> Self {
        RuntimeError { message: msg.into(), span: Some(span.clone()) }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.span {
            Some(s) => write!(f, "runtime error at {}:{}: {}", s.line, s.column, self.message),
            None => write!(f, "runtime error: {}", self.message),
        }
    }
}

impl std::error::Error for RuntimeError {}

// =============================================================================
// Trampoline for TCO
// =============================================================================

pub(crate) enum Thunk {
    Value(Value),
    TailCall { fn_name: String, arg: Value },
}

// =============================================================================
// Public API
// =============================================================================

/// Parse and load a Keln source string, returning a ready-to-call Evaluator.
pub fn load_source(source: &str) -> Result<Evaluator, String> {
    let program = crate::parser::parse(source).map_err(|e| format!("{}", e))?;
    let mut ev = Evaluator::new();
    ev.load_program(&program);
    Ok(ev)
}

/// Parse source, call a named function with a single Value argument.
pub fn eval_fn(source: &str, fn_name: &str, arg: Value) -> Result<Value, String> {
    let mut ev = load_source(source)?;
    ev.call_fn(fn_name, arg).map_err(|e| format!("{}", e))
}
