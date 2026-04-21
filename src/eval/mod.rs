pub mod env;
#[allow(clippy::module_inception)]
pub mod eval;
pub mod fingerprint;
pub mod stdlib;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod integration_tests;

use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

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
    List(Arc<Vec<Value>>),
    /// Product type or anonymous record: layout index into global interner + positional values
    Record(u32, Vec<Value>),
    /// Sum type variant: Ok(5), None, Running { attempt: 1 }
    Variant { name: String, payload: VariantPayload },
    /// First-class reference to a named function
    FnRef(String),
    /// Partially applied function via .with
    PartialFn { name: String, bound: Vec<(String, Value)> },
    /// Concurrent channel backed by Arc<Mutex<ChannelInner>>
    Channel(Arc<Mutex<ChannelInner>>),
    /// Duration in milliseconds
    Duration(i64),
    /// Unix timestamp in milliseconds
    Timestamp(i64),
    /// Async task handle — wraps a background thread running a Keln function
    Task(Arc<TaskHandle>),
    /// Key-value map backed by BTreeMap for O(log n) operations; Arc for O(1) clone
    Map(Arc<BTreeMap<Value, Value>>),
    /// Unique set backed by BTreeSet for O(log n) operations; Arc for O(1) clone
    Set(Arc<BTreeSet<Value>>),
    /// Compile-time phantom type descriptor — runtime representation of TypeRef<T>.
    /// Value is the type name string (e.g. "JobMessage", "Int").
    TypeRef(String),
    /// Named capturing closure — references a body+env snapshot in the evaluator's closure_table.
    Closure { id: usize },
    /// VM closure produced by closure-lifting in the bytecode backend.
    /// Stores the lifted function index and a snapshot of captured variable values.
    VmClosure { fn_idx: usize, captures: Vec<(String, Value)> },
    /// Min-priority queue — push with explicit Int priority, popMin returns smallest.
    Heap(Arc<MinHeap>),
    /// FIFO queue — O(1) amortised enqueue/dequeue via VecDeque.
    Queue(Arc<VecDeque<Value>>),
}

// Compile-time check: Value must be Send + Sync for cross-thread task spawning.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<Value>();
        _assert_send_sync::<TaskHandle>();
    }
};

/// Backing store for a running or completed Keln task.
#[derive(Debug)]
pub struct TaskHandle {
    /// The result of the computation, set exactly once when the thread finishes.
    pub result: OnceLock<Result<Value, String>>,
    /// The OS thread running the computation (taken on the first await).
    pub thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Debug, Clone)]
pub enum VariantPayload {
    Unit,
    Tuple(Box<Value>),
    Record(u32, Vec<Value>),
}

impl Value {
    /// Construct a record from parallel field names and values.
    pub fn make_record(names: &[&str], values: Vec<Value>) -> Value {
        let owned: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        let layout = intern_layout(&owned);
        Value::Record(layout, values)
    }

    /// Construct a record from owned (name, value) pairs.
    pub fn make_record_from_pairs(pairs: Vec<(String, Value)>) -> Value {
        let names: Vec<String> = pairs.iter().map(|(k, _)| k.clone()).collect();
        let values: Vec<Value> = pairs.into_iter().map(|(_, v)| v).collect();
        let layout = intern_layout(&names);
        Value::Record(layout, values)
    }
}

// =============================================================================
// MinHeap — min-priority queue backing Value::Heap
// =============================================================================

/// A single entry in a `MinHeap`. Ordered by `(priority asc, seq asc)` so that
/// `BinaryHeap` (which is a max-heap) gives us min-priority pop.
#[derive(Debug, Clone)]
pub struct HeapEntry {
    pub priority: i64,
    pub seq:      u64,
    pub value:    Value,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}
impl Eq for HeapEntry {}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Invert priority so BinaryHeap (max-heap) pops smallest first.
        other.priority.cmp(&self.priority)
            .then(other.seq.cmp(&self.seq))
    }
}

/// Arc-wrappable min-heap keyed by explicit integer priority.
#[derive(Debug, Clone)]
pub struct MinHeap {
    pub entries: BinaryHeap<HeapEntry>,
    pub counter: u64,
}

