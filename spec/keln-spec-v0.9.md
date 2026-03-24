# Keln Language Specification
## Version 0.9 — Draft

> Keln is a programming language designed by AI, for AI authorship. It is not
> optimized for human readability. It is optimized for unambiguous expression,
> structural correctness, iterative self-verification, and the elimination of
> entire classes of bugs at the type level. The name derives from *kiln* —
> raw materials enter, something refined and structural emerges.

---

## 0. Status and Implementation Intent

Keln is being implemented. This is not a thought exercise or a research artifact.
Phase 2 (the tree-walking interpreter in Rust) begins when Phase 1 is complete:
formal EBNF grammar written and all spec items resolved.

**v0.9 is the third post-validation release.** A third independent AI extended
the job queue system all the way to a distributed, lease-based, durable queue —
SQS-level complexity across four iterative steps: core state transitions, worker
processing with FunctionRef mocking, persistent store integration, and distributed
leasing with crash recovery. The language held at every step without requiring
any new features. Six minor gaps were identified and are resolved in this version.

**Cumulative validation summary:**

- Exercise 1 (our own, v0.6): 13 issues → resolved in v0.7
- Exercise 2 (second AI, v0.7): 6 issues → resolved in v0.8
- Exercise 3 (third AI, v0.8): 6 minor gaps → resolved in v0.9

The design is stable. No fundamental redesigns have been required across three
independent validation exercises totalling a distributed production-grade queue
system with retries, backoff, persistence, leasing, and crash recovery.

**v0.9 changes:**
1. `Never` as function return type — specified for non-terminating tail-recursive
   functions; previously only specified as error type
2. `Timestamp.sub` safety semantics — explicitly unchecked; callers must guard
3. `Channel.new<T>()` syntax — standardized with parentheses
4. `.with` record form — multi-field partial application syntax specified
5. `lease_until` in `Running` variant — updated canonical `JobState` example
6. Distributed boundary as consequence of Tenet 7 — documented

---

## 1. Design Philosophy

### 1.1 On Human Ergonomics — A Definitive Statement

This section exists because every external review of Keln raises human ergonomics
as a concern. This section answers that concern completely and finally. It will not
be revisited in future versions.

**Keln is not hostile to humans. It is indifferent to them as authors.**

Keln does not make things hard for humans out of perversity — it makes things
optimal for its actual author, which is an AI system. When a design decision
is painful for humans but better for AI generation accuracy, the AI wins. That
is not a flaw. It is the specification of the author.

**The human ergonomics concern conflates two separate things:**

The first is *authorship ergonomics* — the experience of writing Keln. Keln is
not designed for human authorship. Humans do not write Keln. AI systems write
Keln. Optimizing the authorship experience for humans would be like optimizing
a CNC machine's control protocol for hand-typing. The protocol's author is a
CAD system, not a person.

The second is *inspection ergonomics* — the experience of reading and debugging
Keln programs. This is a real concern and Keln addresses it — not by compromising
the language, but by making the language emit rich structured data that tooling
can render for humans. `VerificationResult`, `Confidence`, `Provenance`,
`reason` fields, PatternDB history — these are structured data that a debugger,
monitoring system, or code review tool can consume and render in any form
appropriate for human inspection.

The language and the tooling are separate layers. The language is optimized for
its author. The tooling is optimized for its reader.

**Why specific human-unfriendly decisions were made:**

*No lambdas:* Anonymous callables cannot carry `verify` blocks, `confidence`,
or `provenance`. Untracked execution is incompatible with the verification and
learning loop — not inconvenient, incompatible.

*No `if`/`else`:* One form — exhaustive match — covers all cases and produces
a compile error when cases are missing. There is no reason to have two forms
when one is strictly superior for the author.

*Verbosity in declarations:* Every field occupies the same position every time.
Zero cognitive cost of "which form does this use?" Verbosity that eliminates
ambiguity is not a cost for an AI author.

*Everything named:* Naming is free for an AI. The gain — every callable is
referenceable, testable, attributable — is structural and permanent.

**What this means for reviewers:**

A reviewer observing that Keln is "verbose," "unfriendly," or "would be painful
to write by hand" is making an accurate observation about a property Keln does
not optimize for. This is not a gap. It is the design working as intended.
Future reviews that raise human ergonomics as a concern should be understood
in this context.

### 1.2 What Keln Is and Why

Keln is a **programming language** — not a compiler IR, not a bytecode format,
not an assembly target for other tools.

**Why not an IR?**
An IR cannot carry `confidence`, `provenance`, `reason`, or `verify` blocks.
These are author-level concepts. Stripping them away destroys the feedback loop
that makes Keln a learning system.

**Why not a human language with AI tooling?**
Human languages force AI to translate its internal reasoning into a form designed
for someone else. Translation introduces errors.

**Why a real executable language?**
A language that cannot build real software has not been tested against reality.
Keln must deploy a GraphQL server, an application server, a distributed job queue
in Docker or AWS.

### 1.3 Core Tenets

**Tenet 1 — No human focus.**
Syntax, ergonomics, and conventions serve AI generation accuracy. See section 1.1.

**Tenet 2 — Invalid programs are unrepresentable.**
The type system eliminates null, unchecked failure, missing match cases, untyped
errors, and shared mutable state structurally — not by convention.

**Tenet 3 — Everything is named.**
No anonymous functions, no lambdas, no inline callables. Every callable has a
declaration, a name, a signature, and optionally a verify block.

**Tenet 4 — Compilation and testing are the same loop.**
`verify` blocks are part of function declarations. One phase: verification.

**Tenet 5 — All feedback is structured data.**
The compiler is an API. Its output is `VerificationResult`.

**Tenet 6 — Uncertainty is explicit.**
`confidence` and `provenance` are first-class declaration fields.

**Tenet 7 — Effects are declared, not inferred.**
A function's entire behavior surface is visible in its signature. No hidden I/O,
no surprise mutations, no implicit logging.

**Consequence of Tenet 7 for distributed systems:**
In distributed systems, correctness often depends on the specific boundary where
`claim`, `update`, and `now()` are called. In conventional languages this
dependency is invisible — buried in implementation details. In Keln, these
dependencies are in the signature. You can point to exactly where correctness
depends on `JobStore.claim`, `JobStore.update`, and `Clock.now`. The distributed
boundary is explicitly visible in types. This was demonstrated by the third
validation exercise: a full distributed lease protocol expressed without any
hidden mechanisms.

**Tenet 8 — Immutable by default.**
All bindings are immutable. Mutation requires explicit `var`, prohibited across
task boundaries. Enforced structurally.

