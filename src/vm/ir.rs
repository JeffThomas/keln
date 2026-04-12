use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::eval::{RuntimeError, Value};

// =============================================================================
// Register frame
// =============================================================================

pub struct Frame {
    pub regs: Vec<Option<Value>>,
}

impl Frame {
    pub fn new(size: usize) -> Self {
        Frame { regs: vec![None; size] }
    }

    pub fn read(&self, r: usize) -> Result<&Value, RuntimeError> {
        self.regs
            .get(r)
            .and_then(|v| v.as_ref())
            .ok_or_else(|| RuntimeError::new(format!("read from moved register R{}", r)))
    }

    pub fn take(&mut self, r: usize) -> Result<Value, RuntimeError> {
        self.regs
            .get_mut(r)
            .and_then(|v| v.take())
            .ok_or_else(|| RuntimeError::new(format!("move from moved register R{}", r)))
    }

    pub fn clone_reg(&self, r: usize) -> Result<Value, RuntimeError> {
        self.read(r).cloned()
    }

    pub fn write(&mut self, r: usize, v: Value) {
        if r >= self.regs.len() {
            self.regs.resize(r + 1, None);
        }
        debug_assert!(self.regs[r].is_none(), "double-write to R{}", r);
        self.regs[r] = Some(v);
    }
}

// =============================================================================
// Call frame (for explicit call stack — Phase 4b follow-up)
// =============================================================================

pub struct CallFrame {
    pub fn_idx: usize,
    pub ip: usize,
    pub frame: Frame,
    pub dst: usize,
}

// =============================================================================
// Constant table
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Constant {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
}

impl Constant {
    pub fn to_value(&self) -> Value {
        match self {
            Constant::Int(n)  => Value::Int(*n),
            Constant::Float(f) => Value::Float(*f),
            Constant::Bool(b) => Value::Bool(*b),
            Constant::Str(s)  => Value::Str(s.clone()),
            Constant::Unit    => Value::Unit,
        }
    }
}

pub struct ConstantTable {
    pub entries: Vec<Constant>,
    str_idx:  HashMap<String, u32>,
    int_idx:  HashMap<i64, u32>,
    bool_idx: [Option<u32>; 2],   // [false_idx, true_idx]
    unit_idx: Option<u32>,
}

impl Default for ConstantTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstantTable {
    pub fn new() -> Self {
        ConstantTable {
            entries: Vec::new(),
            str_idx: HashMap::new(),
            int_idx: HashMap::new(),
            bool_idx: [None; 2],
            unit_idx: None,
        }
    }

    fn push(&mut self, c: Constant) -> u32 {
        let idx = self.entries.len() as u32;
        self.entries.push(c);
        idx
    }

    pub fn intern_int(&mut self, n: i64) -> u32 {
        if let Some(&i) = self.int_idx.get(&n) { return i; }
        let i = self.push(Constant::Int(n));
        self.int_idx.insert(n, i);
        i
    }

    pub fn intern_float(&mut self, f: f64) -> u32 {
        self.push(Constant::Float(f))
    }

    pub fn intern_bool(&mut self, b: bool) -> u32 {
        let slot = b as usize;
        if let Some(i) = self.bool_idx[slot] { return i; }
        let i = self.push(Constant::Bool(b));
        self.bool_idx[slot] = Some(i);
        i
    }

    pub fn intern_str(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.str_idx.get(s) { return i; }
        let i = self.push(Constant::Str(s.to_string()));
        self.str_idx.insert(s.to_string(), i);
        i
    }

    pub fn intern_unit(&mut self) -> u32 {
        if let Some(i) = self.unit_idx { return i; }
        let i = self.push(Constant::Unit);
        self.unit_idx = Some(i);
        i
    }

    pub fn get(&self, idx: u32) -> &Constant {
        &self.entries[idx as usize]
    }

    /// Rebuild intern-index maps from `entries` after deserialization.
    pub fn rebuild_indices(&mut self) {
        self.str_idx.clear();
        self.int_idx.clear();
        self.bool_idx = [None; 2];
        self.unit_idx = None;
        for (i, c) in self.entries.iter().enumerate() {
            let i = i as u32;
            match c {
                Constant::Str(s)  => { self.str_idx.insert(s.clone(), i); }
                Constant::Int(n)  => { self.int_idx.insert(*n, i); }
                Constant::Bool(b) => { self.bool_idx[*b as usize] = Some(i); }
                Constant::Unit    => { self.unit_idx = Some(i); }
                Constant::Float(_) => {}
            }
        }
    }
}

