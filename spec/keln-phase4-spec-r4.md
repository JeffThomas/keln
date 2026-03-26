# Keln Phase 4 — Bytecode VM Design
## Specification Addendum to keln-spec-v0.9 (Revision 4 — Final)

This is the fourth and final revision of the Phase 4 design specification.
It closes the remaining sharp edges identified in three rounds of design review.
This document is implementation-ready: every instruction is fully specified,
all runtime semantics are unambiguous, and the checklist maps directly to code.

**Changes from Revision 3:**
- Labels resolved to instruction indices at lowering time; VM never sees symbols
- `MATCH_LIT_EQ` uses constant table index exclusively; no register-vs-literal ambiguity
- `FIELD_GET` uses precomputed positional index, not runtime symbol lookup
- `MAKE_RECORD` encodes field-to-index mapping at compile time
- Variant tags interned to `u32` at compile time; runtime comparison is integer equality
- `CallFrame` gains `dst: usize` field; `RETURN` knows where to write
- "Values passed in tail position are consumed" stated as explicit language guarantee
- Value Shape Guarantees table added
- Countdown lowering trace corrected (removed redundant `LOAD_INT`)

---

## Phase 4 Scope and Sequencing

### 4a — Bytecode IR and Lowering Pass
Lower the typed AST to a flat, register-based bytecode IR.

### 4b — Bytecode Interpreter with TCO
Execute `KelnModule` bytecodes. All Phase 2/3 tests pass against both backends.
TCO as frame-reset loop. Initial implementation uses Rust call stack for
non-tail calls; explicit `Vec<CallFrame>` stack follows.

### 4c — Work-Stealing Scheduler
`Value: Send + Sync`. Real Tokio tasks and channels.

### 4d — Binary Output
Standalone executable with embedded VM runtime.

---

## 4a — Bytecode IR

### Register Model

Every value occupies a numbered register. No runtime name lookup. Names exist
only in debug info. The lowering pass assigns registers sequentially
(`next_reg: usize`, incremented for each new value). Every register is written
exactly once (single-assignment invariant, enforced by the lowering pass).

```
R0          -- function input (written at frame entry from call argument)
R1..Rn      -- all other values: let-bindings, intermediates, call results
RRET        -- convention: RETURN Rsrc writes Rsrc to the caller's dst register
```

`Frame` stores registers as `Vec<Option<Value>>`. Moved registers become
`None`. Read-after-move is `RuntimeError`.

```rust
struct Frame {
    regs: Vec<Option<Value>>,
}
impl Frame {
    fn read(&self, r: usize) -> Result<&Value, RuntimeError> {
        self.regs[r].as_ref()
            .ok_or_else(|| RuntimeError::new(format!("read from moved register R{}", r)))
    }
    fn take(&mut self, r: usize) -> Result<Value, RuntimeError> {
        self.regs[r].take()
            .ok_or_else(|| RuntimeError::new(format!("move from moved register R{}", r)))
    }
    fn clone_reg(&self, r: usize) -> Result<Value, RuntimeError> {
        self.read(r).map(|v| v.clone())
    }
    fn write(&mut self, r: usize, v: Value) {
        debug_assert!(self.regs[r].is_none(), "double-write to register R{}", r);
        self.regs[r] = Some(v);
    }
}
```

### Register Ownership: Move vs Clone

This table is exhaustive and normative. Every instruction either clones or
moves each source operand.

| Instruction | Source behavior |
|---|---|
| `LOAD_REG Rdst, Rsrc` | clone — Rsrc remains valid |
| `CALL Rdst, fn, Rarg` | clone — Rarg remains valid after call |
| `CALL_BUILTIN Rdst, builtin, [Rarg0, ...]` | clone — all arg registers remain valid |
| `TAIL_CALL fn, Rarg` | **move** — Rarg consumed; frame reset |
| `RETURN Rsrc` | **move** — Rsrc consumed; frame destroyed |
| `CHAN_SEND Rchan, Rval` | Rchan clone; Rval **move** |
| `CLONE Rdst, Rsrc` | clone — Rsrc valid; Rdst gets independent copy |
| All arithmetic/comparison | clone all sources |
| `FIELD_GET Rdst, Rsrc, field_idx` | clone — Rsrc remains valid |
| `VARIANT_PAYLOAD Rdst, Rsrc` | clone — Rsrc remains valid |
| `MATCH_TAG_EQ tag_id, Rsrc, ip` | clone — Rsrc valid across all arms |
| `MATCH_LIT_EQ const_idx, Rsrc, ip` | clone — Rsrc valid across all arms |
| `MAKE_RECORD Rdst, layout_idx, [Rfield0, ...]` | clone all field registers |
| `MAKE_VARIANT Rdst, tag_id, Rpayload` | clone Rpayload |