### 1.4 What Keln Is Not

- A language designed for human authorship or readability
- Compatible with existing ecosystems
- A compilation target for other languages
- A research artifact — it will be implemented and deployed

---

## 2. Runtime Model

### 2.1 Evaluation Model

Keln is **strictly evaluated** (eager). No lazy evaluation, no thunks.
`Task.spawn` is the only deferred execution mechanism.

**Pipeline evaluation:** Left to right. Output of step N is input to step N+1.

**Tail-call optimization (required):** The VM must implement TCO for direct
tail calls — a function calling itself as its final expression. Tail-recursive
IO functions are the idiomatic pattern for event loops and dispatch loops.
Without TCO, these patterns overflow the stack on any real workload.
Tree-walker: trampolining. Bytecode VM: loop rewrite at IR level.

### 2.2 VM Implementation

Rust. Tree-walking interpreter during development. Bytecode VM in Phase 4.
Tree-walker is the reference implementation.

**Execution phases:**
1. Parse source → typed AST
2. Type-check and effect-check → annotated AST
3. Run `verify` blocks → `VerificationResult`
4. If not `is_clean` → return to AI with structured errors
5. Lower to bytecode (Phase 4+)
6. Execute on Keln VM with work-stealing scheduler

### 2.3 Deployment

Standalone executables. Docker containers, AWS Lambda/EC2, any Linux-based
host. No runtime installation required.

### 2.4 Concurrency Model

Go-inspired channels with genuine parallelism. Shared mutable state across task
boundaries is a compile error — enforced structurally.

- **Tasks:** lightweight coroutines on a work-stealing scheduler (Tokio)
- **Channels:** typed, first-class, the only inter-task communication mechanism
- **select:** non-deterministic choice over ready channels
- **Fire-and-forget + feedback channel:** idiomatic for supervised worker pools

---

## 3. Type System

### 3.1 Primitive Types

```
Int        -- 64-bit signed integer
Float      -- 64-bit IEEE 754
Bool       -- true | false
String     -- UTF-8 immutable byte sequence
Bytes      -- raw byte sequence
Unit       -- the type of expressions with no meaningful value
Never      -- the type of expressions that do not return
```

**`Never` has two distinct uses:**

**Use 1 — Uninhabited error type.** A function returning `Result<T, Never>` is
structurally infallible — the `Err` variant cannot be constructed. The compiler
knows this and allows safe unwrapping without a match arm for `Err`.

```keln
fn healthHandler {
    Pure Request -> Result<Response, Never>
    -- structurally cannot fail; Err variant is uninhabited
}
```

**Use 2 — Non-terminating function return type.** A function that never returns
normally — because it runs an infinite tail-recursive loop — has return type
`Never`. This is the idiomatic return type for event loops and worker loops.

```keln
fn workerLoop {
    IO & Clock { ... } -> Never
    -- tail-recursive; never returns; TCO makes this safe and correct
    in:  ctx
    out: do {
        -- ... handle one event ...
        workerLoop(ctx)   -- direct tail call; result is Never
    }
    confidence: auto
    reason: "infinite loop via TCO; concurrency_not_verified"
}
```

The compiler verifies that a `Never`-returning function's `out` expression
always either calls itself as a direct tail call or calls another `Never`-typed
function. A `Never`-returning function cannot produce a value of any other type.

**`Never` and `do` blocks:** A `do` block whose final expression has type
`Never` also has type `Never`. This allows event loops expressed as `do` blocks
with a tail call as the final expression.

### 3.2 Algebraic Types

#### Sum Types

```keln
type Maybe<T>   = Some(T) | None
type HttpMethod = GET | POST | PUT | DELETE | PATCH
```

All match expressions must be exhaustive. Non-exhaustive match is a compile
error.

**Inline field refinements in sum type variants:**

Refinement constraints are permitted on fields within sum type variant product
types. Checked at construction time.

```keln
type JobState =
    | Running { attempt: Int where >= 1 }   -- inline refinement: valid
```

**Sum type variant field access:**

Field access on a sum type value requires an explicit match to bind the variant.
Direct dot-access on a sum type value without a prior match is a compile error.

```keln
let job: JobState = ...
let p = job.payload     -- COMPILE ERROR: cannot dot-access sum type directly

match job {
    Pending(p) -> p.payload    -- p is the Pending record; .payload is valid
    Running(r) -> r.payload
    ...
}
```

Idiomatic accessor for repeated access across variants:

```keln
fn payloadOf {
    Pure JobState -> Maybe<Bytes>
    in:  state
    out: match state {
        Pending(p)    -> Some(p.payload)
        Running(r)    -> Some(r.payload)
        Retrying(r)   -> Some(r.payload)
        Failed(f)     -> Some(f.payload)
        Completed(_)  -> None
        DeadLetter(_) -> None
    }
    confidence: 1.0
}
```

**Variant predicate pattern:**

Predicates like `isRunning` are named functions the AI author writes. Not in
the stdlib — they are trivial and domain-specific.

```keln
fn isRunning {
    Pure JobState -> Bool
    in:  state
    out: match state { Running(_) -> true; _ -> false }
    confidence: 1.0
}
```

#### Product Types

Fields are immutable. No null — absent optional fields use `Maybe<T>`.

### 3.3 Structural Equality

Structural equality (`==`) is defined for all types whose fields are
recursively equality-comparable. The compiler derives `Eq` automatically.

```
Primitives:
    Int: numeric equality
    Float: IEEE 754 (NaN != NaN); use Float.approxEq in forall properties
    Bool, String, Bytes, Unit: value equality
    Never: vacuously equal (uninhabited)

Product types: field-by-field; all fields must satisfy Eq
Sum types: same variant + equal fields; different variants always unequal

Does NOT satisfy Eq:
    Channel<T>: equality on channel endpoints is not meaningful
    Task<T>: equality on task handles is not meaningful
    Module instances: equality on stateful resources is not meaningful
```

`!=` is structural inequality, defined as `not(==)`.

`Float.approxEq { Pure Float, Float, Float -> Bool }` — use in forall
property expressions where exact IEEE 754 equality is unreliable.

### 3.4 Result<T, E> — Typed Errors

```keln
type Result<T, E> = Ok(T) | Err(E)
```

Domain errors are sum types. Match on `E` is exhaustive.

