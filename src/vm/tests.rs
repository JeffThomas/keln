use crate::eval::Value;
use crate::parser::parse;
use crate::vm::codec;
use crate::vm::exec::{execute_fn};
use crate::vm::ir::{Instruction, KelnModule};
use crate::vm::lower::lower_program;

// =============================================================================
// Execution helpers
// =============================================================================

fn run(src: &str, fn_name: &str, arg: Value) -> Value {
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    execute_fn(&module, fn_name, arg).expect("execute")
}

fn assert_both_backends(src: &str, fn_name: &str, arg: Value, expected: Value) {
    let tw = crate::eval::eval_fn(src, fn_name, arg.clone()).expect("tree-walker");
    let bc = run(src, fn_name, arg);
    assert_eq!(tw, expected, "tree-walker result");
    assert_eq!(bc, expected, "bytecode VM result");
    assert_eq!(tw, bc, "backends agree");
}

// =============================================================================
// Helpers
// =============================================================================

fn compile(src: &str) -> KelnModule {
    let prog = parse(src).expect("parse failed");
    lower_program(&prog).expect("lower failed")
}

fn instrs<'a>(module: &'a KelnModule, name: &str) -> &'a Vec<Instruction> {
    let idx = module.fn_idx(name).unwrap_or_else(|| panic!("fn '{}' not found", name));
    &module.fns[idx].instructions
}

fn reg_count(module: &KelnModule, name: &str) -> usize {
    let idx = module.fn_idx(name).unwrap_or_else(|| panic!("fn '{}' not found", name));
    module.fns[idx].register_count
}

/// Count instructions matching a predicate.
fn count_of<F: Fn(&Instruction) -> bool>(is: &[Instruction], f: F) -> usize {
    is.iter().filter(|i| f(i)).count()
}

// =============================================================================
// Trace 2 — double (spec §Worked Execution Traces)
// =============================================================================
//
// fn double { Pure Int -> Int  in: n  out: n + n }
//
// Expected bytecode (register_count: 2):
//   ip=0: Add { dst: 1, src1: 0, src2: 0 }
//   ip=1: Return { src: 1 }

#[test]
fn test_lower_double_instructions() {
    let src = "fn double { Pure Int -> Int  in: n  out: n + n }";
    let module = compile(src);
    let is = instrs(&module, "double");

    assert_eq!(is.len(), 2, "double: expected 2 instructions");

    // ip=0 is ADD R1, R0, R0
    match &is[0] {
        Instruction::Add { dst, src1, src2 } => {
            assert_eq!(*dst, 1);
            assert_eq!(*src1, 0);
            assert_eq!(*src2, 0);
        }
        other => panic!("double ip=0 expected Add, got {:?}", other),
    }

    // ip=1 is RETURN R1
    match &is[1] {
        Instruction::Return { src } => assert_eq!(*src, 1),
        other => panic!("double ip=1 expected Return, got {:?}", other),
    }

    assert_eq!(reg_count(&module, "double"), 2);
}

// =============================================================================
// Trace 2 — quadruple (spec §Worked Execution Traces)
// =============================================================================
//
// fn double   { Pure Int -> Int  in: n  out: n + n }
// fn quadruple{ Pure Int -> Int  in: n  out: double(double(n)) }
//
// The inner double call is NOT in tail position (its result feeds the outer).
// The outer double call IS in tail position for quadruple → TailCall.
//
// Expected (register_count: 2):
//   ip=0: Call     { dst: 1, fn_idx: <double>, arg_reg: 0 }
//   ip=1: TailCall { fn_idx: <double>, arg_reg: 1 }

#[test]
fn test_lower_quadruple_instructions() {
    let src = "fn double { Pure Int -> Int  in: n  out: n + n }\n\
               fn quadruple { Pure Int -> Int  in: n  out: double(double(n)) }";
    let module = compile(src);
    let double_idx = module.fn_idx("double").expect("double not found");
    let is = instrs(&module, "quadruple");

    assert_eq!(is.len(), 2, "quadruple: expected 2 instructions");

    // ip=0: CALL (non-tail — result feeds outer call)
    match &is[0] {
        Instruction::Call { dst, fn_idx, arg_reg } => {
            assert_eq!(*fn_idx, double_idx, "inner call targets double");
            assert_eq!(*arg_reg, 0, "inner call arg is R0");
            assert_eq!(*dst, 1);
        }
        other => panic!("quadruple ip=0 expected Call, got {:?}", other),
    }

    // ip=1: TAIL_CALL (tail position — frame reset)
    match &is[1] {
        Instruction::TailCall { fn_idx, arg_reg } => {
            assert_eq!(*fn_idx, double_idx, "outer call targets double");
            assert_eq!(*arg_reg, 1, "outer call arg is result of inner double");
        }
        other => panic!("quadruple ip=1 expected TailCall, got {:?}", other),
    }

    assert_eq!(reg_count(&module, "quadruple"), 2);
}

// =============================================================================
// Trace 1 — countdown (spec §Worked Execution Traces)
// =============================================================================
//
// fn countdown {
//     Pure Int -> Int
//     in: n
//     out: match n { 0 -> 0  _ -> countdown(n - 1) }
// }
//
// Key invariants (layout may differ from spec trace due to arm-ordering):
//   - Exactly one MatchLitEq for literal 0 (src=R0)
//   - Exactly one TailCall to countdown itself (fn_idx=0)
//   - No Call instructions (all tail or return)
//   - TailCall target_ip resolves during lowering (not 0 unless fn is at idx 0)
//   - MatchLitEq target resolves to the literal-0 body, not 0

