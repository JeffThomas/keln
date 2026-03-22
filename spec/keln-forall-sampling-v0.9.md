# Keln forall Sampling — Implementation Specification
## Version 0.9

This document specifies the deterministic stratified sampling algorithm
used to generate inputs for `forall` property checks. It supplements
section 10.6 of the main spec and documents the algorithm implemented in
`src/verify/sample.rs`.

---

## 1. Overview

A `forall` property has the form:

```keln
forall(b₁: T₁ [where C₁], b₂: T₂ [where C₂], ...) -> <logic_expr>
```

The executor generates a finite set of sample rows. Each row assigns one
value to each binding. The logic expression is evaluated for each row. A
counterexample is the first row for which the expression evaluates to `false`
(or produces a runtime error).

**Design goals:**
1. **Deterministic** — same source produces same samples across runs
2. **Boundary-first** — values most likely to expose edge cases come first
3. **Refinement-aware** — `where` constraints narrow the sample space
4. **Budget-bounded** — at most 1000 rows; timeout at 5000ms (Phase 3+)

---

## 2. Per-Type Sample Generation

Each binding independently generates a list of candidate values. The final
rows are the Cartesian product of all per-binding lists (capped at budget).

### 2.1 Int

**Inputs:** base type `Int`; optional refinement constraint.

**Step 1 — Determine bounds:**

```
bounds(refinement):
    None                     → lo = -10,  hi = 10
    Range(lo, hi)            → lo = lo,   hi = hi
    Comparison(>=, n)        → lo = n,    hi = n + 20
    Comparison(>, n)         → lo = n+1,  hi = n + 21
    Comparison(<=, n)        → lo = n-20, hi = n
    Comparison(<, n)         → lo = n-21, hi = n-1
    Comparison(==, n)        → lo = n,    hi = n
    Comparison(!=, n)        → lo = -10,  hi = 10  (unconstrained)
```

**Step 2 — Deterministic sample set (in order):**

1. `lo` — lower bound
2. `hi` — upper bound
3. `lo + (hi - lo) / 2` — midpoint (integer division)
4. `lo + 1` — just above lower bound (if lo + 1 < hi)
5. `hi - 1` — just below upper bound (if lo + 1 < hi)
6. `0` — if `lo <= 0 <= hi`
7. `1` — if `lo <= 1 <= hi`
8. Eight LCG pseudo-random values in `[lo, hi]` (see §2.1.1)

After generating, sort and deduplicate.

**2.1.1 LCG Parameters:**

```
state₀ = 12_345_678_901_234_567  (u64)
stateₙ = stateₙ₋₁ × 6_364_136_223_846_793_005 + 1_442_695_040_888_963_407  (mod 2⁶⁴)
value  = lo + (stateₙ >> 33) % (hi - lo + 1)   (as i64)
```

The LCG is a Knuth multiplicative LCG with additive constant from PCG.
Fixed seed guarantees identical samples for identical constraints across runs.

**Typical output for `Int where 0..10`:**
`[0, 1, 2, 5, 9, 10, ...]` plus LCG values within `[0, 10]`.

### 2.2 Float

**Fixed sample set** (refinement-unaware by default):

```
[0.0, 1.0, -1.0, 0.5, -0.5, 100.0, -100.0]
```

**Refinement-aware addition** (when `where lo..hi` is present):

```
lo, hi, lo + (hi-lo)/2, lo + (hi-lo)/4, lo + 3*(hi-lo)/4
```

These are appended to the fixed set before use (no deduplication for
Float due to approximate equality concerns).

**Note:** `Float.approxEq` should be used in forall property expressions
rather than `==` for Float comparisons.

### 2.3 Bool

Always: `[false, true]`

No refinement constraints apply to Bool.

### 2.4 String

**Fixed sample set:**

```
["", "a", "hello", "hello world", "123"]
```

**Refinement addition:**
- `where len > n`: append `"x".repeat(n + 1)` (minimum-length satisfying string)