```keln
type DbError    = | NotFound { id: String } | Timeout { after: Duration }
                  | Constraint { field: String, violation: String }
type PortError  = | OutOfRange { value: Int } | NotANumber { input: String }
type HttpError  = | BadRequest { message: String } | Unauthorized
                  | NotFound | InternalError { message: String }
type EnvError   = | Missing { key: String } | Invalid { key: String, reason: String }
type JobError   = | ExecutionFailed { message: NonEmptyString }
                  | Timeout { after: Duration }
                  | MaxRetriesReached { attempts: Int }
                  | PayloadInvalid { reason: NonEmptyString }
type ParseError = | InvalidJson { offset: Int } | UnexpectedField { name: String }
                  | MissingField { name: String }
type LeaseError = | AlreadyClaimed { job_id: JobId }
                  | LeaseExpired { job_id: JobId }
```

Convenience alias for simple cases:
```keln
type SimpleError     = { message: NonEmptyString }
type SimpleResult<T> = Result<T, SimpleError>
```

### 3.5 Refinement Types

```keln
type Port           = Int    where 1..65535
type NonEmptyString = String where len > 0
type Email          = String where matches(RFC5322)
type Probability    = Float  where 0.0..1.0
type Positive<T>    = T      where > 0
type UserId         = String where len == 36
type JobId          = String where len == 36
type WorkerId       = String where len == 36
```

Runtime construction via `Result`-returning constructors:
```keln
Port.from { Pure String -> Result<Port, PortError> }
```

### 3.6 Generic Types

```keln
type List<T>        -- ordered immutable sequence
type Map<K, V>      -- immutable hash map
type Set<T>         -- immutable unordered unique set
type Channel<T>     -- typed concurrent channel (task-local ownership)
type Maybe<T>       -- optional value (replaces null)
type Result<T, E>   -- success or typed failure (replaces exceptions)
type Task<T>        -- handle to a spawned concurrent computation
type Ordering       -- LessThan | Equal | GreaterThan
```

### 3.7 State Machine Types

A state machine is a sum type whose variants represent states, combined with
transition functions returning `Result<NextState, TransitionError>`.

**Canonical `JobState` — updated with `lease_until` in `Running`:**

The third validation exercise (distributed leasing) demonstrated that `Running`
must carry `lease_until: Timestamp` to support distributed exclusivity. This is
the canonical validated form of `JobState`.

```keln
type JobState =
    | Pending    { job_id: JobId, payload: Bytes,
                   enqueued_at: Timestamp }
    | Running    { job_id: JobId, payload: Bytes,
                   started_at: Timestamp,
                   attempt: Int where >= 1,
                   worker_id: WorkerId,
                   lease_until: Timestamp }        -- distributed lease expiry
    | Failed     { job_id: JobId, payload: Bytes,  -- payload for retry
                   error: JobError,
                   attempt: Int where >= 1,
                   failed_at: Timestamp }
    | Retrying   { job_id: JobId, payload: Bytes,
                   attempt: Int where >= 1,
                   retry_after: Timestamp }
    | Completed  { job_id: JobId, result: Bytes,
                   completed_at: Timestamp,
                   attempt: Int where >= 1 }
    | DeadLetter { job_id: JobId, payload: Bytes,  -- payload for inspection
                   final_error: JobError,
                   attempts: Int where >= 1,
                   dead_at: Timestamp }
```

`payload` is present in `Failed` and `DeadLetter` — required for retry
re-execution and post-mortem inspection. `lease_until` in `Running` enables
distributed crash recovery: a worker that dies leaves an expired lease, which
causes the job to become visible again via `fetchExpired`.

**Exhaustive match return type:** When exhaustive match on a complete sum type
always produces a value, the return type is the value type directly — not
`Maybe<T>`.

### 3.8 Ownership and Cloning

Values passed into channels transfer ownership. Original binding invalidated
by the compiler.

```keln
let data = buildRequest()
ch <- data      -- data binding invalidated here

let s1, s2 = clone(jobState)
state_ch <- s1
audit_ch  <- s2
```

**Cloneable derivation (compiler-automatic):**
- All primitives: Cloneable
- Product types: Cloneable iff all fields Cloneable
- Sum types: Cloneable iff all variants Cloneable
- `List<T>`, `Map<K,V>`, `Set<T>`: Cloneable iff element types Cloneable
- `Channel<T>`: never Cloneable
- `Task<T>`: never Cloneable
- Module instances: not Cloneable by default

---

## 4. Effect System

### 4.1 Why `Fail` Was Removed

Effects describe what a function does to the world. Failure describes what it
returns. These are orthogonal. `Fail` encoded failure in both places, creating
two places to drift. Gone. Failure is `Result<T, E>`. Always. Only.

### 4.2 Built-in Effects

```
Pure    -- no side effects; may only call Pure functions
IO      -- may perform network, filesystem, or environment I/O
Log     -- may emit structured log output
Metric  -- may emit metrics or telemetry
Clock   -- may read or be influenced by the current time
```

### 4.3 Formal Effect Algebra

**Identity:** `Pure = ∅`

**Union:** `E1 & E2 = E1 ∪ E2`, `E & Pure = E`, `E & E = E`

**Subtyping:** `Pure ⊆ E` for any effect set E.
`FunctionRef<Pure, T, U>` satisfies `FunctionRef<E, T, U>` for any E.
`FunctionRef<IO, T, U>` does NOT satisfy `FunctionRef<Pure, T, U>`.

**Effect compatibility:** `E_callee ⊆ E_caller` (after Pure ⊆ E rule).

**Pipeline effect:** `effects(f |> g |> h) = effects(f) ∪ effects(g) ∪ effects(h)`

**Validated:** Three independent job queue exercises confirmed effect subtyping
works in practice. No `mapIO` was needed in any implementation.

### 4.4 Typed Function References

`FunctionRef<E, In, Out>` is the only way to pass a function as a value.

```keln
List.map { List<T>, FunctionRef<E, T, U> -> List<U> | effect E }
```

**Partial application via `.with`:**

`.with` binds named parameters into a function reference, producing a new
`FunctionRef` with all effects preserved. Two equivalent forms are supported:

```keln
-- Named form: bind one parameter at a time
let boundFetch = fetchUser.with(db: myDb)

-- Record form: bind multiple parameters at once
let boundWorker = workerLoop.with({
    job_ch:   job_ch,
    retry_ch: retry_ch,
    handler:  handler,
    policy:   policy,
    store:    store
})
```

The record form `fn.with({ key: val, ... })` is equivalent to chaining multiple
named `.with` calls. Both forms produce a fully typed `FunctionRef` with effects
preserved. The record form is preferred when binding three or more parameters.

### 4.5 Custom Effects

