# Keln Language Spec — Gap Closure Addendum
## Five Structural Gaps Identified in Type System Review

This addendum closes five gaps in keln-spec-v0.9 and keln-phase4-addendum
identified by independent language review. All five are blocking or near-blocking
for type checker implementation.

---

## Gap 1 — TypeRef Is Undefined

### The Gap

`JSON.parse { Pure Bytes, TypeRef -> Result<TypeRef, ParseError> }` and
`JSON.serialize { Pure T -> Bytes }` use `TypeRef` as a runtime type descriptor
that tells the JSON parser what shape to produce. `TypeRef` is never defined in
the spec, grammar, or type system.

### Resolution

`TypeRef<T>` is a compiler-generated phantom type — a compile-time value that
carries the structural description of type `T` without representing a runtime
object of type `T`. Every Keln type `T` has an associated `.ref` class property
of type `TypeRef<T>` emitted by the compiler.

```keln
-- TypeRef<T> is a phantom type. It has no runtime value shape — it is a
-- compile-time token that the JSON decoder uses to know what type to produce.
-- It is never constructed by AI authors directly.
-- It is always accessed as T.ref for a concrete type T.

type TypeRef<T>   -- phantom; no fields; exists only at the type level

-- Access syntax:
--   MyRecord.ref       : TypeRef<MyRecord>
--   Int.ref            : TypeRef<Int>
--   List<String>.ref   : TypeRef<List<String>>
--   Maybe<Port>.ref    : TypeRef<Maybe<Port>>
```

**Updated stdlib signatures:**

```keln
JSON.parse   { Pure Bytes, TypeRef<T> -> Result<T, ParseError>  | for all T }
JSON.serialize { Pure T -> Bytes                                 | for all T }
```

`JSON.parse` is now fully generic and type-safe. The compiler resolves `T` from
the `TypeRef<T>` argument. There is no runtime reflection — `TypeRef<T>` is
erased after type checking and the bytecode VM uses the structural layout
information the compiler emits alongside it.

**Usage:**

```keln
-- Parse a byte sequence as a JobMessage:
let result = JSON.parse(body, JobMessage.ref)
-- result: Result<JobMessage, ParseError>

-- Parse a list of users:
let result = JSON.parse(body, List<User>.ref)
-- result: Result<List<User>, ParseError>
```

**Compiler requirement:** The compiler must reject `TypeRef<T>` where `T` is
not a concrete, fully-instantiated type:

```keln
-- Invalid: T is unbound
let r = JSON.parse(body, T.ref)         -- compile error: T is not a concrete type

-- Valid: T is bound by the enclosing function's type parameters
fn parseAs[T] {
    Pure Bytes -> Result<T, ParseError>
    in:  bytes
    out: JSON.parse(bytes, T.ref)       -- valid: T is bound in this context
}
```

**Grammar addition (to keln-grammar-v0.9):**

```ebnf
type_ref_expr ::= type_expr ".ref"
```

Add `type_ref_expr` to the expression grammar as a postfix operation on type
expressions. It is valid only in value position where `TypeRef<T>` is expected.

---

## Gap 2 — Generic Function Declaration Syntax

### The Gap

The grammar's `<fn_signature>` is `<effect_set> <type_expr> "->" <type_expr>`.
There is no syntax for declaring type parameters on functions. Yet the stdlib
uses unbound type variables `T`, `U`, `E` throughout:

```keln
List.map { List<T>, FunctionRef<E, T, U> -> List<U> | effect E }
```

The `| effect E` notation also does not appear in the grammar. AI authors cannot
write their own generic higher-order functions without encountering type errors
because `T` looks like an undeclared type name.

### Resolution

**Type parameter declaration on functions:**

```keln
fn <name> [<TypeParam>, ...] {
    <effects> <input_type> -> <output_type>
    ...
}
```

Type parameters are declared in square brackets immediately after the function
name, before the opening brace. They follow the same syntax as type-level type
parameters.

```keln
-- Example: generic filter function
fn keepIf [T, E] {
    E { items: List<T>, predicate: FunctionRef<E, T, Bool> } -> List<T>
    in:  { items, predicate }
    out: List.filter(items, predicate)
    confidence: auto
}
```

**Effect variable declaration:**

Effect variables (e.g., `E` standing for "some effect set") are declared in the
same type parameter list as type variables. By convention, effect variables are
single uppercase letters at the end of the parameter list. The type checker
distinguishes effect variables from type variables by their usage position
(appearing in effect position in `FunctionRef<E, In, Out>` or as a declared
effect set).

**The `| effect E` notation in stdlib signatures:**

```keln
List.map { List<T>, FunctionRef<E, T, U> -> List<U> | effect E }
```