**Phase 2 limitation:** `where matches(regex)` constraints do not narrow
the String sample set. This is a coverage gap reported in
`VerificationResult.coverage_gaps`.

### 2.5 Bytes

Fixed: `[[], [0x61, 0x62]]` (empty and two-byte sample)

### 2.6 Unit

Fixed: `[Unit]`

### 2.7 Bool (named type alias)

Same as §2.3: `[false, true]`

### 2.8 Named Types

For a named type `T` that resolves to a known primitive, delegate to the
primitive sampler. For unknown named types (opaque or user-defined without
a `type` declaration visible to the sampler), emit:

```
[Variant { name: T, payload: Unit }]
```

This is a single nominal placeholder — sufficient for "does not crash"
checks but not for equality-based properties on the type's structure.

### 2.9 List\<T\>

Three coverage samples:

```
[]                      -- empty list
[sample₀(T)]           -- one element: first sample of inner type
[sample₀(T), sample₁(T)] -- two elements: first two samples of inner type
```

Where `sample₀(T)` is the first value generated for type `T`.

### 2.10 Maybe\<T\>

```
None
Some(sample₀(T))
Some(sample₁(T))      -- if T has at least 2 samples
```

### 2.11 Result\<T, E\>

```
Ok(sample₀(T))
Ok(sample₁(T))        -- if T has at least 2 samples
Err(sample₀(E))
Err(sample₁(E))       -- if E has at least 2 samples
```

### 2.12 Product Types (inline record)

Generate exactly one sample: a record where each field gets `sample₀` of
its type (with any inline refinement applied to the per-field sampler):

```
{ field₁: sample₀(T₁, C₁), field₂: sample₀(T₂, C₂), ... }
```

Product types in forall bindings generate a single representative sample.
Coverage for product types comes from binding individual fields separately
when precision is needed.

---

## 3. Cartesian Product Generation

Given bindings `[b₁, b₂, ..., bₙ]` with per-binding sample lists
`[S₁, S₂, ..., Sₙ]`, the executor generates rows by Cartesian product:

```
rows = S₁ × S₂ × ... × Sₙ
     = { (v₁, v₂, ..., vₙ) | vᵢ ∈ Sᵢ }
```

**Budget enforcement:**

The product is generated lazily. Once `budget` rows have been produced,
generation stops:

```
cartesian_samples(bindings, budget = 1000):
    result = [[]  -- one empty row
    for each binding bᵢ with samples Sᵢ:
        next = []
        for each existing_row in result:
            for each v in Sᵢ:
                next.append(existing_row ++ [(bᵢ.name, v)])
                if len(next) >= budget:
                    return truncate(next, budget)
        result = next
    return result
```

**Order:** rows are generated in declaration order of bindings, inner loop
over sample values. This means boundary values for the first binding are
exhausted before varying the second binding — ensuring boundary combinations
are explored first.

**Single binding:** `forall(n: Int where 0..10)` generates up to ~15 rows
(boundary + midpoint + LCG values for the range).

**Two bindings:** `forall(a: Int where 1..5, b: Int where 1..5)` generates
up to `|S_a| × |S_b|` rows. For 5-value ranges this is ~25; budget is never
hit. For wider ranges the budget truncates after 1000 rows.

---

## 4. Evaluation and Result Collection

For each row `(b₁=v₁, b₂=v₂, ..., bₙ=vₙ)`:

```
evaluate_row(row, logic_expr):
    env.push_scope()
    for (name, val) in row:
        env.bind(name, val)
    result = eval_logic(logic_expr)
    env.pop_scope()
    return result
```

**eval_logic semantics:**

| Logic form              | Evaluation                                         |
|-------------------------|----------------------------------------------------|
| `Comparison(l, op, r)`  | eval l, eval r, apply op                          |
| `DoesNotCrash(expr)`    | eval expr; if Bool result → use it; else → true   |
| `Not(p)`                | !eval_logic(p)                                     |
| `And(p, q)`             | short-circuit: false if p is false                 |
| `Or(p, q)`              | short-circuit: true if p is true                   |
| `Implies(p, q)`         | if !p → true (vacuous); else eval q                |

