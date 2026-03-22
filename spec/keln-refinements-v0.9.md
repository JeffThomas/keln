# Keln Refinement Constraint Evaluator — Specification
## Version 0.9

This document specifies how Keln's refinement constraints are expressed,
where they are checked, and what the compiler and runtime must do to
enforce them. It supplements sections 3.5 and 3.7 of the main spec.

---

## 1. What Refinement Constraints Are

A refinement constraint narrows a base type to a subset of its values.
The constraint is written `where <predicate>` in a type declaration or
inline in a product type field.

```keln
type Port           = Int    where 1..65535
type NonEmptyString = String where len > 0
type Email          = String where matches(RFC5322)
type Probability    = Float  where 0.0..1.0
type Positive<T>    = T      where > 0
type UserId         = String where len == 36
```

Inline in a product or sum type:

```keln
type JobState =
    | Running    { attempt: Int where >= 1 }
    | Completed  { attempt: Int where >= 1 }
    | DeadLetter { attempts: Int where >= 1 }
```

---

## 2. Constraint Forms

### 2.1 Numeric Range

```
where lo..hi
```

Inclusive on both ends. `lo` and `hi` are integer or float literals.
Both must be of the same numeric type as the base type.

```keln
type Port        = Int   where 1..65535
type Probability = Float where 0.0..1.0
```

**Check:** `lo <= value <= hi`

### 2.2 Numeric Comparison

```
where >= n
where >  n
where <= n
where <  n
where == n
where != n
```

Single-sided bound. `n` is a literal of the same numeric type.

```keln
type Positive<T> = T where > 0
-- attempt: Int where >= 1
```

**Check:** apply the comparison operator against `n`.

### 2.3 String Length

```
where len > n
where len >= n
where len < n
where len <= n
where len == n
where len != n
```

`len` is the number of Unicode scalar values (characters) in the string.

```keln
type NonEmptyString = String where len > 0
type UserId         = String where len == 36
```

**Check:** compare `String.length(value)` against `n` using the operator.

### 2.4 Pattern Match

```
where matches(<regex_name>)
```

`<regex_name>` is a named regular expression constant. In Phase 2 this is
treated as a declared-but-not-enforced constraint — the type checker records
it, but runtime enforcement requires the regex engine (Phase 3+).

```keln
type Email = String where matches(RFC5322)
```

**Phase 2 behavior:** constraint is recorded and emitted in
`VerificationResult.coverage_gaps`; no runtime check is applied.

### 2.5 Trait Bound

```
where T: Cloneable
```

Used in generic type declarations. Constrains the type parameter to types
that satisfy the `Cloneable` trait. Checked at instantiation time by the
type checker, not at runtime.

```keln
List.clone { Pure List<T> where T: Cloneable -> List<T> }
```

**Phase 2 behavior:** the type checker verifies that the concrete type
bound to `T` satisfies `Cloneable` (see spec section 3.8 for derivation
rules). No runtime component.

---

## 3. When Constraints Are Checked

### 3.1 At Construction Time — `from` Constructors

Refinement type aliases expose a `from` constructor that returns
`Result<T, E>`. The check is a runtime guard:

```keln
Port.from { Pure String -> Result<Port, PortError> }
```

The caller provides the base value; the constructor checks the constraint
and returns `Ok(value)` if it passes or `Err(...)` with a caller-defined
error type if it fails.

**Compiler obligation:** The compiler does NOT automatically check refinement
constraints at assignment. All enforcement is via `from` constructors.
Direct construction of a refined type without `from` is a compile error.

```keln
let p: Port = 8080         -- COMPILE ERROR: use Port.from
let p = Port.from("8080")  -- OK: returns Result<Port, PortError>
```

### 3.2 Inline in Product/Sum Types — At Variant Construction Time

When a sum type variant or product type field carries an inline refinement,
the constraint is checked when constructing the containing record.

```keln
type JobState =
    | Running { attempt: Int where >= 1 }
```

Constructing `Running { attempt: 0 }` is a runtime error — the attempt
field fails `>= 1`. The constructor returns `Err(ConstraintViolation { ... })`
or panics depending on the context.