The `| effect E` annotation is a *constraint declaration*, not a new syntax
form. It states that `List.map`'s effect set is exactly `E` — the effect of the
`FunctionRef` argument. This is the mechanism by which effect subtyping flows
through higher-order functions: if you pass a `Pure` function ref, `List.map`
is `Pure`; if you pass an `IO` function ref, `List.map` is `IO`.

**Grammar addition:**

```ebnf
fn_decl      ::= "fn" identifier type_params? "{" fn_body "}"
type_params  ::= "[" type_param ("," type_param)* "]"
type_param   ::= identifier                  (* type variable, e.g. T *)
               | identifier ":" effect_kind  (* effect variable, e.g. E : Effect *)
effect_kind  ::= "Effect"                    (* marks this param as an effect variable *)

fn_signature ::= effect_set type_expr "->" type_expr effect_constraint?
effect_constraint ::= "|" "effect" effect_expr
```

**Scope rule:** Type parameters declared on a function are in scope throughout
the function's body, signature, `verify` block, `proves` block, and `helpers`
block. They are not in scope outside the function declaration.

**Helper functions inherit parent type parameters:**

```keln
fn processAll [T, E] {
    E List<T> -> List<T>
    in:  items
    out: ...
    helpers: {
        -- T and E are in scope here
        fn validate {
            E T -> Maybe<T>   -- T and E inherited from parent
            ...
        }
    }
}
```

---

## Gap 3 — Partial Application Type Rule

### The Gap

When `.with()` binds named parameters into a `FunctionRef`, the spec says
"effects are preserved" but does not specify:
1. How the type checker validates that bound field names exist in the input type
2. How the type checker validates that bound values have the correct types
3. How the remaining input type is computed

Without this rule, the type checker cannot validate `.with()` calls.

### Resolution

The `.with()` type rule is a structural record subtraction. Given:

```
fn f { E InputRecord -> Out }
InputRecord = { field1: T1, field2: T2, ..., fieldN: TN }
```

A call `f.with(fieldK: vK)` is valid if and only if:

```
1. fieldK ∈ InputRecord  (field name exists in input type)
2. typeof(vK) <: TK      (bound value is a subtype of the field's declared type)
3. result type: FunctionRef<E, InputRecord - {fieldK: TK}, Out>
   where InputRecord - {fieldK: TK} is InputRecord with fieldK removed
```

**Record subtraction definition:**

`{ f1: T1, ..., fi: Ti, ..., fN: TN } - { fi: Ti }` = `{ f1: T1, ..., f(i-1): T(i-1), f(i+1): T(i+1), ..., fN: TN }`

The resulting record type contains all fields except the bound one. The field
ordering in the remaining type is preserved (original order, gap closed).

**Chained .with() calls:**

```keln
let h = f.with(field1: v1).with(field2: v2)
-- result type: FunctionRef<E, InputRecord - {field1, field2}, Out>
-- equivalent to: f.with({ field1: v1, field2: v2 })
```

Each `.with()` call reduces the remaining input type by one field. Chaining
is valid as long as there are remaining fields to bind.

**Record form:**

```keln
f.with({ field1: v1, field2: v2 })
-- type rule: same as chaining, applied simultaneously
-- result: FunctionRef<E, InputRecord - {field1, field2}, Out>
```

**Fully applied FunctionRef:**

When all fields are bound, the remaining input type is `Unit`:

```keln
fn greet { Pure { name: String } -> String }
let g = greet.with(name: "Keln")
-- g: FunctionRef<Pure, Unit, String>
-- g(Unit) == greet({ name: "Keln" })
```

**Error cases:**

```keln
-- Error 1: field does not exist
f.with(nonexistent: v)
-- CompileError: field 'nonexistent' does not exist in input type of f

-- Error 2: type mismatch
fn h { Pure { port: Port } -> String }
h.with(port: "not-a-port")
-- CompileError: cannot bind String to field 'port: Port'

-- Error 3: double-binding
f.with(field1: v1).with(field1: v2)
-- CompileError: field 'field1' is already bound in this FunctionRef

-- Error 4: binding on non-record input
fn scalar { Pure Int -> String }
scalar.with(x: 42)
-- CompileError: .with() requires a record input type; Int is not a record
```

**Type checker implementation note:**

`.with()` desugars to a `Value::PartialFn` construction at the type level. The
type checker maintains a `BoundFields: Map<FieldName, Type>` alongside the
`FunctionRef` type to track which fields have been pre-bound. At call sites, the
type checker verifies that the argument type matches the remaining (unbound)
fields only. The `BoundFields` map is part of the `FunctionRef` type — two
`FunctionRef` values with the same effect, input, and output types but different
bound fields are distinct types.

---

## Gap 4 — Closeable<Channel<T>> Is Undefined

### The Gap

