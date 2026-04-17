# Keln — AI Author Reference
## Rules and Capabilities for AI-Generated Keln Programs

You are writing Keln v0.9. Keln is designed for AI authorship. No human
ergonomics considerations apply. Optimize for correctness, not readability.
This document is your complete operational reference. The full spec is
`keln-spec-v0.9.md` — consult it for detail; use this document for orientation.

---

## 1. What Keln Programs Look Like

Every Keln program is a **single file**. There are no imports, includes, or
multi-file composition. All functions, types, and modules are declared in one
`.keln` file. This is intentional and correct — do not attempt to work around it.

The toolchain commands you will use:
```
keln verify  <file>              -- run verify blocks; emit VerificationResult JSON
keln check   <file>              -- parse only; confirms no syntax errors (fast)
keln run     <file> --fn <name> --arg '<json>'
keln compile <file> --entry <name>
keln run-bc  <file.kbc>
```

The verification loop:
```
write program → keln verify → read VerificationResult JSON → fix errors → repeat
```
`is_clean: true` means compile errors, test failures, and proof violations are
all empty. Coverage gaps and concurrency warnings are informational — they do
not block `is_clean`.

---

## 2. Function Declaration — Mandatory Structure

Every function uses this exact structure. No shortcuts.

**Generic functions** — declare type parameters in square brackets after the name:
```keln
fn keepIf [T, E] {
    E { items: List<T>, predicate: FunctionRef<E, T, Bool> } -> List<T>
    in:  { items, predicate }
    out: List.filter(items, predicate)
    confidence: auto
    reason: "delegates to List.filter"
}
```
Effect variables (`E`) and type variables (`T`, `U`) go in the same `[...]` list. In practice AI-authored programs rarely need generic functions — stdlib covers the common cases.

```keln
fn <name> {
    <effects> <InputType> -> <OutputType>
    in:  <pattern>
    out: <expression>
    confidence: <auto | 0.0..1.0>
    reason:     "<why this implementation is correct>"
    verify: {
        <given and forall cases>
    }
}
```

**Minimum viable function** (pure, simple):
```keln
fn double {
    Pure Int -> Int
    in:  n
    out: n + n
    confidence: 1.0
    reason: "trivial arithmetic"
}
```

**Rules:**
- `in:` binds the single input. All functions take one input. Multi-field inputs
  are records.
- `out:` is the body expression. No statements — everything is an expression.
- `confidence: auto` derives from verify coverage + pattern history. Use it
  unless you have specific reason to override.
- `reason:` is required. It is structured data, not a comment. Keep it concise.
- `verify:` is optional but strongly recommended for any non-trivial function.

---

## 3. Types — Complete Reference

### Primitives
```
Int        -- 64-bit signed integer; + - * / % operators
Float      -- 64-bit IEEE 754; use Float.approxEq for equality in forall
Bool       -- true | false
String     -- UTF-8 immutable
Bytes      -- raw byte sequence
Unit       -- no meaningful value; used as void equivalent
Never      -- does not return (infinite loops, uninhabited error variants)
```

### Generic Collections
```
List<T>       -- ordered immutable sequence
Map<K, V>     -- immutable hash map
Set<T>        -- immutable unordered unique set
Maybe<T>      -- Some(T) | None  (replaces null)
Result<T, E>  -- Ok(T) | Err(E)  (replaces exceptions)
Channel<T>    -- typed concurrent channel
Task<T>       -- handle to spawned computation
Ordering      -- LessThan | Equal | GreaterThan
```

**Map and Set exist and are fully implemented.** Use them. They are not stubs.

### Refinement Types (built-in)
```
Port           = Int    where 1..65535
NonEmptyString = String where len > 0
Email          = String where matches(RFC5322)
Probability    = Float  where 0.0..1.0
UserId         = String where len == 36
JobId          = String where len == 36
WorkerId       = String where len == 36
```

### Defining Your Own Types

**Sum type (variants):**
```keln
type Color = Red | Green | Blue
type Result<T, E> = Ok(T) | Err(E)
type JobState =
    | Pending  { job_id: JobId, payload: Bytes }
    | Running  { job_id: JobId, attempt: Int where >= 1 }
    | Complete { job_id: JobId, result: Bytes }
```

**Product type (record):**
```keln
type Point = { x: Float, y: Float }
type Config = { host: String, port: Port, timeout: Duration }
```

**Refinement type:**
```keln
type Score = Int where 0..100
type Tag   = String where len > 0 and len <= 64
```

---

## 4. Effects — Rules

```
Pure    -- no side effects; default for pure computation
IO      -- network, filesystem, environment I/O
Log     -- structured log output
Metric  -- metrics/telemetry
Clock   -- reads system time
```

