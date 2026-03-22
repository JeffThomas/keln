# Keln Effect System — Formal Semantics
## Version 0.9

This document specifies the formal semantics of Keln's effect system in
compiler-implementable terms. It supplements section 4 of the main spec with
the precise rules a type checker must implement.

---

## 1. Effect Set Representation

An **effect set** `E` is a finite set of effect names. Effect names are
`UpperCamelCase` strings.

```
E ::= ∅  |  { "IO" }  |  { "Log" }  |  { "Metric" }  |  { "Clock" }
        |  { e₁, e₂, ... }   -- any finite combination
```

`Pure` is syntactic sugar for `∅`. It is never stored in the set itself —
`Pure` in source means "no effects", which is the empty set.

**Built-in effect names:**

| Name     | Meaning                                      |
|----------|----------------------------------------------|
| `IO`     | Network, filesystem, or environment I/O      |
| `Log`    | Structured log emission                      |
| `Metric` | Metric or telemetry emission                 |
| `Clock`  | Reading or being influenced by current time  |

Custom effect names may be declared with `effect <Name> { ... }`.

---

## 2. Effect Set Construction (Normalization)

When constructing an effect set from a list of names (e.g., parsing
`IO & Clock`), apply:

```
normalize(names):
    S = { n ∈ names | n ≠ "Pure" }
    if S = ∅:
        return ∅   -- Pure
    else:
        return S   -- drop Pure when real effects present
```

**Rationale:** `IO & Pure = IO`. `Pure` is the identity element. Storing
`"Pure"` alongside other effects would cause spurious inequality checks.

**EBNF in source:** `Pure | IO | Log | Metric | Clock | <custom_effect>`
joined by `&`. Parser extracts names; normalizer produces the final set.

---

## 3. Effect Set Union

```
E₁ ∪ E₂ = set union of the two effect sets (standard set union)
```

Used for:
- `fn f { IO & Clock ... }` → E = normalize(["IO", "Clock"])
- Pipeline: `e₁ |> e₂ |> e₃` → E = E(e₁) ∪ E(e₂) ∪ E(e₃)
- `do` block: E(block) = ∪ E(stmtᵢ)

**Commutativity and idempotence hold:** `E ∪ E = E`, `E₁ ∪ E₂ = E₂ ∪ E₁`.

---

## 4. Effect Subsumption (Subtyping)

`E₁ ⊆ E₂` holds iff every effect in E₁ is also in E₂. This is standard
set subset.

**The Pure ⊆ E rule:** `∅ ⊆ E` for any E, since the empty set is a subset
of every set. A `Pure` function can be called from any context.

**Reading the rule:** "A callee with effects E_callee can be called from a
caller with effects E_caller iff E_callee ⊆ E_caller."

```
effect_compatible(E_callee, E_caller):
    return E_callee ⊆ E_caller
```

**Examples:**

| E_callee       | E_caller       | Compatible? | Reason                        |
|----------------|----------------|-------------|-------------------------------|
| `∅` (Pure)     | `{ IO }`       | ✓           | ∅ ⊆ any set                  |
| `∅` (Pure)     | `∅` (Pure)     | ✓           | ∅ ⊆ ∅                        |
| `{ IO }`       | `{ IO }`       | ✓           | {IO} ⊆ {IO}                  |
| `{ IO }`       | `{ IO, Clock }`| ✓           | {IO} ⊆ {IO, Clock}           |
| `{ IO }`       | `∅` (Pure)     | ✗           | IO ∉ ∅                       |
| `{ IO, Clock }`| `{ IO }`       | ✗           | Clock ∉ {IO}                 |
| `{ Log }`      | `{ IO }`       | ✗           | Log ∉ {IO}                   |

---

## 5. FunctionRef Effect Subtyping

`FunctionRef<E₁, T, U>` is assignable where `FunctionRef<E₂, T, U>` is
expected iff `E₁ ⊆ E₂`. The input and output types must match exactly
(invariant in T and U).

```
fnref_compatible(FunctionRef<E₁, T, U>, FunctionRef<E₂, T₂, U₂>):
    return E₁ ⊆ E₂ AND T = T₂ AND U = U₂
```

**Covariant effect subsumption:** A Pure function reference satisfies any
`FunctionRef<E, T, U>` — the Pure function can be passed to contexts that
expect IO-capable callbacks, since it simply won't use the IO capability.