The Phase 4 addendum introduces `Closeable<Channel<T>>` as the type that causes
the lowering pass to emit `CHAN_RECV_MAYBE` instead of `CHAN_RECV`. `Closeable`
is never defined in the spec, grammar, or type system.

### Resolution

`Closeable<T>` is a first-class wrapper type that marks a value as potentially
closeable. It is defined in the concurrency primitives alongside `Channel<T>`.

```keln
type Closeable<T>   -- marks T as a value that may be explicitly closed
                    -- currently only meaningful for Channel<T>
                    -- future: extendable to other resource types

-- The only Closeable type currently defined:
--   Closeable<Channel<T>>

-- Channel construction:
Channel.new<T>()          : Channel<T>            -- never closeable; CHAN_RECV safe
Channel.newCloseable<T>() : Closeable<Channel<T>> -- may be closed; CHAN_RECV_MAYBE required
```

**Type-system enforcement:**

The type checker enforces that `CHAN_CLOSE` is called only on `Closeable<Channel<T>>`,
and that `CHAN_RECV_MAYBE` is used (not `CHAN_RECV`) when the channel is
`Closeable<Channel<T>>`:

```keln
-- Valid: closing a closeable channel
let ch = Channel.newCloseable<Int>()
CHAN_CLOSE ch     -- valid: ch is Closeable<Channel<Int>>

-- Compile error: closing a non-closeable channel
let ch = Channel.new<Int>()
CHAN_CLOSE ch     -- CompileError: CHAN_CLOSE requires Closeable<Channel<T>>;
                 --               Channel<Int> is not closeable

-- Compile error: using CHAN_RECV on a closeable channel
let ch = Channel.newCloseable<Int>()
let v = <-ch      -- CompileError: receiving from Closeable<Channel<T>> requires
                 --               pattern match on Maybe<T>; use match (<-ch) { ... }
                 --               (which lowers to CHAN_RECV_MAYBE)
```

**Receive syntax for closeable channels:**

Since `CHAN_RECV_MAYBE` returns `Maybe<T>`, the receive expression on a
`Closeable<Channel<T>>` must be in a match context:

```keln
-- Idiomatic receive from closeable channel:
match (<-closeableCh) {
    Some(value) -> handleValue(value)
    None        -> handleClosed(Unit)
}

-- This lowers to:
CHAN_RECV_MAYBE Rdst, Rchan
MATCH_TAG_EQ <Some>, Rdst, .some_arm
-- None arm: ...
.some_arm:
VARIANT_PAYLOAD Rpayload, Rdst
-- handle value...
```

**`select` with closeable channels:**

In a `select` block, arms may receive from either `Channel<T>` or
`Closeable<Channel<T>>`. The arm's binding type reflects the channel type:

```keln
select {
    msg  = <-regularCh    -> handleMsg(msg)     -- msg: T
    item = <-closeableCh  -> handleItem(item)   -- item: Maybe<T>
}
```

**Lowering rule update (replaces Phase 4 addendum):**

The lowering pass emits:
- `CHAN_RECV` when the channel expression has type `Channel<T>`
- `CHAN_RECV_MAYBE` when the channel expression has type `Closeable<Channel<T>>`

This is a type-driven decision, not a scope analysis decision. The type of the
channel value determines the instruction. No "known closeable" scope tracking
required.

**Grammar addition:**

```ebnf
closeable_type ::= "Closeable" "<" type_expr ">"
channel_new    ::= "Channel.new" "<" type_expr ">" "()"
                 | "Channel.newCloseable" "<" type_expr ">" "()"
```

**What does NOT exist:**

- No implicit conversion between `Channel<T>` and `Closeable<Channel<T>>`
- No way to "upgrade" a `Channel<T>` to closeable after creation
- No way to "downgrade" a `Closeable<Channel<T>>` to a plain `Channel<T>`

The closeability of a channel is a property determined at creation. This is
consistent with Keln's design principle that the type system encodes behavioral
contracts structurally.

---

## Gap 5 — forall Over Non-Samplable Types

### The Gap

`forall(ch: Channel<Int>) ->` is syntactically valid but semantically nonsensical
— channels cannot be sampled. The spec does not say this is a compile error.

### Resolution

**Add to keln-spec-v0.9 §10.5 (forall — Logical Operators) and §10.6
(forall Execution Model):**

**Samplable types:**

A type is *samplable* if the `forall` sampling infrastructure can generate
values of that type for bounded verification. A type is non-samplable if it
represents a live resource, a concurrent primitive, or an inherently stateful
object.