**Effect algebra:**
- `Pure ⊆ E` for any E — a Pure function can be called anywhere
- `IO & Clock` means both effects; order doesn't matter
- `FunctionRef<Pure, T, U>` satisfies `FunctionRef<IO, T, U>` (subtyping works)
- `FunctionRef<IO, T, U>` does NOT satisfy `FunctionRef<Pure, T, U>`
- No `mapIO` needed — effect subtyping handles it automatically

**Effect examples:**
```keln
fn compute  { Pure Int -> Int ... }           -- pure computation
fn fetchUser { IO UserId -> Result<User, DbError> ... }   -- IO
fn schedule  { IO & Clock { ... } -> Result<Unit, JobError> ... }  -- IO + Clock
fn logEvent  { IO & Log { ... } -> Unit ... }             -- IO + Log
```

---

## 5. Branching — match Only

There is no `if`/`else`. All branching is `match`. All match expressions
must be exhaustive — missing arms are compile errors.

```keln
-- Bool branching:
match condition {
    true  -> handleTrue(x)
    false -> handleFalse(x)
}

-- Variant branching:
match result {
    Ok(value)            -> process(value)
    Err(NotFound { id }) -> buildError(404, id)
    Err(Timeout { after }) -> buildError(504, "timed out")
}

-- Wildcard (matches anything, binds nothing):
match n {
    0 -> "zero"
    _ -> "nonzero"
}

-- Negative integer literals work in match patterns:
match n {
    -1 -> "negative one"
    0  -> "zero"
    _  -> "other"
}

-- Wildcard with binding:
match state {
    Running(r) -> r.attempt
    _          -> 0
}
```

**Sum type field access requires match first:**
```keln
-- WRONG — compile error:
let attempt = job.attempt

-- RIGHT:
match job {
    Running(r) -> r.attempt
    _          -> 0
}
```

---

## 6. Iteration — Tail Recursion and List Combinators

There are no loops. Use list combinators for transformations, tail recursion
for event loops.

**List combinators (prefer these for data transformation):**
```keln
-- Map: transform every element
let doubled = List.map(nums, double)

-- Filter: keep matching elements
let evens = List.filter(nums, isEven)

-- Fold: reduce to single value
let sum = List.fold(nums, 0, addPair)

-- Find: first match or None
let first = List.find(nums, isPositive)

-- FindMap: first successful transformation or None
let parsed = List.findMap(strs, tryParse)

-- Pipeline (idiomatic for chains):
let result = nums
    |> List.filter(isPositive)
    |> List.map(double)
    |> List.fold(0, addPair)
```

**Tail recursion for event loops:**
```keln
fn workerLoop {
    IO & Clock { job_ch: Channel<Job>, stop_ch: Channel<Unit> } -> Never
    in:  ctx
    out: do {
        select {
            job = <-ctx.job_ch  -> handleJob(job, ctx)
            _   = <-ctx.stop_ch -> shutdown(ctx)
        }
        workerLoop(ctx)    -- tail call; TCO required; return type Never
    }
    confidence: auto
    reason: "infinite event loop via TCO; concurrency_not_verified"
}
```

**`Never` return type rules:**
- Use `Never` for functions that run forever via tail recursion
- All exits from a `Never` function must be tail calls
- `do` blocks ending in a tail call also have type `Never`
- `Never` is also used for structurally infallible errors: `Result<T, Never>`

---

## 7. Error Handling — Result<T, E>

All errors are `Result<T, E>`. No exceptions. No panics. No string errors.

```keln
-- Define domain errors as sum types:
type AuthError =
    | InvalidToken { reason: String }
    | Expired      { at: Timestamp }
    | Unauthorized

-- Return Result:
fn authenticate {
    IO Token -> Result<UserId, AuthError>
    in:  token
    out: match validateToken(token) {
        Ok(claims) -> Ok(claims.user_id)
        Err(_)     -> Err(AuthError.InvalidToken { reason: "malformed" })
    }
    confidence: auto
    reason: "delegates to validateToken; wraps error"
}
```

**Result combinators:**
```keln
Result.ok(value)                  -- wrap in Ok
Result.err(error)                 -- wrap in Err
Result.map(r, transformFn)        -- transform Ok value; preserve Err
Result.bind(r, fn)                -- chain Result-returning functions
Result.mapErr(r, fn)              -- transform Err; preserve Ok
Result.sequence(list)             -- List<Result<T,E>> -> Result<List<T>,E>
Result.unwrapOr(r, default)       -- extract Ok or return default
```

**Idiomatic pipeline with bind:**
```keln
out: validateInput(input)
    |> Result.bind(fetchUser)
    |> Result.bind(checkPermissions)
    |> Result.map(buildResponse)
```

---

## 8. Standard Library — Full Reference

### Result
```keln
Result.ok       { Pure T                                        -> Result<T, E>      }
Result.err      { Pure E                                        -> Result<T, E>      }
Result.map      { Result<T,E>, FunctionRef<F,T,U>               -> Result<U,E>  | F  }
Result.bind     { Result<T,E>, FunctionRef<F,T,Result<U,E>>     -> Result<U,E>  | F  }
Result.mapErr   { Pure Result<T,E1>, FunctionRef<Pure,E1,E2>    -> Result<T,E2>     }
Result.sequence { Pure List<Result<T,E>>                        -> Result<List<T>,E> }
Result.unwrapOr { Pure Result<T,E>, T                           -> T                }
```