#[test]
fn test_lower_countdown_has_match_lit_eq() {
    let src = "fn countdown {\n\
                   Pure Int -> Int\n\
                   in: n\n\
                   out: match n { 0 -> 0  _ -> countdown(n - 1) }\n\
               }";
    let module = compile(src);
    let is = instrs(&module, "countdown");
    let countdown_fn_idx = module.fn_idx("countdown").expect("countdown");

    // There must be exactly one MatchLitEq
    let match_lits: Vec<_> = is.iter().enumerate()
        .filter(|(_, i)| matches!(i, Instruction::MatchLitEq { .. }))
        .collect();
    assert_eq!(match_lits.len(), 1, "expected exactly one MatchLitEq");

    // The MatchLitEq operates on R0 (the input `n`)
    match match_lits[0].1 {
        Instruction::MatchLitEq { src, target_ip, .. } => {
            assert_eq!(*src, 0, "MatchLitEq src must be R0");
            assert!(*target_ip > 0, "MatchLitEq target_ip must be resolved (> 0)");
        }
        _ => unreachable!(),
    }

    // There must be exactly one TailCall to countdown
    let tail_calls: Vec<_> = is.iter()
        .filter(|i| matches!(i, Instruction::TailCall { fn_idx, .. } if *fn_idx == countdown_fn_idx))
        .collect();
    assert_eq!(tail_calls.len(), 1, "expected one TailCall to countdown");

    // No non-tail calls to countdown (all calls to self are tail calls)
    let non_tail_self: usize = count_of(is, |i| {
        matches!(i, Instruction::Call { fn_idx, .. } if *fn_idx == countdown_fn_idx)
    });
    assert_eq!(non_tail_self, 0, "countdown must have no non-tail self-calls");
}

#[test]
fn test_lower_countdown_jump_targets_resolved() {
    let src = "fn countdown {\n\
                   Pure Int -> Int\n\
                   in: n\n\
                   out: match n { 0 -> 0  _ -> countdown(n - 1) }\n\
               }";
    let module = compile(src);
    let is = instrs(&module, "countdown");

    // All Jump instructions must have resolved (non-sentinel) targets.
    for (ip, instr) in is.iter().enumerate() {
        if let Instruction::Jump { target_ip } = instr {
            // A Jump to itself or past the end is suspicious
            assert_ne!(*target_ip, ip, "Jump at ip={} targets itself", ip);
        }
        if let Instruction::MatchLitEq { target_ip, .. } = instr {
            assert!(
                *target_ip < is.len(),
                "MatchLitEq target_ip={} out of range (len={})",
                target_ip, is.len()
            );
        }
    }
}

// =============================================================================
// Trace 3 — safeExtract (spec §Worked Execution Traces)
// =============================================================================
//
// fn safeExtract {
//     Pure Result<Int, String> -> Int
//     in: r
//     out: match r { Ok(n) -> n  Err(_) -> 0 }
// }
//
// Key invariants:
//   - Exactly two MatchTagEq instructions (one for Ok, one for Err)
//   - VariantPayload emitted in the Ok arm (at minimum)
//   - Both arms end with Return
//   - All MatchTagEq target_ips are resolved

#[test]
fn test_lower_safe_extract_variant_instructions() {
    let src = "fn safeExtract {\n\
                   Pure Result<Int, String> -> Int\n\
                   in: r\n\
                   out: match r { Ok(n) -> n  Err(_) -> 0 }\n\
               }";
    let module = compile(src);
    let is = instrs(&module, "safeExtract");

    // Must have at least one MatchTagEq (for Ok; may have a second for Err)
    let tag_eqs = count_of(is, |i| matches!(i, Instruction::MatchTagEq { .. }));
    assert!(tag_eqs >= 1, "expected at least one MatchTagEq");

    // Must have at least one VariantPayload (for Ok arm's `n` extraction)
    let vp_count = count_of(is, |i| matches!(i, Instruction::VariantPayload { .. }));
    assert!(vp_count >= 1, "expected at least one VariantPayload");

    // Must have at least two Return instructions (one per arm)
    let ret_count = count_of(is, |i| matches!(i, Instruction::Return { .. }));
    assert!(ret_count >= 2, "expected at least two Return instructions");

    // All MatchTagEq targets must be resolved (within instruction array)
    for instr in is.iter() {
        if let Instruction::MatchTagEq { target_ip, .. } = instr {
            assert!(
                *target_ip < is.len(),
                "MatchTagEq target_ip={} out of range (len={})",
                target_ip, is.len()
            );
        }
    }
}

#[test]
fn test_lower_safe_extract_ok_tag_jumps_to_variant_payload() {
    let src = "fn safeExtract {\n\
                   Pure Result<Int, String> -> Int\n\
                   in: r\n\
                   out: match r { Ok(n) -> n  Err(_) -> 0 }\n\
               }";
    let module = compile(src);
    let is = instrs(&module, "safeExtract");

    // Find the MatchTagEq for Ok (tag_id = 0, since "Ok" is interned first)
    let ok_tag_id = module.tags.lookup("Ok").expect("Ok tag not interned");
    let ok_match = is.iter().find(|i| {
        matches!(i, Instruction::MatchTagEq { tag_id, .. } if *tag_id == ok_tag_id)
    }).expect("MatchTagEq for Ok not found");

    // The Ok MatchTagEq must jump to a VariantPayload instruction
    if let Instruction::MatchTagEq { target_ip, .. } = ok_match {
        assert!(
            matches!(is[*target_ip], Instruction::VariantPayload { .. }),
            "Ok MatchTagEq must jump to a VariantPayload, got {:?}", is[*target_ip]
        );
    }
}