```
Samplable types (forall variables may have these types):
    Int, Float, Bool, String, Bytes, Unit, Never (vacuously)
    Product types where all fields are samplable
    Sum types where all variants are samplable
    List<T> where T is samplable
    Map<K,V> where K and V are samplable
    Set<T> where T is samplable
    Refinement types (sampled within their constraint bounds)
    FunctionRef<E, In, Out> where In and Out are samplable
        (sampled as mock functions; see §10.4)

Non-samplable types (forall variables may NOT have these types):
    Channel<T>             -- live concurrent resource; no sampling semantics
    Closeable<Channel<T>>  -- same
    Task<T>                -- live async handle; no sampling semantics
    Module instances       -- stateful; no sampling semantics

Compile error:
    forall(ch: Channel<Int>) ->   -- CompileError: Channel<Int> is not samplable
    forall(t: Task<String>) ->    -- CompileError: Task<String> is not samplable
```

**One-sentence addition to §10.6:**

> `forall` variables must have samplable types; using `Channel<T>`, `Task<T>`,
> or module instance types in a `forall` binding is a compile error.

---

## Summary of Changes

| Gap | Affects | Resolution |
|---|---|---|
| 1 — TypeRef undefined | keln-spec-v0.9 §12.8, grammar | `TypeRef<T>` phantom type + `.ref` syntax |
| 2 — Generic fn syntax | keln-spec-v0.9 §5.1, grammar | `fn name [T, E]` type params + `\| effect E` |
| 3 — .with() type rule | keln-spec-v0.9 §4.4 | Record subtraction rule + error cases |
| 4 — Closeable undefined | keln-phase4-addendum, grammar | `Closeable<T>` wrapper + `Channel.newCloseable` |
| 5 — forall non-samplable | keln-spec-v0.9 §10.5, §10.6 | Samplable type list + compile error |

All five changes affect the grammar (keln-grammar-v0.9.ebnf). The grammar changes
are specified inline above. No changes affect the Phase 4 bytecode instruction set
— `CHAN_RECV` and `CHAN_RECV_MAYBE` are unchanged; the lowering rule is clarified
to be type-driven rather than scope-driven.

---

*Keln Language Spec — Gap Closure Addendum*
*Closes five blocking/near-blocking gaps in keln-spec-v0.9 and keln-phase4-addendum.*
*TypeRef, generic function syntax, .with() type rule, Closeable<Channel<T>>, forall samplability.*
*To be merged into keln-spec-v1.0 alongside Phase 2 tree-walker implementation.*

---

## Addendum 2: Negative Literals and Named Capturing Helpers

### Gap 6: Negative Integer Literals

**Problem:** The spec only specifies non-negative integer literals (`[0-9]+`). There was no way to write a negative integer literal directly; users had to write `0 - 1` instead of `-1`.

**Resolution:** Unary minus applied to an integer or float literal is folded at parse time into a negative literal.

- In expressions: `-N` where `N` is an integer or float token produces `IntLiteral(-N)` or `FloatLiteral(-N)`.
- In patterns: `-N` where `N` is an integer token produces `Pattern::Literal(IntLiteral(-N))`, enabling `match x { -1 -> ... }`.
- Unary minus applied to any other expression produces `0 - expr` (unchanged behavior).

**Grammar change:**

```ebnf
integer_literal ::= "-"? [0-9]+
float_literal   ::= "-"? [0-9]+ "." [0-9]+
pattern_literal ::= "true" | "false" | integer_literal | float_literal | string_literal
```

**Note:** `threshold` is a reserved keyword (from `promote: threshold` helper syntax) and cannot be used as an identifier. Avoid it in field names and variables.

---

### Gap 7: Named Capturing Helpers

**Problem:** Keln's "no lambdas" tenet requires all callable values to be named, but the only way to pass context to a fold/map callback was to thread it through an accumulator record. This led to "accumulator bloat" — adding unrelated fields to accumulator types just to share read-only context.

**Resolution:** Named capturing helpers — a `let` binding whose RHS is an inline function definition that lexically closes over the surrounding `let` bindings. The function has a name (satisfying the "everything named" tenet), an explicit type signature (for traceability), and captures the current lexical environment at the point of definition.

**Syntax:**

```
let <name> :: <effects> <In> -> <Out> => <body_expr> in <rest_expr>
```

- `name`: lower_snake_case identifier bound in `rest_expr`
- `effects`: effect set (e.g. `Pure`, `IO`)
- `In`: input type (the argument is bound as `it` inside `body_expr`)
- `Out`: output type
- `body_expr`: the function body; has access to all `let` bindings in scope at the definition site plus `it` as the argument
- `rest_expr`: expression in which `name` is bound to the closure

**Example:**

```keln
fn sum_with_offset {
    Pure { items: List<Int>, offset: Int } -> Int
    in: args
    out:
        let offset = args.offset in
        let addOffset :: Pure { acc: Int, item: Int } -> Int =>
            it.acc + it.item + offset
        in
        List.fold(args.items, 0, addOffset)
}
```

