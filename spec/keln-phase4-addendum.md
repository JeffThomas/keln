# Keln Phase 4 — Design Review Addendum
## Supplementary to Revision 4

This addendum addresses four substantive questions from a third design review,
closes one genuine spec gap (channel close and select timeout), and clarifies
two points where the spec was correct but potentially misleading. It also
captures Phase 5 items surfaced by the review.

---

## Clarification 1 — FunctionRef Is Not a Closure

The spec bans anonymous functions and closures (Tenet 3). Yet `FunctionRef`
appears in worker loops carrying `handler: FunctionRef<IO, Bytes, Result<Bytes, E>>`.
These are not in conflict. The distinction:

**A closure** captures free variables from its lexical environment at the time
it is created. The captured environment is heap-allocated and lives as long as
the closure. The closure can reference mutable state in that environment.

**A `FunctionRef` in Keln** is:
1. A statically-known function name (a string index into the function table)
2. An optional list of pre-bound `(name, Value)` pairs from `.with()` calls

The bound values in a `PartialFn` are **eagerly evaluated** at the time `.with()`
is called. They are concrete `Value`s, not references to variables. When the
`FunctionRef` is later invoked, those bound values are merged with the call
argument to form the input record.

```keln
-- This:
let handler = processJob.with(store: myStore, policy: retryPolicy)

-- Produces:
Value::PartialFn {
    name: "processJob",
    bound: [("store", <store_value>), ("policy", <policy_value>)]
}

-- At call time:
handler(jobPayload)
-- is equivalent to:
processJob({ store: <store_value>, policy: <policy_value>, _input: jobPayload })
```

The bound values are owned by the `PartialFn`. They are cloned from the binding
site. No reference to the original scope is retained. This is partial application,
not closure capture.

**In bytecode IR:** A `FunctionRef` value is either:
- `Value::FnRef(fn_idx: usize)` — just a function table index
- `Value::PartialFn { fn_idx: usize, bound: Vec<(String, Value)> }` — index plus
  eagerly-evaluated bound values

No heap-allocated environment object. No captured mutable state. No free variable
analysis required in the lowering pass.

---

## Clarification 2 — "Cloned" Means Semantically Independent, Not Necessarily a Full Copy

The ownership table says list operations clone their source. This is the
**semantic** guarantee: after `LIST_TAIL`, the caller's list binding remains
valid and independent from the result. It does **not** mandate that the runtime
allocates a full copy of the list's memory.

The runtime is free to implement persistent structural sharing (immutable
linked lists, persistent vectors, copy-on-write) as long as the observable
semantics are identical to full copying. From the program's perspective:
mutations are impossible (all values are immutable), so structural sharing
is always safe.

In Phase 4, the tree-walking interpreter uses `Vec<Value>` for lists, and
`LIST_TAIL` does produce a full copy. This is correct and simple. A
persistent-vector optimization is a Phase 5 concern once there are real
workloads to profile against.

The spec's "cloned" language will not change — it accurately describes the
semantic contract. The implementation note above explains why O(n) copy is
not inevitable.

---

## Spec Gap — Channel Close and Select Timeout

### The Gap

The language spec's `select` syntax supports a `timeout` arm:

```keln
select {
    msg = <-ctx.job_ch -> handleJob(msg, ctx)
    _   = <-ctx.stop_ch -> doShutdown(ctx)
    timeout(Duration.seconds(30)) -> handleTimeout(ctx)
}
```

The Phase 4 instruction set (Revision 4) specified `MATCH_TAG_EQ`, `CHAN_RECV`,
and `JUMP` but did not include a `SELECT` instruction or a `SELECT_TIMEOUT`
instruction. This is an omission. `select` cannot be lowered to existing
instructions without a dedicated opcode.

Additionally, the spec has no `CHAN_CLOSE` instruction. The idiomatic shutdown
pattern — a dedicated stop channel — is a valid workaround, but a proper
`CHAN_CLOSE` with `CHAN_RECV` returning `Maybe<T>` is cleaner and aligns with
the spec's "invalid programs are unrepresentable" tenet.