// =============================================================================
// No-Call-in-tail-position: both backends agree on the shape
// =============================================================================

#[test]
fn test_lower_double_no_tail_call() {
    let src = "fn double { Pure Int -> Int  in: n  out: n + n }";
    let module = compile(src);
    let is = instrs(&module, "double");

    // double is pure arithmetic — no calls at all
    let calls = count_of(is, |i| {
        matches!(i, Instruction::Call { .. } | Instruction::TailCall { .. })
    });
    assert_eq!(calls, 0, "double should have no calls");
}

#[test]
fn test_lower_countdown_no_non_tail_self_call() {
    let src = "fn countdown {\n\
                   Pure Int -> Int\n\
                   in: n\n\
                   out: match n { 0 -> 0  _ -> countdown(n - 1) }\n\
               }";
    let module = compile(src);
    let countdown_idx = module.fn_idx("countdown").expect("countdown");
    let is = instrs(&module, "countdown");

    // Every call to countdown must be a TailCall (TCO invariant)
    let non_tail_to_self = count_of(is, |i| {
        matches!(i, Instruction::Call { fn_idx, .. } if *fn_idx == countdown_idx)
    });
    assert_eq!(non_tail_to_self, 0, "all countdown self-calls must be TailCall");
}

// =============================================================================
// Interpreter execution tests — Trace 2: double and quadruple
// =============================================================================

#[test]
fn test_exec_double() {
    assert_both_backends(
        "fn double { Pure Int -> Int  in: n  out: n + n }",
        "double", Value::Int(5), Value::Int(10),
    );
}

#[test]
fn test_exec_quadruple() {
    assert_both_backends(
        "fn double { Pure Int -> Int  in: n  out: n + n }\n\
         fn quadruple { Pure Int -> Int  in: n  out: double(double(n)) }",
        "quadruple", Value::Int(3), Value::Int(12),
    );
}

// =============================================================================
// Interpreter execution tests — Trace 1: countdown
// =============================================================================

#[test]
fn test_exec_countdown_small() {
    assert_both_backends(
        "fn countdown {\n\
             Pure Int -> Int\n\
             in: n\n\
             out: match n { 0 -> 0  _ -> countdown(n - 1) }\n\
         }",
        "countdown", Value::Int(3), Value::Int(0),
    );
}

/// TCO validation: countdown(1_000_000) must complete without stack overflow.
/// Rust call stack depth remains 1 throughout (TAIL_CALL resets frame in-place).
#[test]
fn test_exec_countdown_tco_no_stack_overflow() {
    let src = "fn countdown {\n\
                   Pure Int -> Int\n\
                   in: n\n\
                   out: match n { 0 -> 0  _ -> countdown(n - 1) }\n\
               }";
    let result = run(src, "countdown", Value::Int(1_000_000));
    assert_eq!(result, Value::Int(0));
}

// =============================================================================
// Interpreter execution tests — Trace 3: safeExtract
// =============================================================================

#[test]
fn test_exec_safe_extract_ok() {
    let src = "fn safeExtract {\n\
                   Pure Result<Int, String> -> Int\n\
                   in: r\n\
                   out: match r { Ok(n) -> n  Err(_) -> 0 }\n\
               }";
    assert_both_backends(
        src, "safeExtract",
        Value::Variant {
            name: "Ok".to_string(),
            payload: crate::eval::VariantPayload::Tuple(Box::new(Value::Int(42))),
        },
        Value::Int(42),
    );
}

#[test]
fn test_exec_safe_extract_err() {
    let src = "fn safeExtract {\n\
                   Pure Result<Int, String> -> Int\n\
                   in: r\n\
                   out: match r { Ok(n) -> n  Err(_) -> 0 }\n\
               }";
    assert_both_backends(
        src, "safeExtract",
        Value::Variant {
            name: "Err".to_string(),
            payload: crate::eval::VariantPayload::Tuple(Box::new(Value::Str("oops".to_string()))),
        },
        Value::Int(0),
    );
}

// =============================================================================
// Interpreter execution tests — arithmetic and let bindings
// =============================================================================

#[test]
fn test_exec_arithmetic() {
    assert_both_backends(
        "fn add3 { Pure Int -> Int  in: n  out: n + 3 }",
        "add3", Value::Int(7), Value::Int(10),
    );
}

#[test]
fn test_exec_modulo() {
    assert_both_backends(
        "fn fizz { Pure Int -> Int  in: n  out: n % 3 }",
        "fizz", Value::Int(9), Value::Int(0),
    );
}

// =============================================================================
// Phase 4d — codec roundtrip and compile+run tests
// =============================================================================

fn roundtrip(src: &str) -> KelnModule {
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    let bytes = codec::encode(&module, codec::FLAG_DEBUG_INFO, None).expect("encode");
    assert!(bytes.starts_with(b"KELN"), "missing magic");
    let (decoded, flags, entry) = codec::decode(&bytes).expect("decode");
    assert_eq!(flags, codec::FLAG_DEBUG_INFO);
    assert_eq!(entry, None);
    decoded
}