**Semantics:**
- At the `let name ::` binding, the current lexical environment is snapshotted.
- A `Value::Closure { id }` is created referencing the captured environment and body.
- When called, the captured environment is restored, `it` is bound to the argument, and the body is evaluated.
- Closures are first-class values passable to `List.fold`, `List.map`, `List.foldUntil`, etc.

**Grammar change:**

```ebnf
let_in_expr ::= "let" pattern (":" type_expr)? "=" expr "in" expr
              | "let" lower_ident "::" effect_set type_expr "->" type_expr "=>" expr "in" expr
```

**VM support:** Named capturing helpers are supported in the tree-walking evaluator (used by `verify`). The bytecode VM does not support them; using them with `compile`/`run-bc` produces a compile-time error. Support may be added in a future phase via closure lifting.

**Reserved keywords:** The following identifiers cannot be used as names, field names, or variables: `fn`, `type`, `module`, `trusted`, `effect`, `let`, `in`, `out`, `match`, `do`, `select`, `timeout`, `clone`, `spawn`, `verify`, `given`, `forall`, `mock`, `call`, `helpers`, `promote`, `threshold`, `confidence`, `reason`, `proves`, `provenance`, `where`, `auto`, `not`, `and`, `or`, `implies`, `true`, `false`, `fuzz`, `inputs`, `crashes_never`.

---

## Addendum 3: Evaluator and Parser Quality-of-Life Fixes

### Gap 8: `Map.empty` / `Set.empty` / `Bytes.empty` in Value Position

**Problem:** When `Map.empty`, `Set.empty`, or `Bytes.empty` appeared in a non-call position (e.g. `let m = Map.empty in ...` or `{ counts: Map.empty }`), the evaluator returned a `Value::FnRef("Map.empty")` instead of the actual empty collection. Downstream operations such as `Map.insert(m, ...)` then failed at runtime with a type error.

**Resolution:** The tree-walking evaluator's `QualifiedName` evaluation now detects these three names and calls stdlib immediately, returning the empty value directly:

```
Map.empty  →  Value::Map(BTreeMap::new())
Set.empty  →  Value::Set(BTreeSet::new())
Bytes.empty →  Value::Bytes(vec![])
```

**Effect:** `Map.empty`, `Set.empty`, and `Bytes.empty` are now usable anywhere a value is expected — in `let` bindings, record field initializers, list literals, function arguments, etc. `Map.fromList([])` remains a valid alternative.

**No grammar change.** This is a pure evaluator behaviour fix.

---

### Gap 9: Type Alias Field Access in the Type Checker

**Problem:** When a named type alias was defined as a product type (`type Frac = { num: Int, den: Int }`) and a function returned or manipulated a value of type `Frac`, the type checker's `infer_field_access` function did not expand the alias to retrieve field types. It fell through to the `_ => Type::TypeVar("_unknown")` arm, silently suppressing field-not-found errors and producing incorrect inferred types for downstream expressions.

**Resolution:** `infer_field_access` in `src/types/check.rs` now handles `TypeDef::Alias { target, .. }` by recursively calling itself on the resolved target type. Alias chains of arbitrary depth are handled by the recursion.

**Effect:** `type Frac = { num: Int, den: Int }` can be used as a shorthand type name throughout the program. Field access, field presence errors, and type inference on aliased product types all work correctly.

**No grammar change.** Type aliases were already syntactically valid; this is a type-checker semantic fix.

---

### Gap 10: Improved Naming Error Messages

**Problem:** When a user wrote an identifier with incorrect casing in a position that requires `lower_snake_case` (function names, let binding names, field names, compact helper names), the parser emitted the generic message `"expected lower_snake_case identifier"` regardless of what the actual token was. This gave no actionable guidance.

**Resolution:** `expect_lower_ident` in `src/parser/mod.rs` now produces targeted messages:

- **Uppercase identifier** (`MyFunc`, `F`, `FooBar`): `"'MyFunc' must be lower_snake_case; did you mean 'my_func'?"`  
  The suggestion is generated by a `camel_to_snake` conversion that correctly handles `CamelCase → camel_case`, `FOO → foo`, and single letters `F → f`.

- **Reserved keyword** (`match`, `out`, `threshold`, etc.): `"'match' is a reserved keyword; use a different name"`

- **Other non-identifier tokens**: falls back to the original `"expected lower_snake_case identifier"` message.

**No grammar change.** This is a parser error-reporting improvement only.

---

### Gap 11: Record Update via `.with()`

**Problem:** `.with()` only worked on `FunctionRef` and `PartialFn` values. When applied to a plain `Value::Record`, the evaluator raised a runtime error: `"cannot apply .with to non-function"`. The only way to produce a record with one updated field was to list every field explicitly:

```keln
-- verbose: must repeat all unchanged fields
let updated = { count: state.count + 1, label: state.label, active: state.active }
```