```keln
effect Database {
    query:       IO TypeRef                             -> Result<List<TypeRef>, DbError>
    execute:     IO String                              -> Result<Unit, DbError>
    transaction: IO FunctionRef<IO, Unit, Result<T, E>> -> Result<T, E>
}
```

### 4.6 Clock Effect

Time is an explicit dependency. Functions that read the clock declare `Clock`.
The `Clock` module is mockable in `verify` blocks.

**Validated:** All three job queue exercises used Clock mocking for backoff
scheduling, retry timing, timeout races, and lease expiry. The pattern held
in all cases including the distributed lease protocol.

```keln
trusted module Clock {
    provides: {
        now:   Clock Unit     -> Timestamp
        since: Pure Timestamp -> Duration
        after: Pure Duration  -> Timestamp
        sleep: IO Duration    -> Unit
    }
    reason: "system clock; deterministic in verify blocks via mock"
}
```

---

## 5. Functions

### 5.1 Uniform Declaration

```keln
fn <n> {
    <effects> <input_type> -> <output_type>

    in:         <binding>
    out:        <expression | do-block>

    confidence: <Confidence | auto>
    reason:     <NonEmptyString>
    proves:     { <property_list> }
    provenance: { <provenance_block> }
    verify:     { <verify_block> }

    helpers: {
        <n> :: <effects> <In> -> <Out> => <expression>
        fn <n> { ... }
    }
}
```

### 5.2 The `do` Block — Effect Sequencing

Sequences effectful `Unit`-returning operations before a final result expression.
Non-final expressions must return `Unit` — compiler enforced.

```keln
out: do {
    Clock.sleep(timeout)
    Result.err(JobError.Timeout { after: timeout })
}
```

`do` blocks are not loops. They are strict left-to-right sequencing of effects
with a final value. A `do` block whose final expression has type `Never` also
has type `Never` — this is the idiomatic form for tail-recursive event loops.

```keln
out: do {
    select { ... }   -- Unit
    workerLoop(ctx)  -- Never (tail call)
}
-- type of this do block: Never
```

### 5.3 No Lambdas (Tenet 3)

Anonymous callables are incompatible with the verification and learning loop.
See section 1.1.

### 5.4 Helper Functions

Named, scoped to their parent, can carry `verify` blocks. Invisible outside
parent. Two forms: compact (single expression, auto confidence) and full.

Compact helpers expose their input as the implicit binding `it`. Full helpers
declare their own `in:` clause as normal.

```keln
helpers: {
    trimName :: Pure User -> User => User.setName(it, String.trim(it.name))

    fn validateEmail {
        Pure User -> Result<User, ValidationError>
        in:  u
        out: match Email.validate(u.email) {
            Ok(_)  -> Result.ok(u)
            Err(e) -> Result.err(ValidationError.InvalidEmail { value: u.email })
        }
        confidence: auto
    }
}
```

Each compact helper carries its own declared `effects`, `input_type`, and
`output_type` — independent of the parent function's signature.

`promote: threshold(N)` hints for toolchain-driven promotion.
Deep nesting (4+ levels) is the signal to extract named functions and use
`Result.bind` pipelines.

### 5.5 Proof Annotations (Advisory)

`proves` blocks declare intended postconditions. Advisory — do not block
compilation. The `verify` block is the operative correctness check.

### 5.6 Pipeline Operator

`|>` passes the left-hand value as the sole input to the right-hand named
function reference. Left to right. Effect is union of all steps.

---

## 6. Pattern Matching

The only branching mechanism. All match expressions must be exhaustive.
No `if`/`else`, `when`, or `unless`.

```keln
match fetchUser(id) {
    Ok(user)                  -> processUser(user)
    Err(NotFound { id })      -> buildErrorResponse(404, id)
    Err(Timeout { after })    -> buildErrorResponse(504, "timed out")
    Err(Constraint { field }) -> buildErrorResponse(400, field)
}
```

---

## 7. Concurrency

### 7.1 Tasks

```keln
fn fetchAllUsers {
    IO List<UserId> -> Result<List<User>, DbError>
    in:  ids
    out: ids
        |> List.map(spawnFetchUser)
        |> Task.awaitAll
        |> Result.sequence
    confidence: auto
}
```

### 7.2 Channels

**`Channel.new<T>()` syntax:**

Channels are created with `Channel.new<T>()`. The empty parentheses are required
— `Channel.new<T>` without parentheses is a type reference, not a value.

```keln
let job_ch   = Channel.new<JobMessage>()
let retry_ch = Channel.new<RetryMessage>()
let state_ch = Channel.new<{ job_id: JobId, new_state: JobState }>()
```

```keln
ch <- value                  -- ownership transferred; binding invalidated
let received = <-ch          -- blocks until available
let s1, s2 = clone(value)    -- fan-out
select {
    job  = <-jobCh  -> handleJob(job)
    _    = <-stopCh -> Result.ok(Unit)
}
```

### 7.3 Concurrency Verification Scope

Verified: `Pure` and `IO`-with-mocks.
Not verified: `select` ordering, task interleaving, channel timing.
Guaranteed: no data races (ownership system, enforced structurally).
Not guaranteed: ordering, liveness, deadlock freedom.
`VerificationResult.concurrency_not_verified` lists affected functions.
Path to full verification: deterministic scheduler replay in Phase 5.

### 7.4 Tail Recursion as Event Loops

The idiomatic pattern for indefinitely-running processes. TCO makes it correct.
Return type is `Never`.

```keln
fn workerLoop {
    IO & Clock { ... } -> Never
    in:  ctx
    out: do {
        select {
            job_msg   = <-ctx.job_ch   -> handleIncomingJob(job_msg, ctx)
            retry_msg = <-ctx.retry_ch -> handleRetry(retry_msg, ctx)
        }
        workerLoop(ctx)    -- direct tail call; TCO required
    }
    confidence: auto
    reason: "infinite loop; concurrency_not_verified"
}
```

### 7.5 Fire-and-Forget + Feedback Channel Pattern

Discard the `Task<T>` handle. Workers communicate results via state channels.
Worker failures are captured as typed state transitions — more informative than
raw task completion status. This is not a loss of error visibility — it is a
deliberate architectural decision.

### 7.6 Idiomatic Server Pattern