**Language-level guarantee — tail position consumes its argument:**

A value passed to a function in tail position is moved (consumed). This is
not merely an implementation detail. It is a language-level semantic guarantee:
in tail position, the caller's binding is invalidated. This is intentional —
tail calls are frame replacements, not calls, and treating them as such enables
the ownership model to remain consistent. Compiler passes must not "optimize"
tail calls into non-tail calls without accounting for this ownership transfer.

### Labels and Control Flow

Labels are **not symbolic at runtime**. All labels are resolved to instruction
indices (`usize`) during the lowering pass. The VM never sees symbolic labels.

```rust
Instruction::Jump    { target_ip: usize }
Instruction::Label   // not an instruction — labels are erased after lowering
                     // they exist only in the lowering pass as named ip markers
```

The lowering pass uses a two-pass approach for forward references:
1. First pass: emit instructions with placeholder `target_ip = 0` for forward
   jumps; record `(instruction_index, label_name)` for each placeholder
2. Record `label_name → instruction_index` as each `LABEL` marker is encountered
3. Second pass (fixup): replace placeholders with resolved instruction indices

After lowering, no symbolic label information is retained in `KelnFn`.

### Tags, Fields, and Layouts

**Variant tags** are interned to `u32` at compile time. Every distinct variant
name (`Ok`, `Err`, `Some`, `None`, `Running`, etc.) receives a unique `u32`
tag ID assigned by the constant table during lowering. Runtime tag comparison
is integer equality (`u32 == u32`). No string comparison at runtime.

**Field indices** are positional indices into a type's canonical record layout,
computed at compile time. The canonical layout is the order in which fields
appear in the type declaration. `FIELD_GET` and `MAKE_RECORD` use these
precomputed indices — they never look up field names at runtime.

```rust
// Canonical layout for: type Point = { x: Int, y: Int }
// x → index 0, y → index 1

// FIELD_GET R2, R1, 0   -- R2 = R1.x (index 0)
// FIELD_GET R3, R1, 1   -- R3 = R1.y (index 1)
```

**Record layout table:** The `KelnModule` carries a `RecordLayoutTable` mapping
`type_name → Vec<field_name>` (ordered). The lowering pass consults this table
to resolve `field_name → field_index` for all `FIELD_GET` and `MAKE_RECORD`
instructions. At runtime, `FIELD_GET` uses the precomputed index directly —
no table lookup.

```rust
struct RecordLayoutTable {
    layouts: HashMap<String, Vec<String>>,  // type_name → ordered field names
}
impl RecordLayoutTable {
    fn field_index(&self, type_name: &str, field: &str) -> usize {
        self.layouts[type_name].iter().position(|f| f == field).unwrap()
        // unwrap: valid Keln program; type checker guarantees field exists
    }
}
```

### Instruction Set