### Resolution: SELECT Instruction

`select` lowers to a single `SELECT` instruction carrying all arms inline.
This is the correct lowering because `select` is atomic — in the async model,
all channels are polled simultaneously by `tokio::select!`, which cannot be
expressed as sequential `CHAN_RECV` instructions.

```
SELECT  Rdst, [SelectArm0, SelectArm1, ...], <timeout_arm: Option<TimeoutArm>>

SelectArm:
    binding_reg:  usize          -- register to write received value into (or 0 if "_")
    channel_reg:  usize          -- register holding the Channel value
    body_ip:      usize          -- instruction index of arm body
    body_end_ip:  usize          -- instruction index after arm body (for fall-through)

TimeoutArm:
    duration_reg: usize          -- register holding Duration value
    body_ip:      usize          -- instruction index of timeout body
```

**Sync model behavior:**
Poll each `channel_reg` in order. First non-empty channel wins: write received
value to `binding_reg`, jump to `body_ip`. If no channel is ready and a
`TimeoutArm` is present, jump to timeout `body_ip`. If no channel is ready and
no timeout, return `Value::Unit` (existing behavior, consistent with tree-walker).

**Async model behavior (Phase 4c):**
Emit a `tokio::select!` macro call polling all channels simultaneously. The
timeout arm maps to `tokio::time::sleep(duration)`. True non-deterministic
selection.

**Lowering:**

```keln
select {
    msg = <-job_ch -> handleJob(msg)
    _   = <-stop_ch -> Unit
    timeout(Duration.seconds(5)) -> handleTimeout(Unit)
}
```

```
-- channels already in registers from prior FIELD_GET or LOAD
SELECT R_result,
    [SelectArm { binding=R_msg, channel=R_job_ch, body_ip=.arm0, end_ip=.arm0_end },
     SelectArm { binding=0,     channel=R_stop_ch, body_ip=.arm1, end_ip=.arm1_end }],
    Some(TimeoutArm { duration=R_dur, body_ip=.timeout })

.arm0:
    CALL R_result, <handleJob>, R_msg
    JUMP .select_end
.arm0_end:

.arm1:
    LOAD_UNIT R_result
    JUMP .select_end
.arm1_end:

.timeout:
    CALL_BUILTIN R_result, <handleTimeout>, [R_unit]
    JUMP .select_end

.select_end:
```

### Resolution: CHAN_CLOSE and Maybe<T> Receive

```
CHAN_CLOSE   Rchan              -- mark channel as closed; Rchan cloned (handle remains valid)
                               -- subsequent CHAN_SEND on a closed channel → RuntimeError
                               -- subsequent CHAN_RECV on a closed empty channel → Maybe::none()
                               -- subsequent CHAN_RECV on a closed non-empty channel → Maybe::some(value)
```

**CHAN_RECV semantics update (closed channels):**

| Channel state | Sync model | Async model |
|---|---|---|
| Open, non-empty | `Ok(value)` — returns head | `Ok(value)` — returns head |
| Open, empty | `RuntimeError` | suspends until value or close |
| Closed, non-empty | `Maybe::some(value)` | `Maybe::some(value)` |
| Closed, empty | `Maybe::none()` | `Maybe::none()` |

**Breaking change note:** This changes `CHAN_RECV`'s return type from `T` to
`Maybe<T>` when the channel may be closed. Callers that know the channel is
always open (the common case) continue to use `CHAN_RECV` and pattern-match
away the `Maybe` wrapper. The type checker will enforce this: `Channel<T>`
is the type for channels that may or may not be closed; a future
`OpenChannel<T>` refinement could recover the direct-`T` receive if needed.

**Practical note:** The stop-channel pattern (`<-ctx.stop_ch`) from the
validation exercises remains idiomatic. `CHAN_CLOSE` is for cases where the
sender wants to signal completion without sending a value — e.g., a producer
that has finished all items.