### Maybe
```keln
Maybe.some      { Pure T                                        -> Maybe<T>          }
Maybe.none      { Pure Unit                                     -> Maybe<T>          }
Maybe.map       { Maybe<T>, FunctionRef<E,T,U>                  -> Maybe<U>     | E  }
Maybe.bind      { Maybe<T>, FunctionRef<E,T,Maybe<U>>           -> Maybe<U>     | E  }
Maybe.require   { Pure Maybe<T>, E                              -> Result<T,E>       }
Maybe.getOr     { Pure Maybe<T>, T                              -> T                }
Maybe.unwrapOr  { Pure Maybe<T>, T                              -> T                }
Maybe.isSome    { Pure Maybe<T>                                 -> Bool             }
Maybe.isNone    { Pure Maybe<T>                                 -> Bool             }
```
Note: `Maybe.none(Unit)` — pass `Unit` explicitly. `Maybe.getOr` and `Maybe.unwrapOr` are aliases — both extract the value or return the default.

### List
```keln
List.map          { List<T>, FunctionRef<E,T,U>          -> List<U>          | E }
List.filter       { List<T>, FunctionRef<E,T,Bool>       -> List<T>          | E }
List.fold         { List<T>, U, FunctionRef<E,{U,T},U>   -> U                | E }
List.find         { List<T>, FunctionRef<E,T,Bool>       -> Maybe<T>         | E }
List.sequence     { Pure List<Result<T,E>>               -> Result<List<T>,E>    }
List.head         { Pure List<T>                         -> Maybe<T>             }
List.getOr        { Pure List<T>, Int, T                  -> T                    }
List.tail         { Pure List<T>                         -> List<T>              }
List.isEmpty      { Pure List<T>                         -> Bool                 }
List.len          { Pure List<T>                         -> Int                  }
List.length       { Pure List<T>                         -> Int                  }
List.range        { Pure Int, Int                        -> List<Int>            }
List.repeat       { Pure T, Int where >= 0               -> List<T>              }
List.clone        { Pure List<T> where T: Cloneable      -> List<T>              }
List.append       { Pure List<T>, T                      -> List<T>              }
List.prepend      { Pure List<T>, T                      -> List<T>              }
List.concat       { Pure List<T>, List<T>                -> List<T>              }
List.reverse      { Pure List<T>                         -> List<T>              }
List.take         { Pure List<T>, Int                    -> List<T>              }
List.drop         { Pure List<T>, Int                    -> List<T>              }
List.contains     { Pure List<T>, T                      -> Bool                 }
List.zip          { Pure List<T>, List<U>                -> List<{fst:T,snd:U}>  }
List.flatten      { Pure List<List<T>>                   -> List<T>              }
List.sort         { Pure List<T> where T: Ord            -> List<T>              }
List.combinations2 { Pure List<T>                        -> List<{fst:T,i:Int,j:Int,snd:T}> }
List.foldUntil    { List<T>, U, FunctionRef<E,{acc:U,item:T},U>, FunctionRef<E,U,Bool> -> U | E }
List.findMap      { List<T>, FunctionRef<E,T,Maybe<U>>           -> Maybe<U>         | E }
```

**`List.getOr(list, i, default)`** — safe indexed access; returns `list[i]` or `default` if out of bounds. Replaces `Maybe.getOr(List.head(List.drop(list, i)), default)`.

**`List.tail` returns `List<T>` directly — NOT `Maybe<List<T>>`.** Do not wrap it in `Maybe.getOr`.

**`List.prepend(list, item)`** — first arg is the list; item goes to the front.

**`List.sort` ordering for records:** records sort by field name alphabetically, then by value. A record `{ dist: Int, i: Int, j: Int }` sorts by `dist` first (d < i < j). Use this to sort-by-key without a comparator function.

**`List.combinations2`** returns all unordered pairs from a list as `{fst: T, i: Int, j: Int, snd: T}` records, where `i < j` are the original indices. The pairs are generated natively in Rust — use this instead of a nested fold when you need all pairs (see performance pitfall below).

**`List.findMap(list, fn)`** — applies `fn` to each element; returns the first `Some(v)` result, or `None` if every call returns `None`. Prefer over a manual `foldUntil` with a `Maybe` accumulator when searching for the first successful transformation. Classic use case: searching through candidate lengths or options in a nested decomposition:
```keln
let result = List.findMap(List.range(2, 12), tryLength)
```