```
-- =========================================================================
-- Load operations
-- =========================================================================
LOAD_INT     Rdst, <i64>
LOAD_FLOAT   Rdst, <f64>
LOAD_BOOL    Rdst, <bool>
LOAD_STR     Rdst, <const_idx: u32>     -- index into string constant table
LOAD_UNIT    Rdst
LOAD_REG     Rdst, Rsrc                  -- clone

-- =========================================================================
-- Arithmetic and comparison (all sources cloned; type mismatch → RuntimeError)
-- =========================================================================
ADD          Rdst, Rsrc1, Rsrc2
SUB          Rdst, Rsrc1, Rsrc2
MUL          Rdst, Rsrc1, Rsrc2
DIV          Rdst, Rsrc1, Rsrc2     -- Int: RuntimeError on /0; Float: IEEE 754
EQ           Rdst, Rsrc1, Rsrc2     -- structural equality → Bool
NE           Rdst, Rsrc1, Rsrc2
LT           Rdst, Rsrc1, Rsrc2
LE           Rdst, Rsrc1, Rsrc2
GT           Rdst, Rsrc1, Rsrc2
GE           Rdst, Rsrc1, Rsrc2

-- =========================================================================
-- Record construction
-- =========================================================================
MAKE_RECORD  Rdst, <layout_idx: u32>, [Rfield0, Rfield1, ...]
             -- layout_idx: index into RecordLayoutTable (identifies the type)
             -- field registers listed in canonical layout order (index 0, 1, ...)
             -- all field registers cloned
             -- (refinement CHECK_* instructions emitted before this)

-- =========================================================================
-- Variant construction
-- =========================================================================
MAKE_VARIANT Rdst, <tag_id: u32>, Rpayload
             -- tag_id: interned u32 for variant name (e.g. Ok=0, Err=1)
             -- Rpayload cloned; holds Unit, Tuple value, or Record value

-- =========================================================================
-- Destructuring (all sources cloned; Rsrc remains valid)
-- =========================================================================
FIELD_GET    Rdst, Rsrc, <field_idx: usize>
             -- field_idx: precomputed positional index in canonical layout
             -- Rsrc must be Record or Variant{Record payload}
             -- RuntimeError if Rsrc is not a record-like value

VARIANT_PAYLOAD  Rdst, Rsrc
             -- CHECKED: RuntimeError if Rsrc is a unit variant or non-variant
             -- Rsrc cloned; Rdst receives payload value
             -- In correct lowering, always preceded by a successful MATCH_TAG_EQ

-- =========================================================================
-- List operations (all sources cloned)
-- =========================================================================
MAKE_LIST    Rdst, <count: usize>, [Ritem0, ...]
LIST_HEAD    Rdst, Rsrc             -- returns Maybe<T>
LIST_TAIL    Rdst, Rsrc             -- returns List<T>
LIST_IS_EMPTY Rdst, Rsrc           -- returns Bool

-- =========================================================================
-- Channel operations
-- =========================================================================
CHAN_NEW      Rdst
CHAN_SEND     Rchan, Rval           -- Rchan cloned; Rval MOVED
CHAN_RECV     Rdst, Rchan           -- Rchan cloned
             -- sync: RuntimeError if empty; async: suspends

-- =========================================================================
-- Function calls
-- =========================================================================
CALL         Rdst, <fn_idx: usize>, Rarg
             -- Rarg cloned; non-tail; Rust stack grows (explicit stack in 4b+)
CALL_BUILTIN Rdst, <builtin_idx: u16>, [Rarg0, ...]
             -- all args cloned; O(1) dispatch via builtin_idx integer match
TAIL_CALL    <fn_idx: usize>, Rarg
             -- Rarg MOVED; frame reset; no stack growth
             -- only emitted at tail position
             -- Never-returning functions: all exits must be TAIL_CALL

-- =========================================================================
-- Pattern matching (Rsrc cloned; valid across all arms)
-- =========================================================================
MATCH_TAG_EQ <tag_id: u32>, Rsrc, <target_ip: usize>
             -- jump to target_ip if Rsrc variant tag_id == tag_id
             -- comparison: integer equality (u32)
             -- fall through if no match

MATCH_LIT_EQ <const_idx: u32>, Rsrc, <target_ip: usize>
             -- jump to target_ip if Rsrc == constant[const_idx]
             -- equality: same structural semantics as EQ instruction
             -- applies to: Int, Float, Bool, String, Unit literals
             -- does NOT apply to composite types (Record, Variant, List):
             --   use EQ + conditional Jump for those cases
             -- const_idx always references the constant table;
             --   literal values are never embedded directly in this instruction

-- =========================================================================
-- Control flow
-- =========================================================================
JUMP         <target_ip: usize>    -- unconditional; target resolved at lowering
-- LABEL is not a VM instruction; labels erased after lowering (see §Labels)

-- =========================================================================
-- Return
-- =========================================================================
RETURN       Rsrc                  -- Rsrc MOVED; value written to caller's dst

-- =========================================================================
-- Clone
-- =========================================================================
CLONE        Rdst, Rsrc            -- deep copy; always explicit; never implicit

-- =========================================================================
-- Refinement checks (emitted before MAKE_RECORD/MAKE_VARIANT
--                    when field has a `where` constraint)
-- =========================================================================
CHECK_RANGE  Rsrc, <lo: i64>, <hi: i64>
             -- RuntimeError if !(lo <= Rsrc <= hi); applies to Int
CHECK_RANGE_F Rsrc, <lo: f64>, <hi: f64>
             -- RuntimeError if !(lo <= Rsrc <= hi); applies to Float
CHECK_CMP    Rsrc, <op: u8>, <n: i64>
             -- RuntimeError if Rsrc fails comparison; op is ComparisonOp index
CHECK_LEN    Rsrc, <op: u8>, <n: i64>
             -- RuntimeError if String char-count of Rsrc fails comparison
```