**Updated instruction table rows:**

```
CHAN_CLOSE   Rchan              -- Rchan cloned; marks channel closed
CHAN_RECV    Rdst, Rchan        -- returns Maybe<T> (Some(v) or None on closed+empty)
                               -- Rchan cloned; sync: RuntimeError on open+empty
```

**Updated Value Shape Guarantees rows:**

| Instruction | Expected input | Error on mismatch |
|---|---|---|
| `CHAN_CLOSE` | Rchan is `Channel` | RuntimeError |
| `CHAN_RECV` | Rchan is `Channel` | RuntimeError |
| `SELECT` | all channel_reg are `Channel` | RuntimeError |

---

## Phase 5 Items Surfaced by Review

These items are substantively interesting but do not belong in Phase 4.
They are recorded here for the Phase 5 roadmap.

### Gas Metering

```
GAS  <cost: u32>   -- consume <cost> gas units from the current execution budget
                   -- RuntimeError if budget exhausted
```

AI agents operating with compute budgets need a way to bound execution.
Gas metering emitted at function entry (cost = register_count as a proxy
for frame complexity) and at loop-back edges (i.e., `JUMP` instructions
that target an earlier `ip`) would provide a coarse but useful bound.

The `VerificationResult` would gain a `gas_used` field when metering is
enabled. This is a Phase 5 item alongside PatternDB.

### Serialization of VM State

The ability to serialize the entire VM state (all frames, all channel
contents) to JSON would allow AI agents to inspect running programs, implement
checkpointing, and resume from failure. This requires `Value: Serialize` and
a snapshot of the `Vec<CallFrame>` call stack. Architecturally straightforward
once the explicit call stack is in place (Phase 4b follow-up).

### Hot Code Reloading

Replacing a `KelnFn` in the function table while the VM is running. Requires
version-stamping frames so a running function is not replaced mid-execution.
Useful for long-running agents where the AI corrects a function without
restarting the process. Phase 5+.

---

## Updated Instruction Set (Additions Only)

Add these to the Phase 4 instruction set in Revision 4:

```
SELECT  Rdst, [SelectArm...], Option<TimeoutArm>
         -- see §Resolution: SELECT Instruction for full semantics
CHAN_CLOSE  Rchan
         -- mark channel closed; subsequent CHAN_RECV returns Maybe<T>
```

Update `CHAN_RECV` semantics to return `Maybe<T>` when channel may be closed.

---

## Updated Phase 4 Checklist Additions

### Phase 4a additions
- [ ] Define `SelectArm { binding_reg, channel_reg, body_ip, end_ip }` struct
- [ ] Define `TimeoutArm { duration_reg, body_ip }` struct
- [ ] Add `SELECT` to `Instruction` enum
- [ ] Add `CHAN_CLOSE` to `Instruction` enum
- [ ] Update `CHAN_RECV` to return `Maybe<T>` (closed channel semantics)
- [ ] Lower `select { ... }` expression to `SELECT` instruction
- [ ] Lowering test: `select` with timeout arm; verify `SELECT` instruction emitted

### Phase 4b additions
- [ ] Implement `SELECT` in interpreter: sync poll loop; timeout arm as fallback
- [ ] Implement `CHAN_CLOSE`: mark channel closed in `ChannelInner`
- [ ] Update `CHAN_RECV`: return `Maybe::none()` on closed+empty, `Maybe::some(v)` on closed+non-empty

### Phase 4c additions
- [ ] Update `SELECT` async implementation to use `tokio::select!` with timeout via `tokio::time::sleep`

---

*Phase 4 design review addendum — Keln v0.9*
*Addresses: FunctionRef vs closure, list clone semantics, channel close,*
*SELECT instruction, Phase 5 items (gas, hot reload, state serialization).*
*To be merged into keln-spec-v1.0 alongside Phase 4 implementation.*