**DoesNotCrash with Bool:** when the forall body is a comparison expression
that the parser wraps in `DoesNotCrash` (because it cannot distinguish
`double(n) >= 10` from a "does not crash" expression at parse time), the
Bool result is used as the logic value. This makes `forall(n: Int) ->
someCheck(n)` behave as a boolean property check, not just a crash check.

---

## 5. ForAll Outcome

```rust
enum ProofStatus {
    Passed { iterations: usize },
    Failed,
    Timeout,
    Error(String),
}

struct ForAllOutcome {
    status:          ProofStatus,
    counterexample:  Option<Vec<(String, String)>>,  -- binding name → display value
    iterations:      usize,
}
```

**Passed:** all rows evaluated to `true`. `iterations` = number of rows tried.

**Failed:** first row that evaluated to `false`. `counterexample` contains
the binding values as display strings. `iterations` = row index of failure.

**Error:** `eval_logic` returned a `RuntimeError` for some row. The error
message is captured in `ProofStatus::Error`. `counterexample` contains the
row that caused the error.

**Timeout:** (Phase 3+) evaluation exceeded 5000ms wall clock. Not
implemented in Phase 2 — the iteration budget (1000) is the only cap.

---

## 6. Budget and Timeout Policy

| Parameter        | Phase 2 value       | Phase 3+ target |
|------------------|---------------------|-----------------|
| Max iterations   | 1000                | 1000            |
| Wall-clock limit | not enforced        | 5000ms          |
| Result on budget | `Passed { iterations: 1000 }` | same   |
| Result on timeout| not applicable      | `Timeout`       |

The iteration budget is enforced at the Cartesian product generation stage.
A budget-exhausted run that found no counterexample reports
`Passed { iterations: 1000 }` with `coverage: Bounded` implied.

---

## 7. Interaction with Refinement Constraints

The sampler reads `ForAllBinding.refinement: Option<RefinementConstraint>`.

```
struct ForAllBinding {
    name:        String,
    type_expr:   TypeExpr,
    refinement:  Option<RefinementConstraint>,
}
```

**Constraint routing:**

```
sample_for_binding(binding):
    sample_for_type(binding.type_expr, binding.refinement)

sample_for_type(Int, Some(Range(lo, hi))):
    bounds = (lo, hi)
    → Int sample algorithm with refined bounds

sample_for_type(Int, None):
    bounds = (-10, 10)
    → Int sample algorithm with default bounds
```

This ensures `forall(n: Int where 0..100)` generates only non-negative
values ≤ 100, rather than using the default `-10..10` range.

---

## 8. Worked Examples

### Example 1 — `double(n) >= 0` with `n: Int where 0..100`

Bounds: lo=0, hi=100. Generated values (pre-dedup):
`[0, 100, 50, 1, 99, 0, 1, <LCG values in 0..100>]`

After dedup and sort (approximately):
`[0, 1, 3, 17, 50, 57, 99, 100, ...]` — 10–15 values.

All pass `double(n) >= 0` since `n >= 0`.

### Example 2 — `double(n) >= 10` with `n: Int where 0..20`

Generated values include `0`. `double(0) = 0`, which is `< 10`.

First row: n=0. Logic: `0 >= 10` → false. Counterexample: `{ n: "0" }`.
`ForAllOutcome { status: Failed, counterexample: [("n", "0")], iterations: 1 }`

### Example 3 — Two-binding property

```keln
forall(a: Int where 1..5, b: Int where 1..5) ->
    implies(a < b, double(a) < double(b))
```

Samples for a: `[1, 2, 3, 4, 5]` (5 values)
Samples for b: `[1, 2, 3, 4, 5]` (5 values)
Total rows: 25. All are evaluated. All pass (double is monotone increasing).

`ForAllOutcome { status: Passed { iterations: 25 }, ... }`