```keln
fn main {
    IO Unit -> Result<Unit, AppError>
    in:  _
    out: match getPort(Unit) {
        Ok(port) ->
            let requestCh  = Channel.new<Request>()
            let responseCh = Channel.new<Response>()
            Task.spawn(HttpListener.run(port: port, out: requestCh))
            Task.spawn(Router.run(in: requestCh, out: responseCh))
            Task.spawn(HttpWriter.run(in: responseCh))
            Task.awaitAll
        Err(e) -> Result.err(AppError.StartupFailed { cause: e })
    }
    confidence: auto
}
```

---

## 8. Modules

### 8.1 Modules as Typed Contracts

```keln
module Database {
    requires: { connection: Connection, timeout: Duration where milliseconds > 0 }
    provides: {
        query:       IO TypeRef                             -> Result<List<TypeRef>, DbError>
        execute:     IO String                              -> Result<Unit, DbError>
        transaction: IO FunctionRef<IO, Unit, Result<T, E>> -> Result<T, E>
    }
}
```

**Lease-aware store module (from validation exercise):**

```keln
module JobStore {
    requires: { connection: Connection }
    provides: {
        insert:       IO JobState          -> Result<Unit, DbError>
        update:       IO JobState          -> Result<Unit, DbError>
        claim:        IO { now: Timestamp, worker_id: WorkerId,
                           lease_duration: Duration }
                                           -> Result<Maybe<JobState>, DbError>
        extendLease:  IO { job_id: JobId, worker_id: WorkerId,
                           new_until: Timestamp }
                                           -> Result<Unit, DbError>
        fetchReady:   IO Unit              -> Result<List<JobState>, DbError>
        fetchRetry:   IO Unit              -> Result<List<JobState>, DbError>
        fetchExpired: IO Timestamp         -> Result<List<JobState>, DbError>
    }
}
```

### 8.2 Module Semantics

Values, immutable after instantiation. Not comparable. Channel-serializable
only if declared Cloneable. Instantiation is `IO`, returns `Result<Module, E>`.
Resource lifecycle is the author's explicit responsibility. Dependency injection
always via explicit parameters. Mutability inside modules is encapsulated;
`trusted` makes this boundary explicit.

### 8.3 Trusted Modules

`trusted` skips `verify` block requirements. `reason` is required. Trusted
declarations carry an implicit confidence reduction in program-level aggregation.

**Why trusted modules are a risk boundary:**

`trusted` modules are the data entry points of a Keln program — JSON parsing,
HTTP body reading, environment variable decoding. A bug in the underlying Rust
implementation bypasses everything Keln guarantees at the type level. The
`trusted` keyword acknowledges this boundary honestly rather than hiding it.

To address this risk, `trusted` modules may declare a `fuzz` block specifying
what properties the underlying Rust implementation must satisfy when fed
arbitrary inputs. The fuzz block is checked by the Phase 3 fuzzing harness —
not the Keln verifier — and its results appear in `VerificationResult.fuzz_status`.

```keln
trusted module JSON {
    provides: {
        parse:     Pure Bytes, TypeRef -> Result<TypeRef, ParseError>
        serialize: Pure T              -> Bytes
    }
    reason: "correctness guaranteed by external test suite and fuzzing"
    fuzz: {
        parse:     inputs(Bytes) -> returns_result
        serialize: inputs(T)     -> crashes_never
    }
}

trusted module HttpServer {
    provides: {
        start: IO { port: Port, router: Router } -> Result<Unit, HttpError>
    }
    reason: "HTTP stack; correctness guaranteed by integration tests"
    fuzz: {
        start: inputs({ port: Port, router: Router }) -> returns_result
    }
}
```

**Fuzz invariants:**

```
crashes_never    -- implementation must not panic, segfault, or produce UB
                 -- on any input of the declared types
returns_result   -- implementation must return Result<T,E> for all inputs;
                 -- may return Err but must not crash
deterministic    -- implementation must return identical output for identical
                 -- input; appropriate for pure functions like JSON.serialize
```

The `fuzz` block is optional. A `trusted` module without one is legal — the
toolchain emits a `FuzzCoverageWarning` in `VerificationResult`, signalling
that the trusted boundary has no automated safety net. `FuzzCoverageWarning`
does not block `is_clean`.

The fuzzer uses the same stratified sampling infrastructure as `forall` bounded
checking: boundary values, midpoints, then random sampling within the declared
type's constraints. The same iteration budget and timeout settings apply.

---

## 9. Confidence and Provenance System

### 9.1 Structured Confidence

```keln
type Confidence = {
    value:             Probability
    variance:          Float where >= 0.0
    sources:           List<ConfidenceSource>
    low_risk_outliers: List<DependencyRisk>
}
```

Auto-derivation: weighted average of verify coverage (0.5), pattern history
(0.3), dependency score (0.2). Low-confidence dependencies surfaced in
`low_risk_outliers` without collapsing the aggregate.

### 9.2 Provenance and PatternDB

Canonical `PatternId` (format: `category.subcategory.operation`) + structural
fingerprints (`effect_signature`, `ast_shape`, `call_graph`). PatternDB
clustering by fingerprint is resilient to label drift. Weighted failure scoring
with severity weights (RuntimeFailed: 1.0, CompiledThenFailed: 0.7,
VerifyFailed: 0.5) and exponential recency decay.

---

## 10. Verification System

### 10.1 Verification Loop

```
generate fn + verify → type check → effect check → constraint check
→ execute: given cases + forall properties + coverage analysis
→ VerificationResult (JSON)
→ is_clean? → yes: proceed | no: return to AI → correct → recompile
```

### 10.2 Verify Block — Pure Functions

```keln
verify: {
    given("8080")  -> Ok(8080)
    given("65536") -> Err(PortError.OutOfRange { value: 65536 })

    forall(n: Int where 1..65535) ->
        parsePort(Int.toString(n)) == Ok(n)
}
```

### 10.3 Verify Block — IO and Clock Functions

```keln
verify: {
    mock Clock { now() -> Timestamp { epoch_ms: 1000 } }
    given(Running { ... }) -> Ok(Retrying { ... })
}
```

### 10.4 Verify Block — FunctionRef Mocking

```keln
verify: {
    mock Clock { now() -> Timestamp { epoch_ms: 1000 } }

    mock handler {
        call(Bytes.empty()) -> Ok(Bytes.fromString("done"))
    }
    given({ state: Running { ... }, handler: _, policy: defaultPolicy })
        -> Completed { ... }

    mock handler {
        call(_) -> Err(JobError.ExecutionFailed { message: "fail" })
    }
    given({ state: Running { ... }, handler: _, policy: defaultPolicy })
        -> Retrying { ... }
}
```