### Builtin Dispatch Table

`CALL_BUILTIN` dispatches via `match builtin_idx { 0 => ..., 1 => ... }`.
The lowering pass resolves `QualifiedName` (e.g. `List.map`) → `Builtin`
index at compile time. No runtime string lookup.

```rust
#[repr(u16)]
enum Builtin {
    ResultOk = 0, ResultErr = 1, ResultMap = 2, ResultBind = 3,
    ResultMapErr = 4, ResultSequence = 5, ResultUnwrapOr = 6,
    MaybeSome = 10, MaybeNone = 11, MaybeMap = 12,
    MaybeBind = 13, MaybeGetOr = 14,
    ListMap = 20, ListFilter = 21, ListFoldl = 22, ListFind = 23,
    ListHead = 24, ListTail = 25, ListIsEmpty = 26,
    ListLength = 27, ListRange = 28,
    StringConcat = 40, StringLen = 41, StringTrim = 42,
    StringSplit = 43, StringJoin = 44,
    IntToString = 50, IntPow = 51, IntAbs = 52,
    FloatAdd = 60, FloatMul = 61, FloatDiv = 62, FloatToInt = 63,
    // ... complete table in implementation
}
```

### Value Shape Guarantees

Each instruction has a defined expected input shape. Violation is always
`RuntimeError` — never undefined behavior. This table is normative; both
backends must implement the same error behavior.

| Instruction | Expected input shape | Error on mismatch |
|---|---|---|
| `ADD`, `SUB`, `MUL`, `DIV` | Both operands Int, or both Float | RuntimeError |
| `LT`, `LE`, `GT`, `GE` | Both Int, both Float, or both String | RuntimeError |
| `EQ`, `NE` | Any matching pair (structural eq defined for all types) | — |
| `FIELD_GET` | Rsrc is `Record` or `Variant{Record payload}` | RuntimeError |
| `VARIANT_PAYLOAD` | Rsrc is `Variant` with non-Unit payload | RuntimeError |
| `MATCH_TAG_EQ` | Rsrc is `Variant` | RuntimeError |
| `MATCH_LIT_EQ` | Rsrc is Int, Float, Bool, String, or Unit | RuntimeError |
| `LIST_HEAD`, `LIST_TAIL`, `LIST_IS_EMPTY` | Rsrc is `List` | RuntimeError |
| `CHAN_SEND`, `CHAN_RECV` | Rchan is `Channel` | RuntimeError |
| `CHECK_RANGE`, `CHECK_CMP` | Rsrc is Int or Float (per instruction variant) | RuntimeError |
| `CHECK_LEN` | Rsrc is String | RuntimeError |
| `CALL`, `TAIL_CALL` | fn_idx valid in function table | compile-time guarantee |
| `CALL_BUILTIN` | builtin_idx valid in Builtin enum | compile-time guarantee |

Compile-time guarantees (last two rows) mean the type checker ensures valid
indices before bytecode is produced. Runtime bounds check on function/builtin
indices is a debug-mode-only assertion.

### Explicit Call Stack Model

Phase 4b ships with Rust call stack recursion for `CALL`. The explicit
`Vec<CallFrame>` model is specified here to lock semantics before the follow-up
implementation. The two models must be semantically identical.

```rust
struct CallFrame {
    fn_idx: usize,     // function being executed when this frame was suspended
    ip:     usize,     // instruction to resume at (the ip after the CALL)
    frame:  Frame,     // register state of the suspended caller
    dst:    usize,     // register in caller's frame to write RETURN value into
}
```

```
CALL Rdst, fn, Rarg:
    arg = current.frame.clone_reg(Rarg)
    call_stack.push(CallFrame {
        fn_idx: current.fn_idx,
        ip:     current.ip + 1,   // resume after the CALL instruction
        frame:  current.frame,
        dst:    Rdst,             // where to write the return value
    })
    current.fn_idx = fn
    current.ip = 0
    current.frame = Frame::new(module.fns[fn].register_count)
    current.frame.write(0, arg)

RETURN Rsrc:
    result = current.frame.take(Rsrc)
    caller = call_stack.pop()
    caller.frame.write(caller.dst, result)
    current.fn_idx = caller.fn_idx
    current.ip = caller.ip
    current.frame = caller.frame

TAIL_CALL fn, Rarg:
    -- no push; current frame is replaced
    arg = current.frame.take(Rarg)
    current.fn_idx = fn
    current.ip = 0
    current.frame = Frame::new(module.fns[fn].register_count)
    current.frame.write(0, arg)
```