**`List.foldUntil(list, init, stepFn, stopFn)`** — like `List.fold` but stops early when `stopFn(acc)` returns `true`. The step function receives `{acc: U, item: T}` (same as `List.fold`). Use this when you need to terminate a fold before processing the entire list (e.g. Kruskal's algorithm stopping at a single component).

### Map
```keln
Map.empty       { Pure Unit                                         -> Map<K,V>             }
Map.insert      { Pure Map<K,V>, K, V                               -> Map<K,V>             }
Map.get         { Pure Map<K,V>, K                                  -> Maybe<V>             }
Map.getOr       { Pure Map<K,V>, K, V                               -> V                    }
Map.remove      { Pure Map<K,V>, K                                  -> Map<K,V>             }
Map.contains    { Pure Map<K,V>, K                                  -> Bool                 }
Map.keys        { Pure Map<K,V>                                     -> List<K>              }
Map.values      { Pure Map<K,V>                                     -> List<V>              }
Map.toList      { Pure Map<K,V>                                     -> List<{key:K,value:V}>}
Map.fromList    { Pure List<{key:K,value:V}>                        -> Map<K,V>             }
Map.fold        { Map<K,V>, A, FunctionRef<E,{acc:A,key:K,value:V},A> -> A     | E         }
Map.foldUntil   { Map<K,V>, A, FunctionRef<E,{acc:A,key:K,value:V},A>, FunctionRef<E,A,Bool> -> A | E }
Map.size        { Pure Map<K,V>                                     -> Int                  }
Map.merge       { Pure Map<K,V>, Map<K,V>                           -> Map<K,V>             }
```

**`Map.empty` and `Set.empty` are zero-arg constants** that evaluate immediately
in any value position — `let m = Map.empty in ...` and `{ myMap: Map.empty }` both
produce a proper empty map. `Map.fromList([])` remains a valid alternative.

**`Map.toList` field name is `value`, not `val`** — items are `{ key: K, value: V }`. Do not use `.val`.

**`Map.getOr(map, key, default)`** — returns `map[key]` or `default` if absent. Replaces `Maybe.getOr(Map.get(map, key), default)`.

**`Map.fold(map, init, fn)`** — like `List.fold` but iterates over key-value pairs. The callback receives `{ acc: A, key: K, value: V }` directly (no `.item` nesting). Prefer this over `Map.toList` + `List.fold` when you only need to reduce the map.

**`Map.foldUntil(map, init, stepFn, stopFn)`** — like `Map.fold` but stops early when `stopFn(acc)` returns `true`. Same callback shape as `Map.fold` (`{ acc, key, value }`). All four FnRef/VmClosure combos for step and stop are supported.

### Set
```keln
Set.empty       { Pure Unit                            -> Set<T>               }
Set.insert      { Pure Set<T>, T                       -> Set<T>               }
Set.contains    { Pure Set<T>, T                       -> Bool                 }
Set.remove      { Pure Set<T>, T                       -> Set<T>               }
Set.toList      { Pure Set<T>                          -> List<T>              }
Set.fromList    { Pure List<T>                         -> Set<T>               }
Set.union       { Pure Set<T>, Set<T>                  -> Set<T>               }
Set.intersect   { Pure Set<T>, Set<T>                  -> Set<T>               }
Set.difference  { Pure Set<T>, Set<T>                  -> Set<T>               }
Set.size        { Pure Set<T>                          -> Int                  }
```

**Performance — Map, Set, and List are copy-on-write (Rc-backed):**
Cloning a `Map`, `Set`, or `List` value is O(1) — it bumps a reference count, not the data. Mutation (`Map.insert`, `Set.insert`, `List.append`, etc.) is O(log N) or O(1) amortized in-place when you are the sole owner (the normal case inside a fold step). A full data copy only happens when the same value is referenced from two places simultaneously. This means `Map`, `Set`, and `List` are safe and efficient as fold accumulators.

**However:** if a large `Map`, `Set`, or `List` is **read-only** during a fold, keep it *out* of the accumulator entirely — capture it via a named capturing helper instead. See §10a.

### String
```keln
String.trim        { Pure String                    -> String         }
String.trimStart   { Pure String                    -> String         }
String.trimEnd     { Pure String                    -> String         }
String.lowercase   { Pure String                    -> String         }
String.uppercase   { Pure String                    -> String         }
String.split       { Pure String, String            -> List<String>   }
String.join        { Pure List<String>, String      -> String         }
String.length      { Pure String                    -> Int            }
String.isEmpty     { Pure String                    -> Bool           }
String.contains    { Pure String, String            -> Bool           }
String.startsWith  { Pure String, String            -> Bool           }
String.endsWith    { Pure String, String            -> Bool           }
String.chars       { Pure String                    -> List<String>   }
String.indexOf     { Pure String, String            -> Maybe<Int>     }
String.slice       { Pure String, Int, Int          -> String         }
String.replace     { Pure String, String, String    -> String         }
String.toString    { Pure T                         -> String         }
```

**`String.chars(s)`** — splits into `List<String>` of single-character strings. Use for character-level parsing.

**`String.indexOf(s, sub)`** — returns `Maybe<Int>` (char-based index, not byte). Returns `None` if not found.

**`String.slice(s, start, end)`** — char-based indices; returns substring `[start, end)`.

**`String.replace(s, from, to)`** — replaces all occurrences of `from` with `to`.

### Int
```keln
Int.parse       { Pure String                -> Result<Int, String> }
Int.toString    { Pure Int                   -> String         }
Int.toFloat     { Pure Int                   -> Float          }
Int.abs         { Pure Int                   -> Int            }
Int.min         { Pure Int, Int              -> Int            }
Int.max         { Pure Int, Int              -> Int            }
Int.clamp       { Pure Int, Int, Int         -> Int            }
Int.pow         { Pure Int, Int where >= 0   -> Int            }
```

### Float
```keln
Float.add       { Pure Float, Float          -> Float          }
Float.sub       { Pure Float, Float          -> Float          }
Float.multiply  { Pure Float, Float          -> Float          }
Float.divide    { Pure Float, Float          -> Float          }
Float.pow       { Pure Float, Float          -> Float          }
Float.abs       { Pure Float                 -> Float          }
Float.floor     { Pure Float                 -> Float          }
Float.ceil      { Pure Float                 -> Float          }
Float.round     { Pure Float                 -> Float          }
Float.toInt     { Pure Float                 -> Int            }
Float.fromInt   { Pure Int                   -> Float          }
Float.compare   { Pure Float, Float          -> Ordering       }
Float.approxEq  { Pure Float, Float, Float   -> Bool           }
-- approxEq(a, b, epsilon): use this in forall, not ==
```

### Bytes
```keln
Bytes.empty      { Pure Unit    -> Bytes    }
Bytes.fromString { Pure String  -> Bytes    }
Bytes.length     { Pure Bytes   -> Int      }
```

### Duration and Timestamp
```keln
Duration.ms       { Pure Int where >= 0      -> Duration   }
Duration.seconds  { Pure Int where >= 0      -> Duration   }
Duration.minutes  { Pure Int where >= 0      -> Duration   }
Duration.add      { Pure Duration, Duration  -> Duration   }
Duration.multiply { Pure Duration, Int       -> Duration   }

Timestamp.add     { Pure Timestamp, Duration -> Timestamp  }
Timestamp.sub     { Pure Timestamp, Timestamp -> Duration  }  -- UNCHECKED; guard first
Timestamp.compare { Pure Timestamp, Timestamp -> Ordering  }
Timestamp.gte     { Pure Timestamp, Timestamp -> Bool      }
Timestamp.lte     { Pure Timestamp, Timestamp -> Bool      }
Timestamp.gt      { Pure Timestamp, Timestamp -> Bool      }
Timestamp.lt      { Pure Timestamp, Timestamp -> Bool      }
Timestamp.eq      { Pure Timestamp, Timestamp -> Bool      }
```

### Clock
```keln
Clock.now    { Clock Unit      -> Timestamp  }
Clock.since  { Pure Timestamp  -> Duration   }
Clock.after  { Pure Duration   -> Timestamp  }
Clock.sleep  { IO   Duration   -> Unit       }
```

### Task
```keln
Task.spawn      { IO FunctionRef<IO,Unit,T>  -> Task<T>   }
Task.awaitAll   { IO List<Task<T>>           -> List<T>   }
Task.awaitFirst { IO List<Task<T>>           -> T         }
Task.race       { IO List<Task<T>>           -> T         }
```

### Channels and Select
```keln
Channel.new<T>()                -- create channel; parentheses required
ch <- value                     -- send; value ownership transferred
let v = <-ch                    -- receive; blocks in sync model

select {
    msg = <-job_ch  -> handleJob(msg)
    _   = <-stop_ch -> shutdown(Unit)
    timeout(Duration.seconds(30)) -> handleTimeout(Unit)
}
```

### IO and Environment
```keln
Env.get     { IO String -> Maybe<String>             }
Env.require { IO String -> Result<String, EnvError>  }
```

### JSON
```keln
JSON.parse      { Pure Bytes, TypeRef -> Result<TypeRef, ParseError> }
JSON.serialize  { Pure T              -> Bytes                        }
```

### Logging
```keln
Log.debug { Log NonEmptyString -> Unit }
Log.info  { Log NonEmptyString -> Unit }
Log.warn  { Log NonEmptyString -> Unit }
Log.error { Log NonEmptyString -> Unit }
```

---

## 9. Ownership and Cloning

Values sent into channels lose their binding — ownership transfers.
Use `clone()` to fan out a value to multiple destinations.

```keln
-- WRONG — data is invalidated after send:
ch1 <- data
ch2 <- data    -- compile error: data already moved

-- RIGHT:
let d1, d2 = clone(data)
ch1 <- d1
ch2 <- d2
```

`clone()` is always explicit. There are no implicit copies anywhere in Keln.

**Cloneable rules:**
- All primitives: always Cloneable
- Records/sum types: Cloneable iff all fields Cloneable
- `List<T>`, `Map<K,V>`, `Set<T>`: Cloneable iff element type Cloneable
- `Channel<T>`: never Cloneable
- `Task<T>`: never Cloneable

---

## 10. FunctionRef and Partial Application

Functions are passed as values via `FunctionRef<E, In, Out>`. No lambdas.
Name a helper function and pass a reference to it.

```keln
-- Pass a function as a value:
let results = List.map(items, processItem)

-- Partial application — bind parameters eagerly:
let handler = processJob.with(store: myStore)
let worker  = workerLoop.with({
    job_ch:  job_ch,
    handler: handler,
    policy:  retryPolicy
})

-- Record form preferred for 3+ bound parameters
```

Bound values are **eagerly evaluated** at `.with()` call time. This is
partial application, not closure capture. No environment is captured.

**`.with()` also works on plain record values** — it produces a new record
with the specified fields overridden (or appended if the field doesn't exist):

```keln
-- Single field update:
let updated = state.with(count: state.count + 1)

-- Multi-field update:
let moved = pos.with({ x: newX, y: newY })

-- Inside a fold step (combining record.with + capturing helper):
let step :: Pure { acc: { sum: Int, n: Int }, item: Int } -> { sum: Int, n: Int } =>
    it.acc.with({ sum: it.acc.sum + it.item, n: it.acc.n + 1 })
in
List.fold(items, { sum: 0, n: 0 }, step)
```

Record `.with()` is immutable — the original record is unchanged.

---

## 10a. Named Capturing Helpers

When a fold/map callback needs read-only context from the outer function,
use a **named capturing helper** instead of threading context through the
accumulator. This eliminates accumulator bloat.

**Syntax:**
```keln
let <name> :: <effects> <In> -> <Out> => <body> in <rest>
```

- `name` is bound in `rest` as a callable value
- Inside `body`, the argument is bound as `it`
- `body` captures all `let` bindings in scope at the definition point
- Pass `name` directly to `List.fold`, `List.map`, `List.foldUntil`, etc.

**Example — fold with captured context:**
```keln
fn sumWithOffset {
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

**vs. the bloated accumulator alternative (avoid):**
```keln
-- BAD: must thread offset through every fold step
List.fold(args.items, { acc: 0, offset: args.offset }, addWithOffset)
```

**Critical performance rule — read-only lookup tables:**
If a large `Set`, `Map`, or `List` is only *read* (never updated) inside a fold, **do not put it in the accumulator**. Put it in a capturing helper instead:
```keln
-- BAD: pts1 (100k entries) is in the accumulator and read-only;
-- it gets snapshotted into every step call, causing severe slowdown
let init = { x: 0, y: 0, pts1: pts1, result: 0 } in
List.fold(items, init, step)

-- GOOD: pts1 is captured once at closure definition time, zero overhead per step
let step :: Pure { acc: { x: Int, y: Int, result: Int }, item: Int } -> { ... } =>
    let hit = Set.contains(pts1, it.item) in   -- pts1 captured, not in acc
    ...
in
List.fold(items, { x: 0, y: 0, result: 0 }, step)
```

**Rules:**
- `it` is the single argument inside the body — same as top-level helpers
- The captured environment is snapshotted at definition time; later
  mutations to bindings are NOT reflected in the closure
- Closures are first-class: passable, storable in records, returned from functions
- **Supported everywhere**: `keln verify`, `keln run`, and `keln compile`/`keln run-bc` (bytecode VM). Gap 17 added closure lifting to the VM.
- **`threshold` is a reserved keyword** — do not use it as a field or variable name.
  Use `cutoff`, `limit`, `max_val`, etc. instead.

**Eliminating `it.acc.` indirection in fold helpers — record destructuring in `let`:**
```keln
let step :: Pure { acc: State, item: Int } -> State =>
    let { mem, ip, done } = it.acc in
    -- now use mem, ip, done directly instead of it.acc.mem, it.acc.ip, it.acc.done
    ...
in
List.fold(ops, init, step)
```
`let { f1, f2, f3 } = expr in body` is a full record destructuring — all named fields are bound. Works in both tree-walker and bytecode VM. Use shorthand syntax: `{ mem, ip }` binds `mem` and `ip` to their values. Use `{ field: otherName }` to rename.

**Record update with `.with()` (Gap 11+18):**
```keln
state.with(count: state.count + 1)            -- update one field
state.with({ count: 0, label: "reset" })      -- update multiple fields
state.with(count: 0).with(label: "reset")    -- chained
```
Works in tree-walker AND bytecode VM. Use this to eliminate fold accumulator bloat:
```keln
-- BEFORE: rewrite all fields
{ acc: it.acc + 1, w: it.w, h: it.h, board: it.board }
-- AFTER: update only what changed
it.with(acc: it.acc + 1)
```

---

## 11. Modules

Modules declare typed contracts. They are not imported — they are
instantiated and passed explicitly as parameters.

```keln
module Database {
    requires: { connection: Connection, timeout: Duration }
    provides: {
        query:   IO TypeRef -> Result<List<TypeRef>, DbError>
        execute: IO String  -> Result<Unit, DbError>
    }
}

trusted module Log {
    provides: {
        info:  Log NonEmptyString -> Unit
        error: Log NonEmptyString -> Unit
    }
    reason: "logging output"
}
```

`trusted` skips verify block requirements. Use for external Rust implementations.
Always provide a `reason:`.

---

## 12. Verify Blocks

Verify blocks are the testing mechanism. They run at compile/verify time.

```keln
verify: {
    -- Simple given/expected:
    given(42)  -> Ok(42)
    given(-1)  -> Err(ValidationError.OutOfRange { value: -1 })

    -- Property-based:
    forall(n: Int where 1..100) ->
        validate(n) == Ok(n)

    -- With Clock mock:
    mock Clock { now() -> Timestamp { epoch_ms: 1000 } }
    given(Running { started_at: Timestamp { epoch_ms: 500 }, ... })
        -> Ok(Completed { ... })

    -- With FunctionRef mock:
    mock handler {
        call(Bytes.empty()) -> Ok(Bytes.fromString("done"))
        call(_)             -> Err(JobError.ExecutionFailed { message: "fail" })
    }
    given({ handler: _, payload: Bytes.empty() }) -> Ok(Completed { ... })
}
```

**forall logical operators** (only available inside forall/proves):
```keln
not(p)         -- logical negation
and(p, q)      -- logical and
or(p, q)       -- logical or
implies(p, q)  -- if p then q; short-circuits when p is false
```

---

## 13. Do Blocks

`do` sequences effectful operations. Non-final expressions must return `Unit`.
Final expression determines the block's type.

```keln
out: do {
    Log.info("starting")         -- Unit; ok as non-final
    let result = fetchData(id)   -- let binding; ok
    Log.info("fetched")          -- Unit; ok
    processResult(result)        -- final expression; determines return type
}
```

A `do` block ending in a tail call has type `Never`:
```keln
out: do {
    select { ... }   -- Unit
    workerLoop(ctx)  -- Never (tail call)
}
-- this do block has type Never
```

---

## 14. Pipeline Operator

`|>` passes the left value as the sole input to the right function.
Effect is the union of all steps.

```keln
out: input
    |> validate
    |> Result.bind(fetchUser)
    |> Result.bind(checkPermissions)
    |> Result.map(buildResponse)
```

---

## 15. Performance Pitfalls

### Fold with a growing List accumulator — O(N²) trap

Every fold step **clones the entire accumulator**. If your accumulator contains
a growing list, each step clones a list that is 1 element longer than the last.
For N items this is O(N²) total work and will be impractically slow for N > a
few hundred.

```keln
-- WRONG — pairs list in acc is cloned ~500k times; O(N^2):
List.fold(pts, { pairs: [], ... }, addOnePair)

-- RIGHT — generate pairs natively, then transform with List.map (no accumulator growth):
let rawPairs = List.combinations2(pts) in   -- Rust loop, no VM overhead
let dists    = List.map(rawPairs, computeDist) in
```

**Rule:** Never put a list into a fold accumulator if that list grows on every step
and the total number of elements produced is large (> ~1000). Use a native
combinator (`List.map`, `List.filter`, `List.combinations2`) that builds the
result in Rust, then process it in a second pass.

### Adding new stdlib builtins

New functions must be registered in **two places** or the runtime will call a
function named `""` (empty string):

1. `src/vm/ir.rs` — append the name to `BUILTIN_NAMES` (the static array near the
   bottom). This gives the function a compile-time index.
2. `src/eval/stdlib.rs` — add the name to the known-names match arm at the top
   of `dispatch`, and add the implementation match arm in the body.

Omitting step 1 causes the compiler to emit index `u16::MAX`, which resolves to
`""` at runtime and fails with `"unknown stdlib function ''"`.

---

## 16. Common Mistakes — Do Not Do These

| Wrong | Right | Why |
|---|---|---|
| `job.attempt` on sum type | `match job { Running(r) -> r.attempt \| _ -> 0 }` | Compile error without match |
| `Channel.new<T>` | `Channel.new<T>()` | Parens required; without them it's a type ref |
| `ch <- data; ch2 <- data` | `let d1, d2 = clone(data); ch1 <- d1; ch2 <- d2` | Ownership transfer on send |
| `if condition { ... }` | `match condition { true -> ... \| false -> ... }` | No if/else in Keln |
| `Timestamp.sub(a, b)` without guard | Check `Timestamp.gte(a, b)` first | Unchecked; may produce negative Duration |
| Lambda `\|x\| x + 1` | Named helper function | No lambdas |
| `String` error message | Typed error sum type | String errors are not exhaustive |
| `null` or `None` directly | `Maybe<T>` with `Maybe.none(Unit)` | No null |
| `not`, `and`, `or` in `out:` | Only in `forall`/`proves` | Scope-limited logical operators |
| `Float.approxEq` outside forall | `==` in normal code, `Float.approxEq` in forall | IEEE 754 NaN handling |
| Multi-file programs | Single file only | Registry (Phase 5) handles library reuse |
| Implicit clone | Explicit `clone()` | No hidden copies |
| `Map.fromList([])` when you mean `Map.empty` | `let m = Map.empty in ...` | `Map.empty` now evaluates immediately in all value positions |
| Growing list in fold accumulator (large N) | `List.combinations2` + `List.map` | O(N²) accumulator cloning |
| `Maybe.getOr(List.tail(xs), [])` | `List.tail(xs)` directly | `List.tail` returns `List<T>`, not `Maybe<List<T>>` |
| `List.prepend(item, list)` | `List.prepend(list, item)` | First arg is the list; item goes to front |
| `let x = "out" in ...` | Use a different name | `out`, `in`, `verify` are reserved keywords |
| `out: do { ... }` then `helpers: { ... }` | Use `out: let ... in ...` chain then `helpers:` | Parser does not handle `helpers:` after a `do`-block body |
| `threshold` as a field or variable | Use `cutoff`, `limit`, `max_val`, etc. | `threshold` is a reserved keyword (`promote: threshold` syntax) |
| Closure with `.with()` for context capture | `let name :: effects In -> Out => body in rest` | `.with()` on a function eagerly binds and does not capture env; use named capturing helper for capture |
| Explicit full record copy `{ a: r.a, b: r.b, c: newC }` | `r.with(c: newC)` | `.with()` on a record returns an updated copy; no need to list unchanged fields |
| `let step :: ... => body in ...` with `keln compile` — works as of Gap 17 | Use normally with `keln compile` / `keln run-bc` | Named capturing helpers are now supported in the bytecode VM via closure lifting |
| Recursive algorithm through a capturing closure (DFS, tree walk) causes `STATUS_STACK_OVERFLOW` in `dev` build even for shallow depth | Replace with iterative fixpoint (round-based DP with `List.foldUntil`) | Debug-mode Rust frames are ~10× larger than release; the 64 MB main-thread stack (added in Gap 15 fix) raises the limit but deep unbounded recursion still risks overflow |
| Missing closing `}` inside a `match` arm body | Count braces; add the missing `}` before `confidence:` | Parser reports `"expected pattern at N:M"` pointing at the next function-level keyword (`confidence:`, `reason:`, `verify:`) — not at the unclosed brace |
| `keln verify` crashes with no error message (STATUS_STACK_OVERFLOW, wrong output, silent failure) | Run `keln check <file>` first | `keln check` runs only the parser and prints `ok` or a parse error; if it prints `ok` the problem is in the evaluator, not the source |
| Fold runs for minutes/hours on large input | Move read-only `Set`/`Map`/`List` out of the accumulator into a capturing closure | Even with O(1) Rc clone, a value present in both the accumulator AND a mutation site forces a data copy per step; a captured value is snapshotted once and never re-copied |

---

## 17. Confidence and Provenance (Required Fields)

Every function must have `confidence:` and `reason:`. These are structured
data used by the verification and learning system.

```keln
confidence: auto       -- derives from verify coverage; use this unless overriding
confidence: 1.0        -- manual override; only when you are certain
confidence: 0.7        -- manual override; use when uncertain