**Resolution:** The tree-walking evaluator's `With` handler now recognises `Value::Record` as a base value. It returns a new record where specified fields are overridden in-place (preserving order) and any field names not present in the base are appended.

```keln
-- single field:
let updated = state.with(count: state.count + 1)

-- multiple fields:
let moved = pos.with({ x: newX, y: newY })

-- chained:
let s2 = state.with(count: 0).with(label: "reset")
```

The type checker's `With` handling was also extended: `Type::Record` base now produces an updated `Type::Record` with the overridden field types. `Type::Named` bases (type aliases over records) are handled leniently — value types are still inferred for error detection.

**Semantics:**
- The original record is unchanged (immutable copy-on-update).
- If a named field does not exist in the base record, it is appended.
- Evaluation order: base record is evaluated first, then override values left to right.

**No grammar change.** The `<with_expr>` syntax already accepted any expression on the left; this is a pure evaluator and type-checker semantic extension.

---

## Addendum 4: Lexer, Evaluator, and Stdlib Fixes

### Gap 12: Keyword-Prefix Identifier Lexing

**Problem:** The lexer used lexxor's `KeywordMatcher` which treated `_` as a word boundary. This caused any identifier whose prefix up to the first `_` was a reserved keyword to be lexed as two tokens. For example, `do_round` was lexed as keyword `do` + word `_round`, producing a parse error.

Affected identifiers included: `do_*`, `in_*`, `out_*`, `let_*`, `not_*`, `or_*`, `and_*`, `true_*`, `false_*`, `match_*`, `fn_*`, `type_*`, and any other `keyword_suffix` pattern.

**Resolution:** `KeywordMatcher` was removed entirely from the lexer. Keyword detection was moved into `IdentifierMatcher`, which already correctly matches full identifiers (letters, digits, underscores). After matching the full token, `IdentifierMatcher` checks if the result is in the keywords list. It emits `TT_KEYWORD` only when the **complete token** is a reserved word.

**Effect:** `do_round`, `in_count`, `out_val`, `not_ready`, etc. now lex as single `TT_WORD` tokens. Bare `do`, `in`, `out`, `not`, etc. (followed by whitespace or symbol) still lex as `TT_KEYWORD`. Reserved words are still fully reserved as bare identifiers.

**No grammar change.** The grammar already specified that identifiers must not equal a reserved keyword; this is a lexer implementation fix bringing the lexer into conformance with that rule.

---

### Gap 13: Silent Stack Overflow on Deep Expression Nesting

**Problem:** The tree-walking evaluator's `eval_expr` function was purely recursive with no depth limit. A deeply nested AST (e.g. a long `let ... in` chain, deeply nested binary operations, or long field-access chains) would silently crash the Keln process with a Rust stack overflow (`STATUS_STACK_OVERFLOW` on Windows, SIGSEGV on Linux). No error message, no line number, no recovery.

**Resolution:** Two changes were made:

1. **`LetIn` and `ClosureExpr` chains made iterative.** The recursive `eval_expr → eval_expr(body)` call for consecutive `let x = e in ...` chains was replaced with an explicit loop. The loop accumulates pushed scopes and only calls `eval_expr_impl` once on the final non-let expression. This eliminates O(N) Rust stack depth for `let`-chains of length N.

2. **Expression depth counter added.** `Evaluator` now tracks `expr_depth: usize`. The public `eval_expr` wrapper increments this counter, delegates to `eval_expr_impl`, then decrements. If `expr_depth ≥ MAX_EXPR_DEPTH` (currently 10,000), it immediately returns a `RuntimeError` with the message `"expression nesting depth limit (10000) exceeded"` instead of crashing.

**Effect:** Long `let`-chains (the common case) no longer consume any extra Rust stack at all. Pathologically deep expression trees (nested binary ops, deeply recursive non-tail calls) produce a clean runtime error instead of a silent crash.

**No grammar change.** This is a pure evaluator implementation fix.

---

### Gap 14: `Map.fold` Stdlib Primitive

**Problem:** To reduce a map to a single value, the only option was `Map.toList(map)` + `List.fold(...)`. This required the fold callback to receive `{ acc: A, item: { key: K, value: V } }` — two levels of nesting — and if additional context was needed, it had to be threaded through the accumulator alongside the actual reduction state.

**Resolution:** `Map.fold(map, init, fn)` was added to the stdlib. The callback receives `{ acc: A, key: K, value: V }` directly — one flat record with no `.item` intermediate.

**Signature:**
```keln
Map.fold { Map<K,V>, A, FunctionRef<E, {acc:A, key:K, value:V}, A> -> A | E }
```

**Example:**
```keln
let total = Map.fold(scores, 0, sumValues)
helpers: {
    sumValues :: Pure { acc: Int, key: String, value: Int } -> Int =>
        it.acc + it.value
}
```