When the explicit call stack lands, `RuntimeError` gains a stack trace by
walking `call_stack` at point of error.

---

## 4b — Bytecode Interpreter

### Interpreter Loop (Phase 4b — Rust stack for CALL)

```rust
fn execute(module: &KelnModule, fn_idx: usize, arg: Value,
           mock_table: &MockTable) -> Result<Value, RuntimeError> {
    let mut current_fn = fn_idx;
    let mut frame = Frame::new(module.fns[current_fn].register_count);
    frame.write(0, arg);
    let mut ip = 0usize;

    loop {
        match &module.fns[current_fn].instructions[ip] {

            Instruction::TailCall { fn_idx: target, arg_reg } => {
                let new_arg = frame.take(*arg_reg)?;
                current_fn = *target;
                frame = Frame::new(module.fns[current_fn].register_count);
                frame.write(0, new_arg);
                ip = 0;
            }

            Instruction::Return { src } => {
                return frame.take(*src);
            }

            Instruction::Call { dst, fn_idx: target, arg_reg } => {
                let arg = frame.clone_reg(*arg_reg)?;
                let fn_name = &module.fns[*target].name;
                let result = if let Some(r) = mock_table.dispatch(fn_name, &arg) {
                    r?
                } else {
                    execute(module, *target, arg, mock_table)?
                };
                frame.write(*dst, result);
                ip += 1;
            }

            Instruction::CallBuiltin { dst, builtin, args } => {
                let vals: Vec<Value> = args.iter()
                    .map(|r| frame.clone_reg(*r))
                    .collect::<Result<_, _>>()?;
                frame.write(*dst, dispatch_builtin(*builtin, vals)?);
                ip += 1;
            }

            Instruction::VariantPayload { dst, src } => {
                let payload = match frame.clone_reg(*src)? {
                    Value::Variant { payload: VariantPayload::Tuple(v), .. } => *v,
                    Value::Variant { payload: VariantPayload::Record(f), .. } => Value::Record(f),
                    Value::Variant { payload: VariantPayload::Unit, name } =>
                        return Err(RuntimeError::new(
                            format!("VARIANT_PAYLOAD: '{}' has no payload", name))),
                    other =>
                        return Err(RuntimeError::new(
                            format!("VARIANT_PAYLOAD: expected variant, got {}", other))),
                };
                frame.write(*dst, payload);
                ip += 1;
            }

            other => {
                dispatch_instruction(other, &mut frame, module)?;
                ip += 1;
            }
        }
    }
}
```

### Dual-Backend Validation

```rust
fn assert_both_backends(source: &str, fn_name: &str, arg: Value, expected: Value) {
    let tw = eval::eval_fn(source, fn_name, arg.clone()).expect("tree-walker");
    let bc = vm::eval_fn(source, fn_name, arg).expect("bytecode VM");
    assert_eq!(tw, expected, "tree-walker");
    assert_eq!(bc, expected, "bytecode VM");
    assert_eq!(tw, bc, "backends agree");
}
```

TCO validation: `countdown(1_000_000)` completes without stack overflow on
the bytecode VM. Rust call stack depth remains 1 throughout.

---

## Worked Execution Traces

### Trace 1 — TCO: `countdown`

**Source:**
```keln
fn countdown {
    Pure Int -> Int
    in: n
    out: match n {
        0 -> 0
        _ -> countdown(n - 1)
    }
}
```

**Lowered bytecode:**
```
fn countdown  (register_count: 3)
  -- R0 = n (input)

  MATCH_LIT_EQ <const: 0>, R0, .case_zero
  -- fall through: default arm

  LOAD_INT     R1, 1
  SUB          R2, R0, R1           -- R2 = n - 1 (R0 and R1 cloned)
  TAIL_CALL    <countdown>, R2      -- R2 MOVED; frame reset

  -- (unreachable after TAIL_CALL)

.case_zero:  (resolved to ip=5 at lowering time)
  LOAD_INT     R1, 0
  RETURN       R1                   -- R1 MOVED; return 0
```