**Rules:**
- `mock <param_name>` refers to a `FunctionRef` parameter by its binding name
- `call(<pattern>)`: input pattern; first match wins; `_` is wildcard catch-all
- Return value must match the `FunctionRef`'s declared output type — compile
  error if not
- Mock scope: applies to all `given` cases from its declaration until the next
  `mock` for the same name or end of the verify block
- In `given` cases, use `_` as the value for mocked `FunctionRef` parameters

### 10.5 forall — Logical Operators

`not`, `and`, `or`, `implies` — scoped exclusively to `forall` and `proves`
property expressions. Not available in `out` expressions or general code.

```keln
forall(a: Int where 1..5, b: Int where 1..5, p: RetryPolicy) ->
    implies(
        a < b,
        computeBackoff({ policy: p, attempt: a }).milliseconds
            <= computeBackoff({ policy: p, attempt: b }).milliseconds
    )
```

`implies(p, q)` short-circuits: if `p` is false, returns true immediately.

### 10.6 forall Execution Model

Deterministic stratified sampling: boundary → midpoint → stratified random →
pure random. Per-dimension sampling for product types. Budget: 1000 iterations,
5000ms timeout. Result types: `Passed { coverage: Bounded }`, `Failed`, `Timeout`.

"Does not crash" form: omit `==` comparison to check expression evaluates
successfully for all inputs.

### 10.7 Structured VerificationResult

```keln
type VerificationResult = {
    compile_errors:           List<CompileError>
    test_failures:            List<TestFailure>
    coverage_gaps:            List<CoverageSuggestion>
    proof_violations:         List<ProofViolation>
    promotion_suggestions:    List<PromotionSuggestion>
    concurrency_not_verified: List<FunctionName>
    fuzz_status:              List<FuzzResult>
    program_confidence:       Confidence
    is_clean:                 Bool
}
-- is_clean: true iff compile_errors, test_failures, proof_violations(Failed) empty
-- fuzz_status, coverage_gaps, promotion_suggestions,
-- concurrency_not_verified: informational — do not block is_clean

type FuzzResult =
    | FuzzPassed      { module: ModuleName, fn_name: FunctionName,
                        iterations: Int, invariant: FuzzInvariant }
    | FuzzFailed      { module: ModuleName, fn_name: FunctionName,
                        input: Value, violation: FuzzInvariant }
    | FuzzNotDeclared { module: ModuleName }
    -- trusted module has no fuzz block; FuzzCoverageWarning (informational only)

type FuzzInvariant = CrashesNever | ReturnsResult | Deterministic
```

---

## 11. PatternDB — Learning from Failures

PatternDB grounds `confidence: auto` in empirical data. Canonical `PatternId`
+ structural fingerprints + weighted failure scoring. See v0.8 for full
specification — no changes in v0.9.

---

## 12. Standard Library — Core Combinators

### 12.1 Result Combinators

```keln
Result.ok       { Pure T                                           -> Result<T, E>      }
Result.err      { Pure E                                           -> Result<T, E>      }
Result.map      { Result<T, E>, FunctionRef<F, T, U>               -> Result<U, E>
                  | effect F                                                            }
Result.bind     { Result<T, E>, FunctionRef<F, T, Result<U, E>>    -> Result<U, E>
                  | effect F                                                            }
Result.mapErr   { Pure Result<T, E1>, FunctionRef<Pure, E1, E2>    -> Result<T, E2>   }
Result.sequence { Pure List<Result<T, E>>                          -> Result<List<T>, E>}
Result.unwrapOr { Pure Result<T, E>, T                             -> T               }
```

### 12.2 Maybe Combinators

```keln
Maybe.some      { Pure T                                           -> Maybe<T>         }
Maybe.none      { Pure Unit                                        -> Maybe<T>         }
Maybe.map       { Maybe<T>, FunctionRef<E, T, U>                   -> Maybe<U>
                  | effect E                                                           }
Maybe.bind      { Maybe<T>, FunctionRef<E, T, Maybe<U>>            -> Maybe<U>
                  | effect E                                                           }
Maybe.require   { Pure Maybe<T>, E                                 -> Result<T, E>    }
Maybe.unwrapOr  { Pure Maybe<T>, T                                 -> T              }
```

`Maybe.none(Unit)` is the canonical call form — `Unit` is passed explicitly.

### 12.3 List Combinators

```keln
List.map        { List<T>, FunctionRef<E, T, U>         -> List<U>          | effect E }
List.filter     { List<T>, FunctionRef<E, T, Bool>      -> List<T>          | effect E }
List.fold       { List<T>, U, FunctionRef<E, {U, T}, U> -> U                | effect E }
List.find       { List<T>, FunctionRef<E, T, Bool>      -> Maybe<T>         | effect E }
List.sequence   { Pure List<Result<T, E>>               -> Result<List<T>, E>          }
List.head       { Pure List<T>                          -> Maybe<T>                    }
List.tail       { Pure List<T>                          -> List<T>                     }
List.isEmpty    { Pure List<T>                          -> Bool                        }
List.length     { Pure List<T>                          -> Int                         }
List.clone      { Pure List<T> where T: Cloneable       -> List<T>                    }
List.range      { Pure Int, Int                         -> List<Int>                   }
List.repeat     { Pure T, Int where >= 0                -> List<T>                     }
```

### 12.4 Task Combinators

```keln
Task.spawn      { IO FunctionRef<IO, Unit, T>  -> Task<T>  }
Task.awaitAll   { IO List<Task<T>>             -> List<T>  }
Task.awaitFirst { IO List<Task<T>>             -> T        }
Task.race       { IO List<Task<T>>             -> T        }
```

All tasks in a race must return the same type.

### 12.5 Time, Clock, and Ordering

**`Timestamp.sub` safety semantics:**

`Timestamp.sub` is an **unchecked subtraction** — it does not validate that
the result is non-negative. Callers are responsible for ensuring the operand
order is correct. The canonical defensive pattern is:

```keln
-- SAFE: guard before subtracting
let safe_delay =
    match Timestamp.gte(r.retry_after, Clock.now()) {
        true  -> Timestamp.sub(r.retry_after, Clock.now())
        false -> Duration.ms(0)
    }

-- UNSAFE: may produce unexpected behavior if now > retry_after
let delay = Timestamp.sub(r.retry_after, Clock.now())
```

This design is intentional. Keln does not silently wrap unsafe arithmetic into
`Result` — the caller's guard is explicit and visible, which is consistent with
the language's preference for explicit over implicit behavior. The compiler
emits a `CoverageSuggestion` when it detects an unguarded `Timestamp.sub` call
whose operand ordering cannot be statically verified.