**Iteration order:** Map entries are visited in ascending key order (keys are sorted in the underlying `BTreeMap`). This is deterministic and consistent with `Map.keys`, `Map.values`, and `Map.toList`.

**`Map.toList` field name clarification:** Items produced by `Map.toList` have field `value` (not `val`). The correct type is `List<{ key: K, value: V }>`.

**No grammar change.** This is a pure stdlib addition.

---

## Addendum 5: Debug-Mode Stack Overflow and Recursive Algorithms

### Gap 15: Rust Stack Overflow in Debug Builds for Moderately Deep Evaluation

**Problem:** Keln's tree-walking evaluator runs on the Rust call stack. In unoptimized (`dev`) builds, each Rust stack frame is significantly larger than in optimized builds because the compiler does not reuse stack slots across match arms. The `eval_expr_impl` function has ~20+ match arms, each with distinct locals; in debug mode the frame size for this single function can be several KB. When Keln code calls user-defined functions through closures (e.g., a `List.fold` step closure that calls a top-level function which itself calls `List.fold`), each nesting level stacks up multiple large Rust frames:

`call_fn` → `eval_fn_once` → `eval_tail` → `eval_tail` (LetIn loop final) → `eval_tail` (List.fold call) → `stdlib::dispatch` → `call_value` → `eval_expr` → `eval_expr_impl` → …

With Windows' default 1MB thread stack, even 5–10 levels of such nesting can overflow in a `dev` build, while the same code runs fine in `release`.

**Symptoms:** `thread 'main' has overflowed its stack` / `STATUS_STACK_OVERFLOW` on Windows, even when `keln check` (parse-only) succeeds and `call_depth` is nowhere near `MAX_CALL_DEPTH`.

**Resolution:** `src/main.rs` was changed to spawn the main async runtime on a thread with an explicit 64 MB stack:

```rust
fn main() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async_main())
        })
        .unwrap()
        .join()
        .unwrap();
}
```

`#[tokio::main]` was removed and the former `async fn main()` was renamed `async fn async_main()`. All 270 existing tests continue to pass.

**Effect:** Both `dev` and `release` builds now have sufficient stack for deeply-nested or multi-level Keln evaluation. The evaluator's existing `MAX_CALL_DEPTH` (2000) and `MAX_EXPR_DEPTH` (10,000) guards still fire before the 64 MB stack would be exhausted.

**No grammar change.**

---

### Gap 16: Explicit Recursion Through Closures Causes Stack Overflow (Before Fix)

**Problem (design pain point):** When an AI author writes a naturally recursive algorithm in Keln — e.g., a DFS that calls itself through a named capturing helper (`let step :: ... => ... count_paths(...) in List.fold(...)`) — the Rust call stack grows proportionally to the recursion depth. Before the 64 MB stack fix, even shallow recursion (9 levels for a 13-node example graph) caused `STATUS_STACK_OVERFLOW` in debug builds.

**Workaround used:** The recursive DFS was replaced with an iterative round-based DP (same pattern as day_11). Instead of `count_paths` calling itself through a closure, the solution maintains a `Map<String, Int>` memo keyed by `"node:d:f"` and runs repeated passes over all nodes until convergence. Each pass uses only `List.fold` with non-recursive callbacks, eliminating all Rust stack growth from the algorithm itself.

**For AI authors (before the fix was applied):** If a recursive Keln function stack-overflows but `keln check` succeeds and `call_depth` is low, the cause is debug-mode Rust frame sizes, not algorithmic depth. The workaround is to convert the recursion to an iterative fixpoint computation.

**After the fix:** Explicit recursion through closures works correctly in both `dev` and `release` builds, up to the `MAX_CALL_DEPTH` = 2000 limit.

**No grammar change.**

---

### Gap 17: Named Capturing Helpers Not Supported in Bytecode VM (Resolved)

**Problem:** The bytecode VM (`keln compile` / `keln run-bc`) previously rejected any program using `let name :: effects In -> Out => body in rest` syntax with the error *"named capturing helpers are not supported in the bytecode VM; use the tree-walking evaluator"*.

**Resolution — Closure Lifting:**

The compiler now performs *closure lifting*: each `ClosureExpr` is compiled into a new top-level `KelnFn` whose single input is the merged record `{ it: <arg>, cap1: v1, cap2: v2, ... }`. The captured variable values are snapshotted at definition time by the new `MakeClosure` bytecode instruction.

**Implementation details:**