Note: `MATCH_LIT_EQ <const: 0>` references the integer literal 0 in the
constant table. There is no `LOAD_INT R1, 0` before the match — the match
instruction takes its comparison value from the constant table directly.

**Execution trace for `countdown(3)`:**

```
[ip=0] MATCH_LIT_EQ 0, R0=3, ip=5   -- 3 ≠ 0; fall through
[ip=1] LOAD_INT R1, 1                 -- R1 = 1
[ip=2] SUB R2, R0, R1                 -- R2 = 2 (clone R0=3, R1=1)
[ip=3] TAIL_CALL countdown, R2        -- take R2=2; reset: R0=2, ip=0

[ip=0] MATCH_LIT_EQ 0, R0=2, ip=5   -- 2 ≠ 0; fall through
[ip=1] LOAD_INT R1, 1                 -- R1 = 1
[ip=2] SUB R2, R0, R1                 -- R2 = 1
[ip=3] TAIL_CALL countdown, R2        -- take R2=1; reset: R0=1, ip=0

[ip=0] MATCH_LIT_EQ 0, R0=1, ip=5   -- 1 ≠ 0; fall through
[ip=1] LOAD_INT R1, 1                 -- R1 = 1
[ip=2] SUB R2, R0, R1                 -- R2 = 0
[ip=3] TAIL_CALL countdown, R2        -- take R2=0; reset: R0=0, ip=0

[ip=0] MATCH_LIT_EQ 0, R0=0, ip=5   -- 0 == 0; jump to ip=5
[ip=5] LOAD_INT R1, 0                 -- R1 = 0
[ip=6] RETURN R1                      -- take R1=0; return Int(0)

Result: Int(0). Rust call stack depth: 1 throughout. ✓
```

### Trace 2 — Non-tail calls: `quadruple`

**Source:**
```keln
fn double { Pure Int -> Int  in: n  out: n + n }
fn quadruple { Pure Int -> Int  in: n  out: double(double(n)) }
```

**Lowered bytecode:**
```
fn double  (register_count: 2)
  ADD R1, R0, R0     -- R1 = n + n (R0 cloned twice)
  RETURN R1          -- R1 MOVED

fn quadruple  (register_count: 3)
  CALL R1, <double>, R0    -- clone R0; call double → R1
  CALL R2, <double>, R1    -- clone R1; call double → R2
  RETURN R2                -- R2 MOVED (second CALL is NOT in tail position
                           -- because the result feeds another call;
                           -- the outer RETURN is in tail position,
                           -- but double(double(n)) is not a self-call,
                           -- so TAIL_CALL is not emitted here)
```

**Execution trace for `quadruple(3)`:**

```
execute(quadruple, R0=3)
  [ip=0] CALL R1, double, R0         -- clone R0=3
    execute(double, R0=3)
      [ip=0] ADD R1, R0, R0          -- R1=6 (R0 cloned twice)
      [ip=1] RETURN R1               -- take R1=6; return 6
    ← R1 = 6; Rust stack unwinds one frame
  [ip=1] CALL R2, double, R1         -- clone R1=6
    execute(double, R0=6)
      [ip=0] ADD R1, R0, R0          -- R1=12
      [ip=1] RETURN R1               -- take R1=12; return 12
    ← R2 = 12; Rust stack unwinds one frame
  [ip=2] RETURN R2                   -- take R2=12; return 12

Result: Int(12). Peak Rust call stack depth: 2. ✓
```

R0 in `quadruple` is cloned by the first `CALL` and remains valid (though
not used again). R1 is written once by the first `CALL`, cloned by the second
`CALL`, and remains valid. Single-assignment invariant holds.

### Trace 3 — VARIANT_PAYLOAD safety: checked extraction

**Source:**
```keln
fn safeExtract {
    Pure Result<Int, String> -> Int
    in: r
    out: match r {
        Ok(n)  -> n
        Err(_) -> 0
    }
}
```

**Lowered bytecode:**
```
fn safeExtract  (register_count: 3)
  -- R0 = r (Result<Int, String>)

  MATCH_TAG_EQ <Ok: tag_id=0>, R0, .case_ok   -- jump to ip=5 if Ok
  -- fall through: Err arm
  LOAD_INT     R1, 0
  RETURN       R1

.case_ok:  (ip=3)
  VARIANT_PAYLOAD R2, R0   -- CHECKED: extract Ok payload (Int) into R2
                            -- RuntimeError if R0 has no payload
  RETURN R2                 -- R2 MOVED
```