**Not contravariant:** `FunctionRef<IO, T, U>` does NOT satisfy
`FunctionRef<Pure, T, U>` because the caller's Pure constraint would be
violated.

---

## 6. Type Checker Compatibility Algorithm

When the type checker encounters a call `f(arg)` where `f` has declared
effect set `E_callee` and the call site is inside a function with `E_caller`:

```
check_effect_compatibility(call_site):
    E_callee = declared_effects(f)
    E_caller = current_fn_effects()
    if NOT effect_compatible(E_callee, E_caller):
        emit_error("function '{f}' requires effect {E_callee \ E_caller}
                    not in scope ({E_caller})")
```

**Channel operations** require `IO`:

```
Channel.new<T>()   -- requires IO in E_caller
ch <- value        -- (ChannelSend) requires IO in E_caller
<- ch              -- (ChannelRecv) requires IO in E_caller
select { ... }     -- requires IO in E_caller
```

These are checked the same way as function calls: the operation's implicit
effect set is `{ "IO" }`, and `{ "IO" } ⊆ E_caller` must hold.

**Clock.now()** requires `Clock`:
```
Clock.now()   -- requires Clock in E_caller
```

**Emit vs. capability model:** Keln uses the *capability* model. An effect
annotation `IO & Clock` means "this function MAY perform these operations",
not "this function ALWAYS performs them". The type checker approves the
capability, not the necessity.

---

## 7. Custom Effect Declarations

```keln
effect Database {
    query:       IO TypeRef                             -> Result<List<TypeRef>, DbError>
    execute:     IO String                              -> Result<Unit, DbError>
    transaction: IO FunctionRef<IO, Unit, Result<T, E>> -> Result<T, E>
}
```

A custom effect `E` is a named module of operations. Declaring a parameter
of effect type `E` (e.g., `db: Database`) implicitly adds the effect's
operations to the caller's capability set.

The type checker treats custom effects as named module types for the purpose
of field access and call-site checking. Effect compatibility for custom
effects is by name equality (not structural): `Database ⊆ { Database }`.

---

## 8. Pipeline Effect Propagation

For a pipeline expression `e₀ |> f₁ |> f₂ |> ... |> fₙ`:

```
E(pipeline) = E(e₀) ∪ E(f₁) ∪ E(f₂) ∪ ... ∪ E(fₙ)
```

The entire pipeline expression must be permissible within the enclosing
function's `E_caller`. Each step is checked individually:
`E(fᵢ) ⊆ E_caller` for all i.

---

## 9. do Block Effect Propagation

For a `do` block containing statements `s₁; s₂; ...; sₙ`:

```
E(do block) = ∪ { E(sᵢ) | i = 1..n }
```

The `do` block's effect set is the union of all statement effects. Any
statement with an effect not in `E_caller` is a type error.

---

## 10. Verify Block — Effect Checking

In `verify` blocks, mocked effects are treated as satisfied. A `mock Clock`
declaration makes `Clock` effects permissible within the verify block's
execution context without requiring `Clock` in the function's own effect
declaration.

**Rule:** `verify` blocks execute with an augmented effect set that includes
all mocked effects. Type checking of the function body proceeds normally
against the function's declared `E`; mock resolution happens at runtime.

---

## 11. Implementation Notes (Phase 2 type checker)

The Phase 2 implementation in `src/types/check.rs` uses:

```rust
struct EffectSet { effects: HashSet<String> }

impl EffectSet {
    fn from_names(names: &[String]) -> Self {
        let s: HashSet<_> = names.iter()
            .filter(|n| n.as_str() != "Pure")
            .cloned().collect();
        EffectSet { effects: s }
    }

    fn is_pure(&self) -> bool { self.effects.is_empty() }

    fn contains(&self, e: &str) -> bool { self.effects.contains(e) }

    fn subsumes(&self, other: &EffectSet) -> bool {
        // self ⊆ other (self's effects are a subset of other's)
        self.effects.iter().all(|e| other.effects.contains(e.as_str()))
    }
}
```

The `effect_compatible(callee, caller)` check is `callee.subsumes(caller)`.
The `Pure ⊆ E` rule is automatically satisfied because an empty set is a
subset of any set.