#[test]
fn test_codec_roundtrip_double() {
    let src = "fn double { Pure Int -> Int  in: n  out: n * 2 }";
    let decoded = roundtrip(src);
    let result = execute_fn(&decoded, "double", Value::Int(5)).expect("execute");
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_codec_roundtrip_countdown() {
    let src = "fn countdown { Pure Int -> Int\n\
               in: n\n\
               out: match n { 0 -> 0  _ -> countdown(n - 1) } }";
    let decoded = roundtrip(src);
    let result = execute_fn(&decoded, "countdown", Value::Int(100)).expect("execute");
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_codec_header_magic() {
    let src = "fn id { Pure Int -> Int  in: n  out: n }";
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    let bytes = codec::encode(&module, 0, None).expect("encode");
    assert_eq!(&bytes[0..4], codec::MAGIC);
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    assert_eq!(version, codec::FORMAT_VERSION);
}

#[test]
fn test_codec_entry_point_roundtrip() {
    let src = "fn main { Pure Int -> Int  in: n  out: n + 1 }";
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    let entry_idx = module.fn_idx("main").expect("main not found");
    let bytes = codec::encode(&module, codec::FLAG_DEBUG_INFO, Some(entry_idx)).expect("encode");
    let (decoded, flags, entry) = codec::decode(&bytes).expect("decode");
    assert_eq!(flags, codec::FLAG_DEBUG_INFO | codec::FLAG_HAS_ENTRY);
    assert_eq!(entry, Some(entry_idx));
    let fn_name = decoded.fns[entry.unwrap()].name.clone();
    let result = execute_fn(&decoded, &fn_name, Value::Int(41)).expect("execute");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_codec_bad_magic_rejected() {
    let mut bytes = vec![0u8; 12];
    bytes[0..4].copy_from_slice(b"NOPE");
    assert!(codec::decode(&bytes).is_err());
}

// =============================================================================
// Stack trace tests (4b-6)
// =============================================================================

#[test]
fn test_exec_stack_trace_on_nested_error() {
    // divide(n) + 1 forces a non-tail Call so caller stays on the explicit call stack
    let src = "\
fn divide { Pure Int -> Int  in: n  out: n / 0 }\n\
fn caller { Pure Int -> Int  in: n  out: divide(n) + 1 }";
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    let err = execute_fn(&module, "caller", Value::Int(5)).unwrap_err();
    assert!(!err.stack_trace.is_empty(), "expected non-empty stack trace");
    let trace = err.stack_trace.join("\n");
    assert!(trace.contains("divide"), "trace should mention 'divide': {}", trace);
    assert!(trace.contains("caller"), "trace should mention 'caller': {}", trace);
}

#[test]
fn test_exec_stack_trace_display() {
    let src = "\
fn bad { Pure Int -> Int  in: n  out: n / 0 }\n\
fn outer { Pure Int -> Int  in: n  out: bad(n) }";
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    let err = execute_fn(&module, "outer", Value::Int(1)).unwrap_err();
    let display = format!("{}", err);
    assert!(display.contains("division by zero"), "display: {}", display);
    assert!(display.contains("stack trace"), "display: {}", display);
}

#[test]
fn test_exec_stack_trace_direct_error_no_callers() {
    let src = "fn solo { Pure Int -> Int  in: n  out: n / 0 }";
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    let err = execute_fn(&module, "solo", Value::Int(3)).unwrap_err();
    assert_eq!(err.stack_trace.len(), 1);
    assert!(err.stack_trace[0].contains("solo"));
}

#[test]
fn test_codec_release_strips_debug_names() {
    let src = "fn double { Pure Int -> Int  in: n  out: n * 2 }";
    let prog = parse(src).expect("parse");
    let mut module = lower_program(&prog).expect("lower");
    // strip debug names to simulate --release
    for f in &mut module.fns {
        f.debug_names.iter_mut().for_each(|n| *n = None);
    }
    let bytes = codec::encode(&module, 0, None).expect("encode");
    let (decoded, flags, _) = codec::decode(&bytes).expect("decode");
    assert_eq!(flags, 0);
    assert!(decoded.fns[0].debug_names.iter().all(|n| n.is_none()));
    let result = execute_fn(&decoded, "double", Value::Int(7)).expect("execute");
    assert_eq!(result, Value::Int(14));
}

// =============================================================================
// Fix 1 — Channel.close(ch)
// =============================================================================

#[test]
fn test_channel_close_lowers_to_chan_close_instruction() {
    // Verify that Channel.close(ch) in source emits a ChanClose instruction
    // in the bytecode. Before this fix, Channel.close had no AST node or lowering
    // path so it could never appear in compiled output.
    let src = r#"fn closeIt {
    IO Closeable<Channel<Int>> -> Unit
    in: ch
    out: Channel.close(ch)
}"#;
    let module = compile(src);
    let is = instrs(&module, "closeIt");
    let count = count_of(is, |i| matches!(i, Instruction::ChanClose { .. }));
    assert_eq!(count, 1, "Channel.close should emit exactly one ChanClose instruction");
}

#[test]
fn test_channel_close_recv_after_close_returns_none() {
    // Both backends: a recv on a closed Closeable<Channel<T>> must return None.
    // We pre-close the channel in Rust rather than calling Channel.close inside
    // the function body, because `Channel.close(ch)\n<-ch` in a do-block is parsed
    // as `Channel.close(ch) <- ch` (a send) due to operator precedence.
    use std::rc::Rc;
    use std::cell::RefCell;
    use crate::eval::{ChannelInner, VariantPayload};

    let src = r#"fn recvFromClosed {
    IO Closeable<Channel<Int>> -> Maybe<Int>
    in: ch
    out: <-ch
}"#;
    let mut inner = ChannelInner::new_closeable();
    inner.closed = true;
    let ch = Value::Channel(Rc::new(RefCell::new(inner.clone())));
    let expected = Value::Variant { name: "None".to_string(), payload: VariantPayload::Unit };

    // Tree-walker
    let tw = crate::eval::eval_fn(src, "recvFromClosed", ch).expect("tree-walker eval");
    assert_eq!(tw, expected, "tree-walker");

    // Bytecode VM — ChanRecvMaybe is now emitted by the lowering pass for
    // Closeable<Channel<T>> parameters, so this path is exercised correctly.
    let ch2 = Value::Channel(Rc::new(RefCell::new(inner)));
    let bc = run(src, "recvFromClosed", ch2);
    assert_eq!(bc, expected, "bytecode VM");
}

// =============================================================================
// Fix 2 — Schema versioning in codec
// =============================================================================

fn skip_section_bytes(data: &[u8], pos: &mut usize) {
    let len = u32::from_le_bytes([data[*pos], data[*pos+1], data[*pos+2], data[*pos+3]]) as usize;
    *pos += 4 + len;
}

#[test]
fn test_codec_schema_roundtrip_with_record_types() {
    // A program with a record-payload variant triggers layout table entries,
    // which populate the schema_table section. Verify encode→decode succeeds
    // (fingerprints match) so the happy path doesn't produce SchemaMismatch.
    let src = r#"type JobErr = | Timeout { after: Int }
fn makeErr { Pure Int -> JobErr  in: n  out: Timeout { after: n } }"#;
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    assert!(!module.layouts.layouts.is_empty(), "expected non-empty layout table");
    let bytes = codec::encode(&module, 0, None).expect("encode");
    let (decoded, _, _) = codec::decode(&bytes).expect("decode — schema mismatch on clean roundtrip");
    // Sanity check: the decoded module still runs correctly
    let result = execute_fn(&decoded, "makeErr", Value::Int(5)).expect("execute");
    assert!(
        matches!(&result, Value::Variant { name, .. } if name == "Timeout"),
        "expected Timeout variant, got {:?}", result
    );
}

#[test]
fn test_codec_schema_mismatch_detected() {
    // Corrupt the last byte of the schema_table section to simulate a compiled
    // fingerprint that no longer matches the current layout. Decode must return
    // LoadError::SchemaMismatch, not silently succeed.
    use crate::vm::codec::LoadError;

    let src = r#"type JobErr = | Timeout { after: Int }
fn makeErr { Pure Int -> JobErr  in: n  out: Timeout { after: n } }"#;
    let prog = parse(src).expect("parse");
    let module = lower_program(&prog).expect("lower");
    let mut bytes = codec::encode(&module, 0, None).expect("encode");

    // Navigate to the schema_table section (comes after header, const, tags, layouts).
    let mut pos = 8usize; // skip magic(4) + version(2) + flags(2)
    skip_section_bytes(&bytes, &mut pos); // const_table
    skip_section_bytes(&bytes, &mut pos); // tag_table
    skip_section_bytes(&bytes, &mut pos); // layout_table

    // pos is now at the schema_table length prefix.
    let schema_len = u32::from_le_bytes([bytes[pos], bytes[pos+1], bytes[pos+2], bytes[pos+3]]) as usize;
    assert!(schema_len > 0, "schema table must be non-empty for this test to be meaningful");

    // Flip the last byte of the section data — this falls within the 32-byte
    // fingerprint at the end of the final SchemaEntry without corrupting bincode framing.
    let last_byte_idx = pos + 4 + schema_len - 1;
    bytes[last_byte_idx] ^= 0xFF;

    let result = codec::decode(&bytes);
    assert!(
        matches!(result, Err(LoadError::SchemaMismatch { .. })),
        "expected SchemaMismatch from corrupted fingerprint, got: {:?}", result.err()
    );
}

// =============================================================================
// let … in expressions
// =============================================================================

#[test]
fn test_let_in_basic() {
    assert_both_backends(
        r#"fn f { Pure Int -> Int  in: n  out: let x = n + 1 in x * 2 }"#,
        "f", Value::Int(3), Value::Int(8),
    );
}

#[test]
fn test_let_in_nested() {
    assert_both_backends(
        r#"fn f { Pure Int -> Int  in: n  out: let x = n + 1 in let y = x * 2 in x + y }"#,
        "f", Value::Int(4), Value::Int(15), // x=5, y=10, x+y=15
    );
}

#[test]
fn test_let_in_scoped_name_does_not_leak() {
    // After the body of `let x = 1 in x`, using `x` at outer scope is an error.
    // Here we just verify the inner binding is used correctly and the outer
    // function result is the body value, not Unit.
    assert_both_backends(
        r#"fn f { Pure Int -> Int  in: n  out: let x = 10 in x + n }"#,
        "f", Value::Int(5), Value::Int(15),
    );
}

#[test]
fn test_let_in_in_match_arm() {
    assert_both_backends(
        r#"fn f { Pure Int -> Int
    in: n
    out: match n == 0 {
        true  -> let z = 99 in z
        false -> let v = n * n in v
    }
}"#,
        "f", Value::Int(7), Value::Int(49),
    );
}

