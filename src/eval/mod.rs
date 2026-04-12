pub mod env;
#[allow(clippy::module_inception)]
pub mod eval;
pub mod fingerprint;
pub mod stdlib;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod integration_tests;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::rc::Rc;

// =============================================================================
// ChannelInner — backing store for Value::Channel
// =============================================================================

#[derive(Debug, Clone)]
pub struct ChannelInner {
    pub queue:     VecDeque<Value>,
    pub closed:    bool,
    pub closeable: bool,
}

impl ChannelInner {
    pub fn new() -> Self {
        ChannelInner { queue: VecDeque::new(), closed: false, closeable: false }
    }

    pub fn new_closeable() -> Self {
        ChannelInner { queue: VecDeque::new(), closed: false, closeable: true }
    }
}

impl Default for ChannelInner {
    fn default() -> Self { Self::new() }
}

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
    List(Rc<Vec<Value>>),
    /// Product type or anonymous record: ordered field list
    Record(Vec<(String, Value)>),
    /// Sum type variant: Ok(5), None, Running { attempt: 1 }
    Variant { name: String, payload: VariantPayload },
    /// First-class reference to a named function
    FnRef(String),
    /// Partially applied function via .with
    PartialFn { name: String, bound: Vec<(String, Value)> },
    /// Synchronous channel (single-threaded; swap to Arc/Mutex for Tokio later)
    Channel(Rc<RefCell<ChannelInner>>),
    /// Duration in milliseconds
    Duration(i64),
    /// Unix timestamp in milliseconds
    Timestamp(i64),
    /// Completed task result (sync model)
    Task(Box<Value>),
    /// Key-value map backed by BTreeMap for O(log n) operations; Rc for O(1) clone
    Map(Rc<BTreeMap<Value, Value>>),
    /// Unique set backed by BTreeSet for O(log n) operations; Rc for O(1) clone
    Set(Rc<BTreeSet<Value>>),
    /// Compile-time phantom type descriptor — runtime representation of TypeRef<T>.
    /// Value is the type name string (e.g. "JobMessage", "Int").
    TypeRef(String),
    /// Named capturing closure — references a body+env snapshot in the evaluator's closure_table.
    Closure { id: usize },
    /// VM closure produced by closure-lifting in the bytecode backend.
    /// Stores the lifted function index and a snapshot of captured variable values.
    VmClosure { fn_idx: usize, captures: Vec<(String, Value)> },
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
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => a == b,
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

impl Eq for Value {}
impl Eq for VariantPayload {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        fn disc(v: &Value) -> u8 {
            match v {
                Value::Unit => 0,
                Value::Bool(_) => 1,
                Value::Int(_) => 2,
                Value::Float(_) => 3,
                Value::Str(_) => 4,
                Value::Bytes(_) => 5,
                Value::Duration(_) => 6,
                Value::Timestamp(_) => 7,
                Value::List(_) => 8,
                Value::Record(_) => 9,
                Value::Variant { .. } => 10,
                Value::Map(_) => 11,
                Value::Set(_) => 12,
                Value::FnRef(_) => 13,
                Value::PartialFn { .. } => 14,
                Value::Channel(_) => 15,
                Value::Task(_) => 16,
                Value::TypeRef(_) => 17,
                Value::Closure { .. } => 18,
                Value::VmClosure { .. } => 19,
            }
        }
        let d = disc(self).cmp(&disc(other));
        if d != Ordering::Equal { return d; }
        match (self, other) {
            (Value::Unit, Value::Unit) => Ordering::Equal,
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
            (Value::Str(a), Value::Str(b)) => a.cmp(b),
            (Value::Bytes(a), Value::Bytes(b)) => a.cmp(b),
            (Value::Duration(a), Value::Duration(b)) => a.cmp(b),
            (Value::Timestamp(a), Value::Timestamp(b)) => a.cmp(b),
            (Value::List(a), Value::List(b)) => a.cmp(b),
            (Value::Record(a), Value::Record(b)) => {
                let mut a = a.clone(); a.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
                let mut b = b.clone(); b.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
                a.len().cmp(&b.len()).then_with(|| {
                    a.iter().zip(b.iter())
                        .map(|((k1, v1), (k2, v2))| k1.cmp(k2).then_with(|| v1.cmp(v2)))
                        .find(|o| *o != Ordering::Equal)
                        .unwrap_or(Ordering::Equal)
                })
            }
            (Value::Variant { name: n1, payload: p1 }, Value::Variant { name: n2, payload: p2 }) => {
                n1.cmp(n2).then_with(|| p1.cmp(p2))
            }
            (Value::Map(a), Value::Map(b)) => a.cmp(b),
            (Value::Set(a), Value::Set(b)) => a.cmp(b),
            (Value::FnRef(a), Value::FnRef(b)) => a.cmp(b),
            (Value::PartialFn { name: n1, .. }, Value::PartialFn { name: n2, .. }) => n1.cmp(n2),
            (Value::Channel(_), Value::Channel(_)) => Ordering::Equal,
            (Value::Task(a), Value::Task(b)) => a.cmp(b),
            (Value::TypeRef(a), Value::TypeRef(b)) => a.cmp(b),
            (Value::Closure { id: a }, Value::Closure { id: b }) => a.cmp(b),
            (Value::VmClosure { fn_idx: a, .. }, Value::VmClosure { fn_idx: b, .. }) => a.cmp(b),
            _ => unreachable!("discriminants matched but variant arms did not"),
        }
    }
}

impl PartialOrd for VariantPayload {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VariantPayload {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (VariantPayload::Unit, VariantPayload::Unit) => Ordering::Equal,
            (VariantPayload::Unit, _) => Ordering::Less,
            (_, VariantPayload::Unit) => Ordering::Greater,
            (VariantPayload::Tuple(a), VariantPayload::Tuple(b)) => a.cmp(b),
            (VariantPayload::Tuple(_), _) => Ordering::Less,
            (_, VariantPayload::Tuple(_)) => Ordering::Greater,
            (VariantPayload::Record(a), VariantPayload::Record(b)) => {
                let mut a = a.clone(); a.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
                let mut b = b.clone(); b.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
                a.len().cmp(&b.len()).then_with(|| {
                    a.iter().zip(b.iter())
                        .map(|((k1, v1), (k2, v2))| k1.cmp(k2).then_with(|| v1.cmp(v2)))
                        .find(|o| *o != Ordering::Equal)
                        .unwrap_or(Ordering::Equal)
                })
            }
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
            Value::Closure { id } => write!(f, "<closure:{}>", id),
            Value::VmClosure { fn_idx, .. } => write!(f, "<vm-closure:{}>", fn_idx),
            Value::Channel(_) => write!(f, "<channel>"),
            Value::Duration(ms) => write!(f, "<duration:{}ms>", ms),
            Value::Timestamp(ms) => write!(f, "<timestamp:{}>", ms),
            Value::Task(v) => write!(f, "<task:{}>", v),
            Value::Map(map) => {
                write!(f, "Map{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Set(set) => {
                write!(f, "Set{{")?;
                for (i, v) in set.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "}}")
            }
            Value::TypeRef(name) => write!(f, "TypeRef<{}>", name),
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