**Safety scenario — payload on unit variant:**

If a lowering bug emitted `VARIANT_PAYLOAD` on a `None` value:
```
VARIANT_PAYLOAD R2, R0   -- R0 = Variant{name:"None", payload:Unit}
→ RuntimeError: "VARIANT_PAYLOAD: 'None' has no payload"
```

VM produces a deterministic error. No undefined behavior, no wrong value
silently returned.

---

## 4c — Work-Stealing Scheduler

### `Send + Sync` Migration (deferred to 4c)

| Component | Sync (Phase 2/3, default) | Async (Phase 4c, `--async`) |
|---|---|---|
| `Value::Channel` | `Rc<RefCell<VecDeque<Value>>>` | `Arc<Mutex<VecDeque<Value>>>` |
| `Task.spawn` | executes immediately, wraps result | `tokio::spawn` → `JoinHandle` |
| `Task.awaitAll` | unwraps pre-computed values | `futures::join_all` |
| `select` | polls VecDeque; Unit on empty | `tokio::select!`; suspends |
| `CHAN_RECV` sync | RuntimeError on empty | suspends until value available |

Feature flag: Cargo feature `async`.

Verify execution always uses the sync model — mocks, `forall` sampling, and
`given` cases do not need Tokio overhead and run faster without it.

### Scheduler

Tokio `new_multi_thread()` runtime. No custom scheduler. Thread count defaults
to logical CPU count; `--threads N` overrides. Data races are structurally
impossible — the type system's ownership model enforces channel-only
inter-task communication.

---

## 4d — Binary Output

### Format

```
[magic]        4 bytes: 0x4B 0x45 0x4C 0x4E  ("KELN")
[version]      u16 (spec version; currently 9)
[flags]        u16: async=0x01, debug_info=0x02, has_entry=0x04
[const_table]  u32 length + bincode-encoded entries
[layout_table] u32 count + RecordLayoutTable entries
[fn_table]     u32 count + KelnFn entries (bincode)
[entry_point]  u32 fn_table index, or 0xFFFF if library
```

Single self-contained ELF. Stripped size ~5–15 MB. No Keln installation
required on target host.

---

## Phase 4 Checklist (revision 4)

### Phase 4a — Bytecode IR
- [ ] Define `Instruction` enum with all opcodes; all indices are `usize`/`u32`/`u16` (no strings)
- [ ] Define `Frame` with `Vec<Option<Value>>` and `read`/`take`/`write`/`clone_reg`
- [ ] Define `KelnFn`, `KelnModule`, `ConstantTable`, `RecordLayoutTable`
- [ ] Define `Builtin` enum (complete); map `QualifiedName` → `Builtin` at lowering
- [ ] Define `CallFrame { fn_idx, ip, frame, dst }` for explicit call stack
- [ ] Implement lowering pass: typed AST → `KelnModule`
  - [ ] Literals: `LOAD_*`; sequential `next_reg++`; debug names recorded
  - [ ] `let` bindings: new register per binding
  - [ ] Arithmetic, comparison: clone both sources
  - [ ] Record construction: `MAKE_RECORD` with `layout_idx`; field registers in canonical order
  - [ ] Variant construction: `MAKE_VARIANT` with `tag_id` (u32, interned); payload cloned
  - [ ] Refinement checks: `CHECK_*` emitted before `MAKE_RECORD`/`MAKE_VARIANT`
  - [ ] `FIELD_GET`: precomputed `field_idx` from `RecordLayoutTable`; never symbol at runtime
  - [ ] `VARIANT_PAYLOAD`: always emitted after `MATCH_TAG_EQ` in correct lowering
  - [ ] Tail call detection: `CALL` vs `TAIL_CALL` at all tail positions
  - [ ] `Never`-return validation: all exits must emit `TAIL_CALL`
  - [ ] Match lowering: `MATCH_TAG_EQ` (tag_id, target_ip); `MATCH_LIT_EQ` (const_idx, target_ip)
  - [ ] `MATCH_LIT_EQ`: literal value from constant table; never embedded in instruction
  - [ ] Labels: two-pass resolution; `LABEL` erased; `JUMP`/`MATCH_*` use `target_ip: usize`
  - [ ] Pipeline `|>`: sequential `CALL`; last step `TAIL_CALL` if in tail position
  - [ ] `select` and channel ops; `CHAN_SEND` moves Rval
  - [ ] `clone` → `CLONE`
  - [ ] Compact helpers: lower as regular functions with `it` → R0