// =============================================================================
// Sibling helpers can call each other
// =============================================================================

#[test]
fn test_sibling_helper_call() {
    assert_both_backends(
        r#"fn compute {
    Pure Int -> Int
    in: n
    out: double(n) + triple(n)
    helpers: {
        double :: Pure Int -> Int => it * 2
        triple :: Pure Int -> Int => it * 3
    }
}"#,
        "compute", Value::Int(4), Value::Int(20), // 8 + 12
    );
}

#[test]
fn test_sibling_helper_chain() {
    // processLine calls helper, which calls another sibling helper
    assert_both_backends(
        r#"fn run {
    Pure Int -> Int
    in: n
    out: step1(n)
    helpers: {
        step1 :: Pure Int -> Int => step2(it) + 1
        step2 :: Pure Int -> Int => it * 10
    }
}"#,
        "run", Value::Int(3), Value::Int(31), // step2(3)=30, step1=31
    );
}

// =============================================================================
// File.read / File.readLines
// =============================================================================

#[test]
fn test_file_read() {
    let dir = std::env::temp_dir();
    let path = dir.join("keln_test_file_read.txt");
    std::fs::write(&path, "hello world").unwrap();
    let path_str = path.to_string_lossy().replace('\\', "/");
    let src = format!(
        r#"fn f {{ IO String -> String  in: p  out: File.read(p) }}"#
    );
    let result = run(&src, "f", Value::Str(path_str));
    assert_eq!(result, Value::Str("hello world".to_string()));
}