// =============================================================================
// Variant tag intern table
// =============================================================================

pub struct TagTable {
    pub names: Vec<String>,
    index: HashMap<String, u32>,
}

impl Default for TagTable {
    fn default() -> Self {
        Self::new()
    }
}

impl TagTable {
    pub fn new() -> Self {
        TagTable { names: Vec::new(), index: HashMap::new() }
    }

    pub fn intern(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.index.get(name) { return i; }
        let i = self.names.len() as u32;
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), i);
        i
    }

    pub fn name_of(&self, id: u32) -> &str {
        &self.names[id as usize]
    }

    pub fn lookup(&self, name: &str) -> Option<u32> {
        self.index.get(name).copied()
    }

    /// Insert a name directly (used during deserialization; assumes no duplicates).
    pub fn intern_raw(&mut self, name: String) {
        let i = self.names.len() as u32;
        self.index.insert(name.clone(), i);
        self.names.push(name);
    }
}

// =============================================================================
// Record layout table
// =============================================================================

pub struct RecordLayoutTable {
    /// type_name → ordered list of field names in canonical order
    pub layouts: HashMap<String, Vec<String>>,
    /// layout_idx → type_name (for MAKE_RECORD)
    pub by_idx: Vec<String>,
}

impl Default for RecordLayoutTable {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordLayoutTable {
    pub fn new() -> Self {
        RecordLayoutTable { layouts: HashMap::new(), by_idx: Vec::new() }
    }

    /// Register a named record layout, returning its layout_idx.
    /// Anonymous records are registered under a synthetic name.
    pub fn register(&mut self, type_name: &str, fields: Vec<String>) -> u32 {
        if let Some(pos) = self.by_idx.iter().position(|n| n == type_name) {
            return pos as u32;
        }
        let idx = self.by_idx.len() as u32;
        self.layouts.insert(type_name.to_string(), fields);
        self.by_idx.push(type_name.to_string());
        idx
    }

    /// Register an anonymous record layout keyed by sorted field names.
    pub fn register_anon(&mut self, fields: Vec<String>) -> u32 {
        let key = format!("__anon({})", fields.join(","));
        self.register(&key, fields)
    }

    pub fn field_index(&self, layout_idx: u32, field: &str) -> Option<usize> {
        let name = self.by_idx.get(layout_idx as usize)?;
        self.layouts.get(name)?.iter().position(|f| f == field)
    }