```keln
type Ordering = LessThan | Equal | GreaterThan

-- Duration constructors and arithmetic
Duration.ms      { Pure Int where >= 0     -> Duration          }
Duration.seconds { Pure Int where >= 0     -> Duration          }
Duration.minutes { Pure Int where >= 0     -> Duration          }
Duration.add     { Pure Duration, Duration -> Duration          }
Duration.multiply { Pure Duration, Int where >= 0 -> Duration  }

-- Timestamp arithmetic and comparison
Timestamp.add    { Pure Timestamp, Duration  -> Timestamp       }
Timestamp.sub    { Pure Timestamp, Timestamp -> Duration        }
-- sub is unchecked: caller must ensure first arg >= second arg
Timestamp.compare { Pure Timestamp, Timestamp -> Ordering      }
Timestamp.gte    { Pure Timestamp, Timestamp -> Bool            }
Timestamp.lte    { Pure Timestamp, Timestamp -> Bool            }
Timestamp.gt     { Pure Timestamp, Timestamp -> Bool            }
Timestamp.lt     { Pure Timestamp, Timestamp -> Bool            }
Timestamp.eq     { Pure Timestamp, Timestamp -> Bool            }

-- Clock
Clock.now        { Clock Unit     -> Timestamp  }
Clock.since      { Pure Timestamp -> Duration   }
Clock.after      { Pure Duration  -> Timestamp  }
Clock.sleep      { IO   Duration  -> Unit       }
```

### 12.6 Float Numeric Operations

```keln
Float.add      { Pure Float, Float              -> Float          }
Float.sub      { Pure Float, Float              -> Float          }
Float.multiply { Pure Float, Float              -> Float          }
Float.divide   { Pure Float, Float where != 0.0 -> Float          }
Float.pow      { Pure Float, Float              -> Float          }
Float.abs      { Pure Float                     -> Float          }
Float.floor    { Pure Float                     -> Float          }
Float.ceil     { Pure Float                     -> Float          }
Float.round    { Pure Float                     -> Float          }
Float.toInt    { Pure Float                     -> Int            }
-- toInt truncates toward zero; compose with floor/ceil for other rounding
Float.fromInt  { Pure Int                       -> Float          }
Float.compare  { Pure Float, Float              -> Ordering       }
Float.approxEq { Pure Float, Float, Float       -> Bool           }
-- approxEq(a, b, epsilon) = |a - b| < epsilon
```

### 12.7 Logging

```keln
trusted module Log {
    provides: {
        debug: Log NonEmptyString -> Unit
        info:  Log NonEmptyString -> Unit
        warn:  Log NonEmptyString -> Unit
        error: Log NonEmptyString -> Unit
    }
    reason: "logging output; structured in production"
}
```

### 12.8 String, Bytes, Int, and IO

```keln
String.trim      { Pure String               -> String         }
String.lowercase { Pure String               -> String         }
String.uppercase { Pure String               -> String         }
String.split     { Pure String, String       -> List<String>   }
String.join      { Pure List<String>, String -> String         }
String.length    { Pure String               -> Int            }
String.contains  { Pure String, String       -> Bool           }
String.toString  { Pure T                    -> String         }

Int.min          { Pure Int, Int             -> Int            }
Int.max          { Pure Int, Int             -> Int            }
Int.pow          { Pure Int, Int where >= 0  -> Int            }
Int.toString     { Pure Int                  -> String         }
Int.toFloat      { Pure Int                  -> Float          }
Int.abs          { Pure Int                  -> Int            }

Bytes.empty      { Pure Unit                 -> Bytes          }
Bytes.fromString { Pure String               -> Bytes          }
Bytes.length     { Pure Bytes                -> Int            }

Env.get          { IO   String               -> Maybe<String>  }
Env.require      { IO   String               -> Result<String, EnvError> }
JSON.parse       { Pure Bytes, TypeRef       -> Result<TypeRef, ParseError> }
JSON.serialize   { Pure T                    -> Bytes          }
HttpServer.start { IO { port: Port, router: Router } -> Result<Unit, HttpError> }
Response.json    { Pure Int, T               -> Response       }
Response.err     { Pure Int, E               -> Response       }
```

---

## 13. What Does Not Exist in Keln

| Absent | Replacement | Why |
|---|---|---|
| `null` | `Maybe<T>` | Untyped absence; Maybe forces typed handling |
| Exceptions | `Result<T, E>` | Invisible control flow; Result is explicit |
| String errors | Typed error sum types | Strings are unexhaustable; sum types are |
| `Fail` effect | `Result<T, E>` return type | Effects and return values are orthogonal |
| Lambdas | Named fns + helpers | Anonymous callables break traceability; see 1.1 |
| Inheritance | Structural types + modules | Implicit behavior; composition is explicit |
| Implicit coercion | Named conversion functions | Hides type mismatches |
| Operator overloading | Fixed operators | `+` always means Int addition |
| Global mutable state | Explicit parameters | Makes local reasoning impossible |
| Undefined behavior | Compile error or Result | Every input produces a defined output |
| Free-text comments | `reason:` fields | Prose is not queryable |
| `if`/`else` | Exhaustive match on Bool | One branching mechanism; see 1.1 |
| Loops | List combinators + tail recursion | Pipelines are transformational |
| Lazy evaluation | Strict + explicit Task | No hidden deferred computation |
| Human-only errors | `VerificationResult` | Compiler is an API; see 1.1 |
| Separate test files | `verify` blocks | Separation creates sync burden |
| SMT solver | Refinement types + bounded forall | SMT is slow and unpredictable |
| Untyped FunctionRef | `FunctionRef<E,In,Out>` | Effect leakage |
| Scalar confidence | `Confidence { value, variance, sources }` | Scalar collapses uncertainty |
| String pattern names | Canonical `PatternId` + fingerprint | Strings drift; structures don't |
| Implicit clone | Explicit `clone()` | Hidden copies violate ownership reasoning |
| Module equality | Not defined | Equality on stateful resources is not meaningful |
| Implicit resource cleanup | Explicit lifecycle management | Forced honesty |
| Combinator variants (mapIO etc.) | Effect subtyping | Subtyping eliminates explosion |
| Dot-access on sum type values | Explicit match or accessor functions | Unsound without variant knowledge |
| Unspecified Float equality | `Float.approxEq` for properties | IEEE 754 NaN behavior |
| General logical operators | `not`/`and`/`or`/`implies` in forall/proves only | Scope-limited |
| Checked `Timestamp.sub` | Caller guards with `Timestamp.gte` | Explicit over implicit |
| `Channel.new<T>` without `()` | `Channel.new<T>()` | Type reference vs value disambiguation |