#[test]
fn test_file_read_lines() {
    let dir = std::env::temp_dir();
    let path = dir.join("keln_test_file_readlines.txt");
    std::fs::write(&path, "line1\nline2\nline3").unwrap();
    let path_str = path.to_string_lossy().replace('\\', "/");
    let src = r#"fn f { IO String -> Int  in: p  out: List.length(File.readLines(p)) }"#;
    let result = run(src, "f", Value::Str(path_str));
    assert_eq!(result, Value::Int(3));
}

// =============================================================================
// VM closure lifting — named capturing helpers in bytecode
// =============================================================================

#[test]
fn test_closure_fold_captures_offset() {
    let src = r#"
fn offset_fold {
    Pure { list: List<Int>, offset: Int } -> Int
    in: args
    out:
        let step :: Pure { acc: Int, item: Int } -> Int => it.acc + it.item + args.offset in
        List.fold(args.list, 0, step)
    confidence: 1.0
    reason: "closure captures offset from outer scope"
}
"#;
    let arg = Value::make_record(&["list", "offset"], vec![
        Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
        Value::Int(10),
    ]);
    assert_both_backends(src, "offset_fold", arg, Value::Int(36));
}

#[test]
fn test_closure_fold_no_captures() {
    let src = r#"
fn plain_sum {
    Pure List<Int> -> Int
    in: list
    out:
        let step :: Pure { acc: Int, item: Int } -> Int => it.acc + it.item in
        List.fold(list, 0, step)
    confidence: 1.0
    reason: "closure with no captures"
}
"#;
    let arg = Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)]));
    assert_both_backends(src, "plain_sum", arg, Value::Int(10));
}

#[test]
fn test_closure_map_captures_factor() {
    let src = r#"
fn multiply_all {
    Pure { list: List<Int>, factor: Int } -> List<Int>
    in: args
    out:
        let mult :: Pure Int -> Int => it * args.factor in
        List.map(args.list, mult)
    confidence: 1.0
    reason: "closure captures factor for map"
}
"#;
    let arg = Value::make_record(&["list", "factor"], vec![
        Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
        Value::Int(5),
    ]);
    assert_both_backends(src, "multiply_all", arg,
        Value::List(std::rc::Rc::new(vec![Value::Int(5), Value::Int(10), Value::Int(15)])));
}

#[test]
fn test_closure_filter_captures_threshold() {
    let src = r#"
fn filter_above {
    Pure { list: List<Int>, min_val: Int } -> List<Int>
    in: args
    out:
        let pred :: Pure Int -> Bool => it > args.min_val in
        List.filter(args.list, pred)
    confidence: 1.0
    reason: "closure captures min_val for filter predicate"
}
"#;
    let arg = Value::make_record(&["list", "min_val"], vec![
        Value::List(std::rc::Rc::new(vec![
            Value::Int(1), Value::Int(5), Value::Int(3), Value::Int(7), Value::Int(2),
        ])),
        Value::Int(3),
    ]);
    assert_both_backends(src, "filter_above", arg,
        Value::List(std::rc::Rc::new(vec![Value::Int(5), Value::Int(7)])));
}

#[test]
fn test_closure_multi_capture() {
    let src = r#"
fn weighted_sum {
    Pure { list: List<Int>, base: Int, weight: Int } -> Int
    in: args
    out:
        let step :: Pure { acc: Int, item: Int } -> Int =>
            it.acc + it.item * args.weight + args.base
        in
        List.fold(args.list, 0, step)
    confidence: 1.0
    reason: "closure captures two variables"
}
"#;
    // step: 0+(1*2+5)=7 → 7+(2*2+5)=16 → 16+(3*2+5)=27
    let arg = Value::make_record(&["list", "base", "weight"], vec![
        Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
        Value::Int(5),
        Value::Int(2),
    ]);
    assert_both_backends(src, "weighted_sum", arg, Value::Int(27));
}

// =============================================================================
// List.findMap — both backends
// =============================================================================

#[test]
fn test_find_map_fnref_finds_first() {
    let src = r#"
fn doubleFirstEven {
    Pure List<Int> -> Maybe<Int>
    in: xs
    out: List.findMap(xs, tryDouble)
    helpers: {
        tryDouble :: Pure Int -> Maybe<Int> =>
            match it % 2 == 0 {
                true  -> Some(it * 2)
                false -> None
            }
    }
}
"#;
    let xs = Value::List(std::rc::Rc::new(vec![
        Value::Int(1), Value::Int(3), Value::Int(4), Value::Int(6),
    ]));
    let expected = Value::Variant {
        name: "Some".to_string(),
        payload: crate::eval::VariantPayload::Tuple(Box::new(Value::Int(8))),
    };
    assert_both_backends(src, "doubleFirstEven", xs, expected);
}