impl MinHeap {
    pub fn new() -> Self { MinHeap { entries: BinaryHeap::new(), counter: 0 } }
}

impl Default for MinHeap {
    fn default() -> Self { Self::new() }
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
            (Value::Map(a), Value::Map(b)) => **a == **b,
            (Value::Set(a), Value::Set(b)) => **a == **b,
            (Value::List(a), Value::List(b)) => **a == **b,
            (Value::Queue(a), Value::Queue(b)) => **a == **b,
            (Value::Heap(a), Value::Heap(b)) => Arc::ptr_eq(a, b),
            (Value::Record(la, a), Value::Record(lb, b)) => {
                if la == lb {
                    a == b
                } else {
                    if a.len() != b.len() { return false; }
                    let names_a = fields_of_layout(*la);
                    a.iter().zip(names_a.iter()).all(|(v, name)| {
                        field_pos(*lb, name).and_then(|pos| b.get(pos)) == Some(v)
                    })
                }
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
            (VariantPayload::Record(la, a), VariantPayload::Record(lb, b)) => {
                if la == lb {
                    a == b
                } else {
                    if a.len() != b.len() { return false; }
                    let names_a = fields_of_layout(*la);
                    a.iter().zip(names_a.iter()).all(|(v, name)| {
                        field_pos(*lb, name).and_then(|pos| b.get(pos)) == Some(v)
                    })
                }
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
                Value::Record(_, _) => 9,
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
                Value::Heap(_) => 20,
                Value::Queue(_) => 21,
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
            (Value::List(a), Value::List(b)) => a.as_slice().cmp(b.as_slice()),
            (Value::Record(la, a), Value::Record(lb, b)) => {
                let names_a = fields_of_layout(*la);
                let names_b = fields_of_layout(*lb);
                let mut pairs_a: Vec<(&str, &Value)> = names_a.iter().map(String::as_str).zip(a.iter()).collect();
                let mut pairs_b: Vec<(&str, &Value)> = names_b.iter().map(String::as_str).zip(b.iter()).collect();
                pairs_a.sort_by_key(|(k1, _)| *k1);
                pairs_b.sort_by_key(|(k1, _)| *k1);
                pairs_a.len().cmp(&pairs_b.len()).then_with(|| {
                    pairs_a.iter().zip(pairs_b.iter())
                        .map(|((k1, v1), (k2, v2))| k1.cmp(k2).then_with(|| v1.cmp(v2)))
                        .find(|o| *o != Ordering::Equal)
                        .unwrap_or(Ordering::Equal)
                })
            }
            (Value::Variant { name: n1, payload: p1 }, Value::Variant { name: n2, payload: p2 }) => {
                n1.cmp(n2).then_with(|| p1.cmp(p2))
            }
            (Value::Map(a), Value::Map(b)) => a.iter().cmp(b.iter()),
            (Value::Set(a), Value::Set(b)) => a.iter().cmp(b.iter()),
            (Value::FnRef(a), Value::FnRef(b)) => a.cmp(b),
            (Value::PartialFn { name: n1, .. }, Value::PartialFn { name: n2, .. }) => n1.cmp(n2),
            (Value::Channel(_), Value::Channel(_)) => Ordering::Equal,
            (Value::Task(a), Value::Task(b)) => Arc::as_ptr(a).cast::<()>().cmp(&Arc::as_ptr(b).cast::<()>()),
            (Value::TypeRef(a), Value::TypeRef(b)) => a.cmp(b),
            (Value::Closure { id: a }, Value::Closure { id: b }) => a.cmp(b),
            (Value::VmClosure { fn_idx: a, .. }, Value::VmClosure { fn_idx: b, .. }) => a.cmp(b),
            (Value::Heap(a), Value::Heap(b)) => Arc::as_ptr(a).cast::<()>().cmp(&Arc::as_ptr(b).cast::<()>()),
            (Value::Queue(a), Value::Queue(b)) => (**a).iter().cmp((**b).iter()),
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
            (VariantPayload::Record(la, a), VariantPayload::Record(lb, b)) => {
                let names_a = fields_of_layout(*la);
                let names_b = fields_of_layout(*lb);
                let mut pairs_a: Vec<(&str, &Value)> = names_a.iter().map(String::as_str).zip(a.iter()).collect();
                let mut pairs_b: Vec<(&str, &Value)> = names_b.iter().map(String::as_str).zip(b.iter()).collect();
                pairs_a.sort_by_key(|(k1, _)| *k1);
                pairs_b.sort_by_key(|(k1, _)| *k1);
                pairs_a.len().cmp(&pairs_b.len()).then_with(|| {
                    pairs_a.iter().zip(pairs_b.iter())
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
            Value::Record(layout, values) => {
                write!(f, "{{ ")?;
                let names = fields_of_layout(*layout);
                for (i, (k, v)) in names.iter().zip(values.iter()).enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, " }}")
            }
            Value::Variant { name, payload } => match payload {
                VariantPayload::Unit => write!(f, "{}", name),
                VariantPayload::Tuple(v) => write!(f, "{}({})", name, v),
                VariantPayload::Record(layout, values) => {
                    write!(f, "{} {{ ", name)?;
                    let names = fields_of_layout(*layout);
                    for (i, (k, v)) in names.iter().zip(values.iter()).enumerate() {
                        if i > 0 { write!(f, ", ")?; }
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
            Value::Task(h) => match h.result.get() {
                Some(Ok(v)) => write!(f, "<task:done:{}>", v),
                Some(Err(e)) => write!(f, "<task:err:{}>", e),
                None => write!(f, "<task:pending>"),
            },
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
            Value::Queue(q) => {
                write!(f, "Queue[")?;
                for (i, v) in q.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Heap(h) => write!(f, "<heap:{}>", h.entries.len()),
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
    let program = Arc::new(program);
    let mut ev = Evaluator::new();
    ev.load_program(&program);
    ev.program = Some(program);
    Ok(ev)
}

/// Parse source, call a named function with a single Value argument.
pub fn eval_fn(source: &str, fn_name: &str, arg: Value) -> Result<Value, String> {
    let mut ev = load_source(source)?;
    ev.call_fn(fn_name, arg).map_err(|e| format!("{}", e))
}

// =============================================================================
// Record layout interner — global canonical table for field-name → index mapping
// =============================================================================

struct RecordInterner {
    by_idx: Vec<Vec<String>>,
    by_fields: HashMap<Vec<String>, u32>,
}

impl RecordInterner {
    fn new() -> Self {
        RecordInterner { by_idx: Vec::new(), by_fields: HashMap::new() }
    }

    fn intern(&mut self, fields: &[String]) -> u32 {
        if let Some(&idx) = self.by_fields.get(fields) {
            return idx;
        }
        let owned = fields.to_vec();
        let idx = self.by_idx.len() as u32;
        self.by_fields.insert(owned.clone(), idx);
        self.by_idx.push(owned);
        idx
    }

    fn fields_of(&self, idx: u32) -> Option<&[String]> {
        self.by_idx.get(idx as usize).map(|v| v.as_slice())
    }

    fn field_pos(&self, idx: u32, name: &str) -> Option<usize> {
        self.by_idx.get(idx as usize)?.iter().position(|f| f == name)
    }
}

static RECORD_INTERNER: std::sync::LazyLock<RwLock<RecordInterner>> =
    std::sync::LazyLock::new(|| RwLock::new(RecordInterner::new()));

/// Register a list of field names in canonical order, returning a stable layout index.
/// The same field names in the same order always return the same index (global).
pub fn intern_layout(fields: &[String]) -> u32 {
    RECORD_INTERNER.write().unwrap().intern(fields)
}

/// Look up field names for a layout index. Returns an empty vec for unknown indices.
pub fn fields_of_layout(idx: u32) -> Vec<String> {
    RECORD_INTERNER.read().unwrap().fields_of(idx).unwrap_or(&[]).to_vec()
}

/// Find the positional index of a named field within a layout. Returns None if not found.
pub fn field_pos(layout_idx: u32, name: &str) -> Option<usize> {
    RECORD_INTERNER.read().unwrap().field_pos(layout_idx, name)
}
