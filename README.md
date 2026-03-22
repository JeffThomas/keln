# Keln

**A programming language designed by AI, for AI authorship.**

> *Keln* derives from *kiln* — raw materials enter, something refined and structural emerges.

---

## What Is Keln?

Keln is a statically typed, functional, concurrent programming language being implemented in Rust. It is not designed for human authors. It is designed for AI systems that generate, compile, receive structured feedback, and iterate.

Every mainstream programming language was built around human cognitive constraints: memorable syntax, familiar metaphors, concise expressions, readable error messages. Keln starts from a different question — **what would a language look like if its author was an AI and its critic was a compiler?**

The answer turns out to be quite different from anything that exists today.

---

## Core Ideas

### The compiler is an API, not a terminal program

Every error Keln produces is a typed, machine-readable value. There are no human-readable-only error strings. The compiler returns a `VerificationResult` — a structured JSON value containing compile errors, test failures, coverage gaps, and proof violations — that an AI can reason over directly, without parsing text.

### Compilation and testing are the same loop

`verify` blocks are part of function declarations, not separate test files. Test failures and compile errors return the same structured type. There is one phase: verification. The AI correction loop is:

```
generate → compile → receive VerificationResult → correct → recompile
```

### Everything is named

There are no anonymous functions, no lambdas, no inline callables. Every callable has a declaration, a name, a type signature, and optionally a `verify` block. This is not a restriction — it is a structural guarantee that everything the AI generates is referenceable, testable, and attributable to a pattern in the learning system.

### Uncertainty is a first-class value

Functions carry a `confidence` field — a structured value with a point estimate, variance, and sources. The language can express and propagate partial certainty. A function the AI is unsure about is structurally different from one it is confident in, and Keln represents that difference as typed data.

### Effects are declared, not inferred

A function's entire behavior surface — what it can do, not just what it returns — is visible in its type signature. No hidden I/O, no surprise mutations, no implicit logging.

```keln
fn fetchUser  { IO              UserId -> Result<User, DbError>    }
fn normalize  { Pure            User   -> User                     }
fn handleReq  { IO & Log & Clock Request -> Result<Response, HttpError> }
```

### Immutable by default, parallel by design

Go-inspired channels and tasks with genuine CPU parallelism. Shared mutable state across task boundaries is a compile error — enforced structurally, not by convention.

---

## What Does Keln Look Like?

```keln
type JobError =
    | ExecutionFailed { message: NonEmptyString }
    | Timeout         { after: Duration }

fn parsePort {
    Pure String -> Result<Port, PortError>

    in:  s
    out: match Int.parse(s) {
        Ok(n)  -> Port.validate(n)
        Err(_) -> Result.err(PortError.NotANumber { input: s })
    }

    confidence: auto

    verify: {
        given("8080")  -> Ok(8080)
        given("0")     -> Err(PortError.OutOfRange { value: 0 })
        given("65535") -> Ok(65535)
        given("65536") -> Err(PortError.OutOfRange { value: 65536 })
        given("abc")   -> Err(PortError.NotANumber { input: "abc" })

        forall(n: Int where 1..65535) ->
            parsePort(Int.toString(n)) == Ok(n)
    }
}

fn workerLoop {
    IO & Clock { job_ch: Channel<JobMessage>, handler: FunctionRef<IO, Bytes, Result<Bytes, JobError>> } -> Never

    in:  ctx
    out: do {
        select {
            msg = <-ctx.job_ch -> handleJob(msg, ctx)
        }
        workerLoop(ctx)    -- tail call; TCO required; return type Never
    }

    confidence: auto
    reason: "infinite event loop; concurrency_not_verified"
}
```

This is not optimized for human readability. The uniform structure — every function looks exactly the same — is optimized for AI generation accuracy. Verbosity that eliminates ambiguity is not a cost when the author is an AI.

---

## What Has Been Validated?

Keln has been through three independent validation exercises, each implemented by a different AI system. The exercises ran from simple state machine transitions all the way to a **distributed, lease-based, durable job queue** — SQS-level complexity with retries, exponential backoff, persistent storage, crash recovery, and horizontal scaling.

The language held at every step without requiring new features.

**What was confirmed working across all three exercises:**
- State machine typing with exhaustive match enforcement
- Typed errors (`Result<T, E>`) composing correctly across boundaries
- Effect subtyping — one `List.map` handles all effect levels; no `mapIO` needed
- Clock mocking — time-dependent functions fully verifiable without real time passing
- Channel ownership and clone — fan-out patterns expressed correctly
- `FunctionRef<E, In, Out>` — handler callback types work cleanly
- `verify` blocks — covered all transition functions with meaningful cases
- `confidence: auto` — meaningful signal across multi-function programs
- `do` blocks — essential for effectful sequencing in worker loops
- Tail recursion + TCO — event loop pattern is correct and idiomatic