#[test]
fn test_find_map_fnref_returns_none() {
    let src = r#"
fn doubleFirstEven {
    Pure List<Int> -> Maybe<Int>
    in: xs
    out: List.findMap(xs, tryDouble)
    helpers: {
        tryDouble :: Pure Int -> Maybe<Int> =>
            match it % 2 == 0 {
                true  -> Some(it * 2)
                false -> None
            }
    }
}
"#;
    let xs = Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(3), Value::Int(5)]));
    let expected = Value::Variant {
        name: "None".to_string(),
        payload: crate::eval::VariantPayload::Unit,
    };
    assert_both_backends(src, "doubleFirstEven", xs, expected);
}

#[test]
fn test_find_map_closure_captures_context() {
    let src = r#"
fn findAbove {
    Pure { xs: List<Int>, limit: Int } -> Maybe<Int>
    in: args
    out:
        let lim = args.limit in
        let tryAbove :: Pure Int -> Maybe<Int> =>
            match it > lim {
                true  -> Some(it)
                false -> None
            }
        in
        List.findMap(args.xs, tryAbove)
}
"#;
    let input = Value::make_record(&["limit", "xs"], vec![
        Value::Int(10),
        Value::List(std::rc::Rc::new(vec![
            Value::Int(1), Value::Int(5), Value::Int(12), Value::Int(20),
        ])),
    ]);
    let expected = Value::Variant {
        name: "Some".to_string(),
        payload: crate::eval::VariantPayload::Tuple(Box::new(Value::Int(12))),
    };
    assert_both_backends(src, "findAbove", input, expected);
}

// =============================================================================
// Map.foldUntil — both backends
// =============================================================================

#[test]
fn test_map_fold_until_stops_early() {
    let src = r#"
fn sumUntil {
    Pure Map<String, Int> -> Int
    in: m
    out: Map.foldUntil(m, 0, addVal, isDone)
    helpers: {
        addVal :: Pure { acc: Int, key: String, value: Int } -> Int =>
            it.acc + it.value
        isDone :: Pure Int -> Bool =>
            it > 10
    }
}
"#;
    let mut map = std::collections::BTreeMap::new();
    map.insert(Value::Str("a".into()), Value::Int(3));
    map.insert(Value::Str("b".into()), Value::Int(5));
    map.insert(Value::Str("c".into()), Value::Int(7));
    map.insert(Value::Str("d".into()), Value::Int(9));
    // BTreeMap iterates alphabetically: a=3 (acc=3), b=5 (acc=8), c=7 (acc=15 > 10 → stop)
    assert_both_backends(src, "sumUntil", Value::Map(std::rc::Rc::new(map)), Value::Int(15));
}

#[test]
fn test_map_fold_until_full_traverse() {
    let src = r#"
fn sumAll {
    Pure Map<String, Int> -> Int
    in: m
    out: Map.foldUntil(m, 0, addVal, neverStop)
    helpers: {
        addVal :: Pure { acc: Int, key: String, value: Int } -> Int =>
            it.acc + it.value
        neverStop :: Pure Int -> Bool =>
            false
    }
}
"#;
    let mut map = std::collections::BTreeMap::new();
    map.insert(Value::Str("x".into()), Value::Int(10));
    map.insert(Value::Str("y".into()), Value::Int(20));
    map.insert(Value::Str("z".into()), Value::Int(30));
    assert_both_backends(src, "sumAll", Value::Map(std::rc::Rc::new(map)), Value::Int(60));
}

// =============================================================================
// List.mapFold — both backends
// =============================================================================

#[test]
fn test_list_map_fold_prefix_sums() {
    let src = r#"
fn prefixSums {
    Pure List<Int> -> List<Int>
    in: xs
    out: List.mapFold(xs, 0, step).result
    helpers: {
        step :: Pure {acc: Int, item: Int} -> {acc: Int, val: Int} =>
            let s = it.acc + it.item in
            {acc: s, val: s}
    }
}
"#;
    let xs = Value::List(std::rc::Rc::new(vec![
        Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4),
    ]));
    let expected = Value::List(std::rc::Rc::new(vec![
        Value::Int(1), Value::Int(3), Value::Int(6), Value::Int(10),
    ]));
    assert_both_backends(src, "prefixSums", xs, expected);
}