---

## 14. Complete Example — GraphQL Application Server

*(Unchanged from v0.7/v0.8. The job queue validation exercises are the
primary concrete illustration of the language at production complexity.)*

---

## 15. Validation Exercise Results — Cumulative

**Exercise 1 (our own, v0.6):** 13 issues, resolved in v0.7.

**Exercise 2 (second independent AI, v0.7):** 6 issues, resolved in v0.8.

**Exercise 3 (third independent AI, v0.8):** Extended to distributed
lease-based durable queue. 6 minor gaps resolved in v0.9.

**What all three exercises independently confirmed holds:**
- State machine typing with typed sum types and exhaustive transitions ✓
- Typed errors (`Result<T, E>`) compose correctly across boundaries ✓
- Effect subtyping — single `List.map`; no `mapIO` needed ✓
- Clock mocking — time-dependent functions fully verifiable ✓
- Ownership + clone — fan-out patterns correct ✓
- `FunctionRef<E, In, Out>` — handler types work cleanly ✓
- `verify` blocks — cover transition functions with meaningful cases ✓
- `confidence: auto` — meaningful across multi-function programs ✓
- `do` blocks — essential for effect sequencing in worker loops ✓
- Tail recursion + TCO — event loop pattern is correct and idiomatic ✓
- Fire-and-forget + feedback channel — worker pool pattern works cleanly ✓
- Distributed boundary explicit in types (Tenet 7 consequence) ✓

**What v0.9 resolves:**
- `Never` as function return type for non-terminating tail-recursive functions
- `Timestamp.sub` unchecked semantics with required caller guard pattern
- `Channel.new<T>()` syntax standardized with parentheses
- `.with` record form `fn.with({ key: val, ... })` specified
- `lease_until: Timestamp` added to canonical `Running` variant
- Distributed boundary insight documented as consequence of Tenet 7
- `LeaseError` added to domain error types
- `JobStore` module with lease operations added to module examples

**What remains open:**
- Issue #1 (from first exercise): variant tags as values — parallel tag enum
  is idiomatic; `variant` intrinsic deferred to post-implementation

---

## 16. Build Roadmap

### Phase 1 — Specification (current)
- [x] All items from v0.8 roadmap
- [x] `Never` as function return type for infinite loops
- [x] `Timestamp.sub` safety semantics documented
- [x] `Channel.new<T>()` syntax standardized
- [x] `.with` record form specified
- [x] Canonical `JobState` updated with `lease_until`
- [x] Distributed boundary insight documented
- [x] Three independent validation exercises complete
- [x] `fuzz` block for trusted modules — `FuzzResult`, `FuzzInvariant` types
- [x] `fuzz_status` added to `VerificationResult`
- [x] Formal grammar (EBNF) — complete (keln-grammar-v0.9.ebnf)
- [x] Parser precedence and associativity formally specified (grammar section 19)
- [x] Trusted module fuzz grammar (grammar section 20)
- [x] Effect system formal semantics document (keln-effects-formal-v0.9.md)
- [x] Refinement constraint evaluator specification (keln-refinements-v0.9.md)
- [x] forall sampling implementation specification (keln-forall-sampling-v0.9.md)

### Phase 2 — Tree-walking interpreter (Rust)
- [x] Lexer (9 tests)
- [x] Parser → typed AST (`do` blocks, `Never` return type, forall operators) (15 tests)
- [x] Type checker (FunctionRef, Cloneable, Result<T,E>, Eq derivation,
       `Never` type checking, variant field access) (23 tests)
- [x] Effect checker
- [x] Tree-walking evaluator with TCO (trampolining) (23 tests)
- [x] `verify` executor: given + forall + FunctionRef mocking (13 tests)
- [x] `VerificationResult` emitter (JSON)
- [x] Clock effect and mock support (module mock dispatch)
- [x] Channel and task primitives; `Channel.new<T>()`, `Task.spawn` FunctionRef invocation (sync model)
- [x] Refinement constraint evaluator (runtime enforcement): Range, Comparison, Length at construction time (8 tests)
- [x] Helper function scoping: compact helpers use own declared signature; implicit input binding `it` (4 tests)
- [x] Clone operation: `clone(expr)` returns value; `Value::Duration`/`Value::Timestamp` added (2 tests)
- [x] Structural fingerprint computation: `effect_signature`, `ast_shape`, `call_graph` over `FnDecl` (5 tests)
- [x] Log, Float, Timestamp arithmetic modules: Log.{debug,info,warn,error}, Float complete, Int.{toFloat,pow}, Duration, Timestamp, Clock.{now,since,after,sleep} (22 tests)

### Phase 3 — Standard library (Rust)
- [ ] Result<T,E>, Maybe
- [ ] List (range, repeat, all combinators)
- [ ] Map, Set
- [ ] Task, Channel
- [ ] String, Bytes, Int, Float (complete)
- [ ] Duration, Timestamp (complete arithmetic, unchecked sub), Clock
- [ ] Ordering type
- [ ] Log module
- [ ] JSON, HTTP (trusted) — with fuzz harness
- [ ] GraphQL execution engine (trusted) — with fuzz harness
- [ ] Env and configuration
- [ ] Domain error types: DbError, HttpError, EnvError, ParseError,
       PortError, JobError, QueueError, WorkerError, RetryError, LeaseError
- [ ] Fuzz harness: stratified sampler feeding trusted module `fuzz` blocks
- [ ] `FuzzResult` emitter integrated into `VerificationResult`

### Phase 4 — Bytecode VM
- [ ] Lower typed AST to bytecode IR
- [ ] Bytecode interpreter with TCO (loop rewrite at IR level)
- [ ] Work-stealing scheduler
- [ ] Binary output (standalone executable)

### Phase 5 — AI Toolchain + Advanced Verification
- [ ] PatternDB: emitter, persistence, canonical ID registry, fingerprint clustering
- [ ] Auto-confidence derivation with weighted failure scoring
- [ ] Correction patch applicator
- [ ] Package and module registry
- [ ] Docker base image
- [ ] Concurrency verification: deterministic scheduler replay (research item)

---

*Keln specification v0.9 — A language designed by AI, for AI.*
*Three independent validation exercises complete. Formal EBNF grammar complete.*
*Phase 1 complete. Phase 2 functional: lexer, parser, type checker, evaluator, verify executor, VerificationResult JSON, refinement checks, Task.spawn — 101 tests passing.*