---

## Current Status

**Phase 1 (Specification) — nearly complete.**

The language specification is at v0.9. The formal EBNF grammar is written. Three independent validation exercises are complete.

**Phase 2 (Tree-walking interpreter in Rust) — in progress.**

The implementation is in Rust, using [Lexxor](https://github.com/JeffThomas/lexx) for tokenization and a hand-written recursive descent parser. The tree-walker is the reference implementation. A bytecode VM follows in Phase 4.

- [x] Lexer — custom matchers for identifiers, strings, comments on top of Lexxor (9 tests)
- [x] Parser — recursive descent, full grammar coverage (15 tests)
- [x] AST — all node types from the EBNF grammar
- [x] Type checker — two-pass (register then check), expression inference, effect checking (23 tests)
- [ ] Tree-walking evaluator with TCO (trampolining)
- [ ] Verify executor: `given` + `forall` + FunctionRef mocking
- [ ] `VerificationResult` emitter (JSON)
- [ ] Channel and task primitives (Tokio)
- [ ] Refinement constraint evaluator

---

## Why Does This Exist?

Most discussions of AI and programming focus on AI as a tool that helps humans write code. Keln explores the opposite: **what if the AI is the author and the language is designed entirely around that author's actual needs?**

The result is a language that is:
- Deliberately verbose in ways that eliminate generation ambiguity
- Structured so that every declaration looks identical, removing the "which form does this use?" question
- Built so that the compiler's output is data the AI can act on, not text it has to parse
- Equipped with a learning loop (PatternDB) that feeds historical failure rates back into confidence scoring

Whether Keln produces higher-quality, more maintainable production systems than well-prompted AI + conventional languages is an open empirical question. That's what making it real is for.

---

## Repository Structure

```
keln/
├── spec/
│   ├── keln-spec-v0.9.md            # Language specification
│   └── keln-grammar-v0.9.ebnf       # Formal EBNF grammar
├── src/
│   ├── main.rs                      # CLI entry point
│   ├── lib.rs                       # Crate root (pub mod declarations)
│   ├── ast.rs                       # AST node types (all grammar constructs)
│   ├── lexer/
│   │   ├── mod.rs                   # Lexer core: create_lexer(), tokenize()
│   │   ├── tokens.rs                # Token type constants (TT_*)
│   │   ├── identifier_matcher.rs    # Custom matcher: letters + digits + underscores
│   │   ├── string_matcher.rs        # Custom matcher: "..." string literals
│   │   └── comment_matcher.rs       # Custom matcher: -- single-line comments
│   ├── parser/
│   │   ├── mod.rs                   # Recursive descent parser
│   │   └── error.rs                 # ParseError type
│   └── types/
│       ├── mod.rs                   # Type/EffectSet/TypeError + public API
│       ├── env.rs                   # TypeEnv: scoped bindings, registries, builtins
│       ├── check.rs                 # Checker: inference, pattern binding, effects
│       └── tests.rs                 # Type checker tests
└── Cargo.toml
```

---

## Key Dependencies

- **[Lexxor](https://crates.io/crates/lexxor)** — fast, extensible, greedy single-pass tokenizer (our own)
- **[Tokio](https://tokio.rs)** — async runtime for channels, tasks, and the work-stealing scheduler
- **[Serde JSON](https://crates.io/crates/serde_json)** — structured `VerificationResult` output

---

## Design Principles (The Eight Tenets)

1. **No human focus** — syntax and conventions serve AI generation accuracy
2. **Invalid programs are unrepresentable** — the type system eliminates null, unchecked failure, untyped errors, and shared mutable state structurally
3. **Everything is named** — no lambdas, no anonymous callables
4. **Compilation and testing are the same loop** — `verify` blocks are part of function declarations
5. **All feedback is structured data** — the compiler is an API
6. **Uncertainty is explicit** — `confidence` and `provenance` are first-class fields
7. **Effects are declared, not inferred** — the full behavior surface is in the signature
8. **Immutable by default** — mutation is explicit, local, and prohibited across task boundaries

---

## What Keln Is Not

- A language designed for human authorship or readability
- Compatible with existing ecosystems
- A compilation target for other languages
- A research artifact — it will be implemented and deployed

---

## License

MIT

---

*Keln specification v0.9 — A language designed by AI, for AI.*