#[test]
fn test_list_map_fold_closure_captures() {
    // VmClosure path: step captures 'base' from outer let binding
    let src = r#"
fn scaledCumSum {
    Pure { xs: List<Int>, base: Int } -> List<Int>
    in: args
    out:
        let b = args.base in
        let step :: Pure {acc: Int, item: Int} -> {acc: Int, val: Int} =>
            let s = it.acc + it.item + b in
            {acc: s, val: s}
        in
        List.mapFold(args.xs, 0, step).result
}
"#;
    // b=10: 0+(1+10)=11, 11+(2+10)=23, 23+(3+10)=36
    let input = Value::make_record(&["base", "xs"], vec![
        Value::Int(10),
        Value::List(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
    ]);
    let expected = Value::List(std::rc::Rc::new(vec![
        Value::Int(11), Value::Int(23), Value::Int(36),
    ]));
    assert_both_backends(src, "scaledCumSum", input, expected);
}

#[test]
fn test_list_map_fold_returns_final_acc() {
    let src = r#"
fn sumAndList {
    Pure List<Int> -> Int
    in: xs
    out: List.mapFold(xs, 0, step).acc
    helpers: {
        step :: Pure {acc: Int, item: Int} -> {acc: Int, val: Int} =>
            let s = it.acc + it.item in {acc: s, val: s}
    }
}
"#;
    let xs = Value::List(std::rc::Rc::new(vec![Value::Int(5), Value::Int(10), Value::Int(15)]));
    assert_both_backends(src, "sumAndList", xs, Value::Int(30));
}

// =============================================================================
// Fix: helpers: functions visible from named capturing helpers (VM backend)
// =============================================================================

#[test]
fn test_vm_helpers_visible_from_closure() {
    let src = r#"
fn compute {
    Pure Int -> Int
    in: n
    out:
        let doubled :: Pure Int -> Int => double(it) in
        doubled(n)
    helpers: {
        double :: Pure Int -> Int => it * 2
    }
}
"#;
    assert_both_backends(src, "compute", Value::Int(5),  Value::Int(10));
    assert_both_backends(src, "compute", Value::Int(7),  Value::Int(14));
}

// =============================================================================
// Fix: let rec — recursive named capturing helpers (VM backend)
// =============================================================================

#[test]
fn test_vm_let_rec_factorial() {
    let src = r#"
fn factorial {
    Pure Int -> Int
    in: n
    out:
        let rec fact :: Pure Int -> Int =>
            match it {
                0 -> 1
                n -> n * fact(n - 1)
            }
        in
        fact(n)
}
"#;
    assert_both_backends(src, "factorial", Value::Int(0), Value::Int(1));
    assert_both_backends(src, "factorial", Value::Int(5), Value::Int(120));
    assert_both_backends(src, "factorial", Value::Int(6), Value::Int(720));
}

#[test]
fn test_vm_let_rec_captures_context() {
    let src = r#"
fn sumUpTo {
    Pure { start: Int, limit: Int } -> Int
    in: args
    out:
        let rec loop :: Pure Int -> Int =>
            match it == args.limit {
                true  -> it
                false -> it + loop(it + 1)
            }
        in
        loop(args.start)
}
"#;
    let input = Value::make_record(&["start", "limit"], vec![Value::Int(1), Value::Int(5)]);
    assert_both_backends(src, "sumUpTo", input, Value::Int(15));
}

// =============================================================================
// Fix: and/or/not as boolean expression operators (VM backend)
// =============================================================================

#[test]
fn test_vm_bool_not() {
    let src = r#"
fn testNot {
    Pure Bool -> Bool
    in: b
    out: not(b)
}
"#;
    assert_both_backends(src, "testNot", Value::Bool(true),  Value::Bool(false));
    assert_both_backends(src, "testNot", Value::Bool(false), Value::Bool(true));
}

#[test]
fn test_vm_bool_and() {
    let src = r#"
fn testAnd {
    Pure { a: Bool, b: Bool } -> Bool
    in: args
    out: and(args.a, args.b)
}
"#;
    let t = |a, b| Value::make_record(&["a", "b"], vec![Value::Bool(a), Value::Bool(b)]);
    assert_both_backends(src, "testAnd", t(true,  true),  Value::Bool(true));
    assert_both_backends(src, "testAnd", t(true,  false), Value::Bool(false));
    assert_both_backends(src, "testAnd", t(false, true),  Value::Bool(false));
}

#[test]
fn test_vm_bool_or() {
    let src = r#"
fn testOr {
    Pure { a: Bool, b: Bool } -> Bool
    in: args
    out: or(args.a, args.b)
}
"#;
    let t = |a, b| Value::make_record(&["a", "b"], vec![Value::Bool(a), Value::Bool(b)]);
    assert_both_backends(src, "testOr", t(false, false), Value::Bool(false));
    assert_both_backends(src, "testOr", t(false, true),  Value::Bool(true));
    assert_both_backends(src, "testOr", t(true,  false), Value::Bool(true));
}

// =============================================================================
// Debug.print — pass-through, prints to stderr and returns value unchanged
// =============================================================================

#[test]
fn test_vm_debug_print_int_passthrough() {
    let src = r#"
fn debugIdentity {
    IO Int -> Int
    in: n
    out: Debug.print(n)
}
"#;
    assert_both_backends(src, "debugIdentity", Value::Int(42), Value::Int(42));
}

#[test]
fn test_vm_debug_print_string_passthrough() {
    let src = r#"
fn debugStr {
    IO String -> String
    in: s
    out: Debug.print(s)
}
"#;
    assert_both_backends(src, "debugStr", Value::Str("hello".into()), Value::Str("hello".into()));
}

#[test]
fn test_vm_debug_print_inline() {
    let src = r#"
fn compute {
    IO Int -> Int
    in: n
    out:
        let x = Debug.print(n * 2) in
        x + 1
}
"#;
    assert_both_backends(src, "compute", Value::Int(5), Value::Int(11));
}

// =============================================================================
// Fix 3 — Generic T.ref parsing
// =============================================================================

#[test]
fn test_generic_type_ref_parses() {
    // Before this fix, List<String>.ref failed to parse: the parser saw `<` after
    // an upper ident and treated it as a comparison, producing a parse error.
    // After the fix, the speculative parse path handles UpperIdent<TypeArgs>.ref.
    //
    // We use `Int -> Int` to keep the signature trivial; parse() doesn't type-check,
    // so a TypeRef in an Int-typed position still gives us a clean parse-only test.
    let src = r#"fn f {
    Pure Int -> Int
    in: n
    out: List<String>.ref
}"#;
    let result = parse(src);
    assert!(result.is_ok(), "List<String>.ref should parse without error: {:?}", result.err());
}