    pub fn fields_of(&self, layout_idx: u32) -> Option<&Vec<String>> {
        let name = self.by_idx.get(layout_idx as usize)?;
        self.layouts.get(name)
    }
}

// =============================================================================
// Builtin dispatch table
// =============================================================================

/// All stdlib qualified names that map to CALL_BUILTIN.
/// Index = u16 passed in the CallBuiltin instruction.
pub static BUILTIN_NAMES: &[&str] = &[
    // Result (0–9)
    "Result.ok", "Result.err", "Result.map", "Result.bind", "Result.mapErr",
    "Result.sequence", "Result.unwrapOr", "Result.isOk", "Result.isErr", "Result.toMaybe",
    // Maybe (10–19)
    "Maybe.some", "Maybe.none", "Maybe.map", "Maybe.bind", "Maybe.getOr",
    "Maybe.isSome", "Maybe.isNone", "Maybe.require", "Maybe.unwrapOr", "",
    // List (20–39)
    "List.map", "List.filter", "List.foldl", "List.find", "List.head",
    "List.tail", "List.isEmpty", "List.length", "List.range", "List.repeat",
    "List.append", "List.prepend", "List.concat", "List.contains", "List.reverse",
    "List.zip", "List.flatten", "List.take", "List.drop", "List.sequence",
    // String (40–59)
    "String.len", "String.trim", "String.concat", "String.contains", "String.split",
    "String.join", "String.toLower", "String.toUpper", "String.startsWith", "String.endsWith",
    "String.fromInt", "String.slice", "String.isEmpty", "String.trimStart", "String.trimEnd",
    "String.chars", "String.indexOf", "String.replace", "String.toString", "String.length",
    // Int (60–69)
    "Int.parse", "Int.toString", "Int.abs", "Int.min", "Int.max",
    "Int.clamp", "Int.toFloat", "Int.pow", "", "",
    // Float (70–84)
    "Float.add", "Float.sub", "Float.multiply", "Float.divide", "Float.pow",
    "Float.abs", "Float.floor", "Float.ceil", "Float.round", "Float.toInt",
    "Float.fromInt", "Float.compare", "Float.approxEq", "Float.parse", "Float.toString",
    // Bool (85–89)
    "Bool.toString", "Bool.not", "Bool.and", "Bool.or", "",
    // Log (90–93)
    "Log.debug", "Log.info", "Log.warn", "Log.error",
    // Duration/Timestamp/Clock (94–109)
    "Duration.ms", "Duration.seconds", "Duration.minutes", "Duration.add", "Duration.multiply",
    "Timestamp.add", "Timestamp.sub", "Timestamp.compare", "Timestamp.gte", "Timestamp.lte",
    "Timestamp.gt", "Timestamp.lt", "Timestamp.eq",
    "Clock.now", "Clock.since", "Clock.after",
    // Bytes (110–113)
    "Bytes.len", "Bytes.empty", "Bytes.fromString", "Bytes.toString",
    // Map (114–123)
    "Map.empty", "Map.insert", "Map.get", "Map.remove", "Map.contains",
    "Map.keys", "Map.values", "Map.toList", "Map.fromList", "Map.size",
    // Set (124–133)
    "Set.empty", "Set.insert", "Set.contains", "Set.remove", "Set.toList",
    "Set.fromList", "Set.union", "Set.intersect", "Set.difference", "Set.size",
    // Env (134–135)
    "Env.get", "Env.require",
    // Json (136–137)
    "Json.parse", "Json.serialize",
    // Http stubs (138–142)
    "Http.get", "Http.post", "Http.put", "Http.delete", "Http.patch",
    // HttpServer/Response/GraphQL (143–147)
    "HttpServer.start", "Response.json", "Response.err", "GraphQL.execute", "GraphQL.query",
    // Task (148–153)
    "Task.spawn", "Task.await", "Task.awaitAll", "Task.awaitFirst", "Task.race", "Task.sequence",
    // Clock.sleep (154)
    "Clock.sleep",
    // Map.merge (155)
    "Map.merge",
    // List aliases (156–159)
    "List.fold", "List.foldr", "List.len", "List.clone",
    // String aliases (160–161)
    "String.lowercase", "String.uppercase",
    // JSON aliases (162–163)
    "JSON.parse", "JSON.serialize",
    // Maybe.unwrapOr already at 18; Result.unwrap (164)
    "Result.unwrap",
    // File I/O (165–166)
    "File.read", "File.readLines",
    // List extras (167–169)
    "List.sort", "List.combinations2", "List.foldUntil",
    // Map.fold (170)
    "Map.fold",
    // List.getOr (171)
    "List.getOr",
    // Map.getOr (172)
    "Map.getOr",
];

pub struct BuiltinTable {
    index: HashMap<String, u16>,
}

impl Default for BuiltinTable {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinTable {
    pub fn new() -> Self {
        let mut index = HashMap::new();
        for (i, &name) in BUILTIN_NAMES.iter().enumerate() {
            if !name.is_empty() {
                index.insert(name.to_string(), i as u16);
            }
        }
        BuiltinTable { index }
    }

    pub fn lookup(&self, name: &str) -> Option<u16> {
        self.index.get(name).copied()
    }

    pub fn name_of(idx: u16) -> &'static str {
        BUILTIN_NAMES.get(idx as usize).copied().unwrap_or("")
    }
}

// =============================================================================
// Instructions
// =============================================================================

/// One arm of a `select` expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectArm {
    /// Register to write the received value into (0 if binding is `_`).
    pub binding_reg: usize,
    /// Register holding the channel.
    pub channel_reg: usize,
    /// Instruction index of the arm body.
    pub body_ip: usize,
    /// Instruction index past the end of the arm body.
    pub end_ip: usize,
}