| Component | Change |
|-----------|--------|
| `src/eval/mod.rs` | Added `Value::VmClosure { fn_idx: usize, captures: Vec<(String, Value)> }` |
| `src/vm/ir.rs` | Added `Instruction::MakeClosure { dst, fn_idx, capture_regs: Vec<(String, usize)> }` |
| `src/vm/ir.rs` | Added `"Map.fold"` to `BUILTIN_NAMES` (index 170) for VM dispatch |
| `src/vm/lower.rs` | `Lowerer::lower_closure_expr` — snapshots scope, registers lifted `KelnFn`, emits `MakeClosure` |
| `src/vm/lower.rs` | `Lowerer::lower_closure_body` — builds lifted fn's `FnCtx`, extracts `it` + each capture via `FieldGetNamed` |
| `src/vm/exec.rs` | `MakeClosure` handler, `VmClosure` branches in `CallDyn`/`TailCallDyn` |
| `src/vm/exec.rs` | `VmClosure` paths in `List.fold`, `List.map`, `List.filter`, `List.foldUntil`, `Map.fold` higher-order builtins |

**Calling convention for lifted closures:**

When a `VmClosure` is called with argument `arg`, the VM builds the record `{ it: arg, cap1: v1, ... }` and calls the lifted function with it as R0. The lifted function's preamble immediately extracts `it` and each capture with `FieldGetNamed` into local registers.

**Capture strategy:** All currently-in-scope variables (excluding `_`-prefixed internal names and `it`) are captured at definition time. This is conservative (may capture unused variables) but always correct.

**Higher-order builtins with VmClosure:** `List.fold`, `List.map`, `List.filter`, `List.foldUntil`, and `Map.fold` all dispatch through the VM when their function argument is a `VmClosure`, rather than delegating to the tree-walking stdlib. `Map.fold` is also now available in the bytecode backend (`BUILTIN_NAMES[170]`).

**No grammar change.** The `let name :: effects In -> Out => body in rest` syntax is unchanged.

---

## Addendum 6: AI-Author Friction Reduction

### Gap 18: `List.getOr` and Record `.with()` in the Bytecode VM

**Problem:** Two patterns caused disproportionate verbosity in AI-authored Keln programs:

1. **Indexed list access** required `Maybe.getOr(List.head(List.drop(list, i)), default)` — 5 function calls for a single array lookup.
2. **Record field update** in fold accumulators required retyping every unchanged field: `{ acc: it.acc + 1, w: state.w, h: state.h, board: state.board }` instead of just `it.with(acc: it.acc + 1)`. Record `.with()` was implemented in the tree-walking evaluator (Gap 11) but the bytecode VM's `MakePartial` handler only supported `FnRef` bases, crashing on `Record` bases.

**Resolution:**

**`List.getOr(list, i, default)`** added to stdlib:
- Returns `list[i]` if `0 <= i < len(list)`, otherwise returns `default`
- Registered in `is_stdlib`, `dispatch`, and `BUILTIN_NAMES[171]`
- Replaces the verbose `Maybe.getOr(List.head(List.drop(...)))` idiom everywhere

**Record `.with()` in the bytecode VM** — `MakePartial` in `src/vm/exec.rs` now branches on the base value type:
- `Value::Record` base → apply overrides in-place (matching field names updated, new names appended) and return a new `Value::Record`
- `Value::FnRef` / `Value::PartialFn` base → existing partial application behavior unchanged

**Usage:**
```keln
-- Indexed access (replaces 5-call chain):
List.getOr(mem, ip, 0)

-- Record field update in fold steps (both verify and run-bc):
it.with(ip: it.ip + 4)
it.with({ ip: it.ip + 4, done: false })
it.with(ip: 0).with(done: true)
```

**`Map.getOr(map, key, default)`** added to stdlib:
- Returns `map[key]` if key exists, otherwise returns `default`
- Registered in `is_stdlib`, `dispatch`, and `BUILTIN_NAMES[172]`
- Replaces `Maybe.getOr(Map.get(map, key), default)` — symmetric with `List.getOr`

**`let { f1, f2, f3 } = expr in body` record destructuring** — already implemented in the parser, tree-walker, and VM lowering (no code changes needed). Eliminates the `it.acc.` double indirection in fold helpers:
```keln
-- BEFORE: it.acc. everywhere
it.acc.ip + 4 ... it.acc.mem ... it.acc.done

-- AFTER: destructure at the top of the helper
let { mem, ip, done } = it.acc in
ip + 4 ... mem ... done
```
The VM lowering emits `FieldGetNamed` for each field in the pattern. Shorthand `{ f }` binds `f`; named `{ field: alias }` binds `alias`.

**Remaining friction (not addressed in this gap):**
- Fold step type annotation ceremony: `let step :: Pure { acc: T, item: U } -> T =>` still requires explicit types. Type inference for closure signatures would require a full type-checker implementation and is deferred.
- `foldUntil` boilerplate guard: `match it.acc.done { true -> it.acc, false -> ... }` in each step is unavoidable without a `break`-aware fold primitive. The combination of `.with()` record update, `let`-destructuring, and named capturing helpers reduces the per-step verbosity significantly even without this.

**No grammar change.**