**Phase 2 implementation:** the evaluator checks inline refinements at
record construction time and raises `RuntimeError` with a descriptive
message if violated.

### 3.3 In forall Bindings — At Sample Generation Time

```keln
forall(n: Int where 1..65535) -> ...
```

The `where` constraint in a `forall` binding narrows the sample space. The
sampler generates values satisfying the constraint rather than from the
full type domain. See `keln-forall-sampling-v0.9.md` for the algorithm.

---

## 4. Constraint Checking Algorithm

### 4.1 Numeric Range Check

```
check_range(value, lo, hi):
    return lo <= value AND value <= hi
```

Both `lo` and `hi` are inclusive. If either bound is omitted (open range),
the omitted end is the type's natural bound (`Int.MIN` / `Int.MAX` for `Int`,
`-∞`/`+∞` for `Float`).

### 4.2 Numeric Comparison Check

```
check_comparison(value, op, n):
    match op:
        ">="  -> value >= n
        ">"   -> value > n
        "<="  -> value <= n
        "<"   -> value < n
        "=="  -> value == n
        "!="  -> value != n
```

### 4.3 String Length Check

```
check_len(value, op, n):
    L = length_in_chars(value)
    check_comparison(L, op, n)
```

`length_in_chars` counts Unicode scalar values, not bytes.

### 4.4 Evaluation Order

For a product type with multiple constrained fields, fields are checked in
declaration order. The first failing constraint produces the error. All
remaining constraints are skipped (fail-fast).

---

## 5. Error Reporting

### 5.1 from Constructors

The error type is caller-defined. The canonical pattern:

```keln
type PortError = | OutOfRange { value: Int } | NotANumber { input: String }

fn parsePort {
    Pure String -> Result<Port, PortError>
    in:  s
    out: match Int.fromString(s) {
        Err(_)  -> Err(PortError.NotANumber { input: s })
        Ok(n)   -> match n >= 1 and n <= 65535 {
            true  -> Ok(n)
            false -> Err(PortError.OutOfRange { value: n })
        }
    }
}
```

The `from` constructor is a regular Keln function. The constraint check is
explicit in the `out` expression.

### 5.2 Inline Constraints

For inline variant/product constraints, the runtime raises a
`ConstraintViolation` error with the field name, the constraint, and the
actual value:

```
ConstraintViolation {
    field:      "attempt",
    constraint: ">= 1",
    actual:     "0"
}
```

### 5.3 Compiler Errors

The type checker emits a compile error for:
- Direct construction of a refinement type alias without `from`
- Type parameter `T` instantiated with a non-Cloneable type for
  `where T: Cloneable` bounds

These are `CompileError` entries in `VerificationResult.compile_errors`.

---

## 6. Interaction with the Verification System

### 6.1 forall Sampling

The `forall` sampler reads `where` constraints from binding declarations
and generates values within the constrained range. This ensures:
- `forall(n: Int where 1..65535)` never generates 0 or 65536
- `forall(s: String where len > 0)` never generates `""`

### 6.2 Type Checker in verify Blocks

The type checker runs before verify execution. It validates:
- `given` input expression types match the function's declared input type
- `given` expected expression types match the function's declared output type
- `mock` return expression types match the mocked FunctionRef's output type

Constraint violations in `given` inputs that pass type checking but fail
runtime constraints surface as `TestFailure.RuntimeError` entries.

---

## 7. Phase 2 Implementation Status

| Constraint Form        | Type Check | Runtime Check | Sample-aware |
|------------------------|------------|---------------|--------------|
| `where lo..hi`         | ✓          | ✗ (Phase 3)   | ✓            |
| `where >= n` etc.      | ✓          | ✗ (Phase 3)   | ✓            |
| `where len op n`       | ✓          | ✗ (Phase 3)   | ✓ (basic)    |
| `where matches(regex)` | recorded   | ✗ (Phase 3+)  | ✗            |
| `where T: Cloneable`   | ✓          | n/a           | n/a          |

"Type Check" here means: the constraint is parsed, stored in the AST, and
used by the sampler. Full runtime enforcement (preventing construction of
out-of-range values) is Phase 3.

The `from` constructor pattern delegates enforcement to explicitly-written
Keln code, which executes correctly in Phase 2.