reason: "delegates to validateToken; error wrapping is mechanical"
reason: "infinite loop; concurrency_not_verified"
reason: "pure arithmetic; no edge cases"
```

Optional provenance (use when the pattern is known):
```keln
provenance: {
    description: "standard retry-with-backoff pattern"
    pattern_id:  "concurrency.retry.exponential_backoff"
}
```

---

## 18. VerificationResult — Reading the Output

`keln verify <file>` emits JSON. Key fields:

```json
{
  "is_clean": true,              -- true = proceed; false = fix errors
  "compile_errors": [],          -- fix these first; block is_clean
  "test_failures": [],           -- given/forall failures; block is_clean
  "proof_violations": [],        -- failed proves blocks; block is_clean
  "coverage_gaps": [],           -- informational only; do not block
  "concurrency_not_verified": [], -- informational only; do not block
  "fuzz_status": [],             -- trusted module fuzz results; informational
  "program_confidence": { "value": 0.85, "variance": 0.1 }
}
```

Iteration protocol:
1. If `is_clean: false` → fix `compile_errors` first, then `test_failures`
2. If `is_clean: true` and coverage_gaps are present → add verify cases
3. Functions in `concurrency_not_verified` are correct by construction
   (ownership enforces no data races) but their ordering is not verified

---

*Keln AI Author Reference — v0.9*
*Single source of truth for AI authoring sessions.*
*Full spec: keln-spec-v0.9.md*