/// The optional timeout arm of a `select` expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutArm {
    /// Register holding the Duration value.
    pub duration_reg: usize,
    /// Instruction index of the timeout body.
    pub body_ip: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instruction {
    // =========================================================================
    // Load operations
    // =========================================================================
    LoadInt   { dst: usize, val: i64 },
    LoadFloat { dst: usize, val: f64 },
    LoadBool  { dst: usize, val: bool },
    /// Load a string from the constant table.
    LoadStr   { dst: usize, const_idx: u32 },
    LoadUnit  { dst: usize },
    /// Clone a register — source remains valid.
    LoadReg   { dst: usize, src: usize },
    /// Produce a Value::FnRef for a named function (used when passing a function as a value).
    LoadFnRef { dst: usize, name: String },

    // =========================================================================
    // Arithmetic (both sources cloned; Int or Float; type mismatch → RuntimeError)
    // =========================================================================
    Add { dst: usize, src1: usize, src2: usize },
    Sub { dst: usize, src1: usize, src2: usize },
    Mul { dst: usize, src1: usize, src2: usize },
    Div { dst: usize, src1: usize, src2: usize },
    Rem { dst: usize, src1: usize, src2: usize },
    Neg { dst: usize, src: usize },

    // =========================================================================
    // Comparison (sources cloned; result is Bool)
    // =========================================================================
    Eq { dst: usize, src1: usize, src2: usize },
    Ne { dst: usize, src1: usize, src2: usize },
    Lt { dst: usize, src1: usize, src2: usize },
    Le { dst: usize, src1: usize, src2: usize },
    Gt { dst: usize, src1: usize, src2: usize },
    Ge { dst: usize, src1: usize, src2: usize },

    // =========================================================================
    // Record construction
    // =========================================================================
    /// Construct a record; field registers listed in canonical layout order; all cloned.
    MakeRecord { dst: usize, layout_idx: u32, fields: Vec<usize> },
    /// Extract a field by precomputed positional index; source cloned.
    FieldGet   { dst: usize, src: usize, field_idx: usize },

    // =========================================================================
    // Variant construction and destructuring
    // =========================================================================
    /// Construct a variant. `payload` is None for unit variants.
    MakeVariant    { dst: usize, tag_id: u32, payload: Option<usize> },
    /// Extract the payload of a non-unit variant; source cloned.
    /// RuntimeError if variant is unit or src is not a Variant.
    VariantPayload { dst: usize, src: usize },

    // =========================================================================
    // List construction (all elements cloned)
    // =========================================================================
    MakeList { dst: usize, items: Vec<usize> },

    // =========================================================================
    // Pattern matching (source always cloned; remains valid across all arms)
    // =========================================================================
    /// Jump to target_ip if src.tag_id == tag_id (integer comparison).
    MatchTagEq { tag_id: u32, src: usize, target_ip: usize },
    /// Jump to target_ip if src == constant[const_idx] (structural equality).
    /// Applies to Int, Float, Bool, String, Unit literals.
    MatchLitEq { const_idx: u32, src: usize, target_ip: usize },

    // =========================================================================
    // Control flow (target_ip resolved at lowering; no symbols at runtime)
    // =========================================================================
    Jump { target_ip: usize },

    // =========================================================================
    // Function calls
    // =========================================================================
    /// Non-tail call; arg cloned; result written to dst.
    Call        { dst: usize, fn_idx: usize, arg_reg: usize },
    /// Call a builtin stdlib function; all args cloned; O(1) dispatch via u16.
    CallBuiltin { dst: usize, builtin: u16, args: Vec<usize> },
    /// Tail call; arg MOVED; frame reset; no Rust stack growth.
    TailCall    { fn_idx: usize, arg_reg: usize },
    /// Tail call through a runtime FnRef/PartialFn value.
    TailCallDyn { fn_reg: usize, arg_reg: usize },
    /// Non-tail call through a runtime FnRef/PartialFn value.
    CallDyn     { dst: usize, fn_reg: usize, arg_reg: usize },

    // =========================================================================
    // Return (src MOVED; frame destroyed)
    // =========================================================================
    Return { src: usize },

    // =========================================================================
    // Named field access (runtime lookup — used for anonymous records in Phase 4a)
    // =========================================================================
    /// Extract a field by name (string constant index). Used for anonymous records
    /// where no canonical layout is available at compile time.
    FieldGetNamed { dst: usize, src: usize, name_idx: u32 },

    // =========================================================================
    // Partial application
    // =========================================================================
    /// Create a PartialFn value from a FnRef + bound-args record.
    MakePartial { dst: usize, fn_reg: usize, bound_reg: usize },

    // =========================================================================
    // VM closures (closure-lifting: named capturing helpers compiled to bytecode)
    // =========================================================================
    /// Create a VmClosure from a pre-registered lifted function and a snapshot of
    /// captured variable values. `capture_regs` is (field_name, source_reg) pairs.
    MakeClosure { dst: usize, fn_idx: usize, capture_regs: Vec<(String, usize)> },

    // =========================================================================
    // Clone — always explicit; never implicit
    // =========================================================================
    Clone { dst: usize, src: usize },

    // =========================================================================
    // Refinement checks (emitted before MAKE_RECORD/MAKE_VARIANT with `where` constraints)
    // =========================================================================
    CheckRange  { src: usize, lo: i64, hi: i64 },
    CheckRangeF { src: usize, lo: f64, hi: f64 },
    /// op: 0=Ge, 1=Gt, 2=Le, 3=Lt, 4=Eq, 5=Ne
    CheckCmp    { src: usize, op: u8, n: i64 },
    /// op same encoding as CheckCmp; checks String char-count
    CheckLen    { src: usize, op: u8, n: i64 },

    // =========================================================================
    // Channel operations
    // =========================================================================
    ChanNew   { dst: usize },
    /// Create a closeable channel; CHAN_CLOSE is valid on this channel.
    ChanNewCloseable { dst: usize },
    /// Rchan cloned; Rval MOVED. RuntimeError if channel is closed.
    ChanSend  { chan_reg: usize, val_reg: usize },
    /// Rchan cloned; returns T directly. RuntimeError on empty or closed channel.
    ChanRecv  { dst: usize, chan_reg: usize },
    /// Rchan cloned; returns Maybe<T>. Use only for closeable channels.
    /// Closed+empty → Maybe::none(); closed+non-empty → Maybe::some(v).
    /// Open+empty → RuntimeError (sync) / suspends (async).
    ChanRecvMaybe { dst: usize, chan_reg: usize },
    /// Mark channel closed; Rchan cloned. Subsequent CHAN_SEND → RuntimeError.
    ChanClose { chan_reg: usize },

    // =========================================================================
    // Select — atomic channel poll; lowers from Expr::Select
    // =========================================================================
    Select { dst: usize, arms: Vec<SelectArm>, timeout: Option<TimeoutArm> },
}