- [ ] Tests: lower `countdown`, `quadruple`, `safeExtract`; assert instruction output
      matches traces in §Worked Execution Traces

### Phase 4b — Bytecode Interpreter
- [ ] Implement interpreter loop with `TAIL_CALL` as frame-reset (no Rust stack growth)
- [ ] Implement `CALL` with Rust stack recursion (explicit `Vec<CallFrame>` in follow-up)
- [ ] Implement `VARIANT_PAYLOAD` with RuntimeError on unit/non-variant
- [ ] Implement `MATCH_LIT_EQ` using same structural equality as `EQ`
- [ ] Implement `FIELD_GET` using precomputed `field_idx` (no runtime name lookup)
- [ ] Implement `MATCH_TAG_EQ` using integer tag_id comparison (no string comparison)
- [ ] Wire `mock_table` into `CALL` and `CALL_BUILTIN` dispatch
- [ ] Dual-backend harness: all Phase 2/3 tests against tree-walker and VM
- [ ] TCO test: `countdown(1_000_000)` without stack overflow
- [ ] Value shape errors: verify RuntimeError (not panic) for all mismatch cases
- [ ] Follow-up: implement explicit `Vec<CallFrame>` call stack per §Explicit Call Stack Model;
      add stack trace to `RuntimeError`

### Phase 4c — Work-Stealing Scheduler
- [ ] Cargo feature `async`; gate `Arc<Mutex<...>>` channel path
- [ ] Migrate `Value::Channel` to `Arc<Mutex<VecDeque<Value>>>`
- [ ] Real `tokio::spawn` for `Task.spawn`; `join_all` for `Task.awaitAll`
- [ ] `tokio::select!` for `select`
- [ ] Integration test: concurrent worker pool fan-out (from validation exercises)

### Phase 4d — Binary Output
- [ ] `KelnModule` serializer/deserializer (bincode); include `RecordLayoutTable`
- [ ] `kelnc compile` CLI (`--output`, `--async`, `--threads`, `--release`)
- [ ] Embed module + VM runtime in ELF
- [ ] Tests: compile + run `countdown`; compile + run worker integration test

---

## Design Decisions Log

**Full register model:** Parallel named-binding system eliminated. Names in
debug info only.

**`Vec<Option<Value>>`:** Enforces move semantics at runtime. Moved registers
become `None`; read-after-move is `RuntimeError`.

**Tail-call move as language guarantee:** Values in tail position are consumed.
This is a language-level semantic, not an implementation detail. Compiler
passes must not silently convert tail calls to non-tail calls.

**Labels resolved at lowering:** VM never sees symbolic labels. Two-pass
lowering resolves all forward references to `target_ip: usize` before
`KelnFn` is finalized.

**MATCH_LIT_EQ uses constant table:** Literal values in match instructions
come from the constant table (`const_idx`), not registers. No mixed model.
No ambiguity.

**FIELD_GET uses precomputed index:** Field names resolved to positional
indices at lowering time via `RecordLayoutTable`. Runtime field access is
`regs[field_idx]`, not a hash lookup.

**Tags interned to u32:** Variant tag comparison is integer equality (`u32`).
No string comparison at runtime.

**VARIANT_PAYLOAD always checked:** RuntimeError on unit variant or
non-variant. Unchecked path deferred to optimization phase.

**MATCH_LIT_EQ equality = EQ equality:** Explicit tie. No silent divergence.

**Constant table and record layout are independent:** Field layout is
canonical per type, fixed at lowering. Constant table is not queried for
positional semantics at runtime.

**CallFrame carries dst:** Explicit call stack requires storing the
destination register in each frame so RETURN knows where to write.

**CALL_BUILTIN indexed:** O(1) dispatch via u16 enum. QualifiedName resolved
at lowering; no runtime string lookup.

**CLONE always explicit:** Implicit cloning rejected. Predictability
and debuggability outweigh any ergonomic gain.

**Sync verify, async deploy:** Sync model default for verify execution.
`--async` feature for production deployment path.

**Send+Sync migration deferred to 4c:** Most disruptive change. Deferred
to keep a stable dual-backend baseline through 4a and 4b.

---

*Phase 4 specification addendum — Keln v0.9, revision 4 (final)*
*Three rounds of design review incorporated. Implementation-ready.*
*To be merged into keln-spec-v1.0 alongside Phase 4 implementation.*
