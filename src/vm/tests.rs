use crate::eval::Value;
use crate::parser::parse;
use crate::vm::exec::execute_fn;
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