// =============================================================================
// KelnFn — compiled function
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct KelnFn {
    pub name: String,
    pub register_count: usize,
    pub instructions: Vec<Instruction>,
    /// Debug: mapping from register index to source variable name.
    pub debug_names: Vec<Option<String>>,
}

impl KelnFn {
    pub fn new(name: impl Into<String>) -> Self {
        KelnFn {
            name: name.into(),
            register_count: 0,
            instructions: Vec::new(),
            debug_names: Vec::new(),
        }
    }
}

// =============================================================================
// KelnModule — compiled program
// =============================================================================

pub struct KelnModule {
    pub fns: Vec<KelnFn>,
    pub fn_index: HashMap<String, usize>,
    pub constants: ConstantTable,
    pub tags: TagTable,
    pub layouts: RecordLayoutTable,
}

impl Default for KelnModule {
    fn default() -> Self {
        Self::new()
    }
}

impl KelnModule {
    pub fn new() -> Self {
        KelnModule {
            fns: Vec::new(),
            fn_index: HashMap::new(),
            constants: ConstantTable::new(),
            tags: TagTable::new(),
            layouts: RecordLayoutTable::new(),
        }
    }

    pub fn fn_idx(&self, name: &str) -> Option<usize> {
        self.fn_index.get(name).copied()
    }

    pub fn add_fn(&mut self, f: KelnFn) -> usize {
        let idx = self.fns.len();
        self.fn_index.insert(f.name.clone(), idx);
        self.fns.push(f);
        idx
    }
}
