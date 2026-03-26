use crate::eval::{stdlib, RuntimeError, Value, VariantPayload};
use crate::vm::ir::{BuiltinTable, Constant, Frame, Instruction, KelnModule};

// =============================================================================
// Error type — wraps RuntimeError for the VM layer
// =============================================================================

#[derive(Debug)]
pub struct ExecError {
    pub message: String,
}

impl ExecError {
    pub fn new(msg: impl Into<String>) -> Self {
        ExecError { message: msg.into() }
    }
}

impl From<RuntimeError> for ExecError {
    fn from(e: RuntimeError) -> Self {
        ExecError { message: e.message }
    }
}

// =============================================================================
// Public entry point
// =============================================================================

/// Parse, lower, and execute a named function from source.
/// This is the VM-backend equivalent of `eval::eval_fn`.
pub fn execute_fn(module: &KelnModule, fn_name: &str, arg: Value) -> Result<Value, ExecError> {
    let fn_idx = module
        .fn_idx(fn_name)
        .ok_or_else(|| ExecError::new(format!("function '{}' not found in module", fn_name)))?;
    execute(module, fn_idx, arg)
}

// =============================================================================
// Core interpreter loop
// =============================================================================

/// Execute a single KelnFn by index.
///
/// - `TAIL_CALL` resets the frame in-place; no Rust stack growth.
/// - `CALL` recurses via Rust call stack (explicit Vec<CallFrame> in 4b follow-up).
fn execute(module: &KelnModule, fn_idx: usize, arg: Value) -> Result<Value, ExecError> {
    let mut current_fn = fn_idx;
    let mut frame = Frame::new(module.fns[current_fn].register_count);
    frame.write(0, arg);
    let mut ip = 0usize;

    loop {
        let instr = &module.fns[current_fn].instructions[ip];
        match instr {
            // ------------------------------------------------------------------
            // Tail call — frame reset; no Rust stack growth
            // ------------------------------------------------------------------
            Instruction::TailCall { fn_idx: target, arg_reg } => {
                let new_arg = frame.take(*arg_reg)?;
                current_fn = *target;
                frame = Frame::new(module.fns[current_fn].register_count);
                frame.write(0, new_arg);
                ip = 0;
            }

            // ------------------------------------------------------------------
            // Return — move src; destroy frame
            // ------------------------------------------------------------------
            Instruction::Return { src } => {
                return Ok(frame.take(*src)?);
            }

            // ------------------------------------------------------------------
            // Non-tail call — Rust stack recursion
            // ------------------------------------------------------------------
            Instruction::Call { dst, fn_idx: target, arg_reg } => {
                let arg = frame.clone_reg(*arg_reg)?;
                let result = execute(module, *target, arg)?;
                frame.write(*dst, result);
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Builtin call — delegate to stdlib::dispatch via name lookup
            // ------------------------------------------------------------------
            Instruction::CallBuiltin { dst, builtin, args } => {
                let builtin_name = BuiltinTable::name_of(*builtin);
                let vals: Vec<Value> = args
                    .iter()
                    .map(|r| frame.clone_reg(*r))
                    .collect::<Result<Vec<_>, _>>()?;
                // stdlib::dispatch takes (&str, Vec<Value>, &mut Evaluator)
                // We have no Evaluator here, so use a shim that covers pure builtins.
                let result = dispatch_builtin(builtin_name, vals)?;
                frame.write(*dst, result);
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Dynamic (FnRef) calls
            // ------------------------------------------------------------------
            Instruction::TailCallDyn { fn_reg, arg_reg } => {
                let fn_val = frame.clone_reg(*fn_reg)?;
                let new_arg = frame.take(*arg_reg)?;
                let name = fn_ref_name(&fn_val)?;
                let target = module
                    .fn_idx(&name)
                    .ok_or_else(|| ExecError::new(format!("TailCallDyn: unknown fn '{}'", name)))?;
                current_fn = target;
                frame = Frame::new(module.fns[current_fn].register_count);
                frame.write(0, new_arg);
                ip = 0;
            }

            Instruction::CallDyn { dst, fn_reg, arg_reg } => {
                let fn_val = frame.clone_reg(*fn_reg)?;
                let arg = frame.clone_reg(*arg_reg)?;
                let name = fn_ref_name(&fn_val)?;
                let target = module
                    .fn_idx(&name)
                    .ok_or_else(|| ExecError::new(format!("CallDyn: unknown fn '{}'", name)))?;
                let result = execute(module, target, arg)?;
                frame.write(*dst, result);
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Load instructions
            // ------------------------------------------------------------------
            Instruction::LoadInt   { dst, val } => { frame.write(*dst, Value::Int(*val));   ip += 1; }
            Instruction::LoadFloat { dst, val } => { frame.write(*dst, Value::Float(*val)); ip += 1; }
            Instruction::LoadBool  { dst, val } => { frame.write(*dst, Value::Bool(*val));  ip += 1; }
            Instruction::LoadUnit  { dst }      => { frame.write(*dst, Value::Unit);        ip += 1; }
            Instruction::LoadStr   { dst, const_idx } => {
                if let Some(Constant::Str(s)) = module.constants.entries.get(*const_idx as usize) {
                    frame.write(*dst, Value::Str(s.clone()));
                } else {
                    return Err(ExecError::new(format!("LoadStr: invalid const_idx {}", const_idx)));
                }
                ip += 1;
            }
            Instruction::LoadReg { dst, src } => {
                let v = frame.clone_reg(*src)?;
                frame.write(*dst, v);
                ip += 1;
            }
            Instruction::LoadFnRef { dst, name } => {
                frame.write(*dst, Value::FnRef(name.clone()));
                ip += 1;
            }
            Instruction::Clone { dst, src } => {
                let v = frame.clone_reg(*src)?;
                frame.write(*dst, v);
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Arithmetic (clone both sources; type mismatch → ExecError)
            // ------------------------------------------------------------------
            Instruction::Add { dst, src1, src2 } => {
                let v = arith_op(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?, |a,b| a+b, |a,b| a+b)?;
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Sub { dst, src1, src2 } => {
                let v = arith_op(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?, |a,b| a-b, |a,b| a-b)?;
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Mul { dst, src1, src2 } => {
                let v = arith_op(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?, |a,b| a*b, |a,b| a*b)?;
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Div { dst, src1, src2 } => {
                let v = arith_div(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?)?;
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Rem { dst, src1, src2 } => {
                let v = arith_rem(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?)?;
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Neg { dst, src } => {
                let v = match frame.clone_reg(*src)? {
                    Value::Int(n)   => Value::Int(-n),
                    Value::Float(f) => Value::Float(-f),
                    other => return Err(ExecError::new(format!("NEG: expected number, got {}", other))),
                };
                frame.write(*dst, v); ip += 1;
            }

            // ------------------------------------------------------------------
            // Comparison (clone both; result Bool)
            // ------------------------------------------------------------------
            Instruction::Eq { dst, src1, src2 } => {
                let v = Value::Bool(frame.clone_reg(*src1)? == frame.clone_reg(*src2)?);
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Ne { dst, src1, src2 } => {
                let v = Value::Bool(frame.clone_reg(*src1)? != frame.clone_reg(*src2)?);
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Lt { dst, src1, src2 } => {
                let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? < 0);
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Le { dst, src1, src2 } => {
                let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? <= 0);
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Gt { dst, src1, src2 } => {
                let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? > 0);
                frame.write(*dst, v); ip += 1;
            }
            Instruction::Ge { dst, src1, src2 } => {
                let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? >= 0);
                frame.write(*dst, v); ip += 1;
            }

            // ------------------------------------------------------------------
            // Record construction
            // ------------------------------------------------------------------
            Instruction::MakeRecord { dst, layout_idx, fields } => {
                let field_names = module
                    .layouts
                    .fields_of(*layout_idx)
                    .ok_or_else(|| ExecError::new(format!("MakeRecord: unknown layout {}", layout_idx)))?
                    .clone();
                let mut record: Vec<(String, Value)> = Vec::with_capacity(field_names.len());
                for (name, reg) in field_names.into_iter().zip(fields.iter()) {
                    record.push((name, frame.clone_reg(*reg)?));
                }
                frame.write(*dst, Value::Record(record));
                ip += 1;
            }

            Instruction::FieldGet { dst, src, field_idx } => {
                let rec = frame.clone_reg(*src)?;
                let v = match &rec {
                    Value::Record(fields) => fields
                        .get(*field_idx)
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| ExecError::new(format!("FIELD_GET: index {} out of range", field_idx)))?,
                    other => return Err(ExecError::new(format!("FIELD_GET: expected record, got {}", other))),
                };
                frame.write(*dst, v);
                ip += 1;
            }

            Instruction::FieldGetNamed { dst, src, name_idx } => {
                let rec = frame.clone_reg(*src)?;
                let field_name = match module.constants.entries.get(*name_idx as usize) {
                    Some(Constant::Str(s)) => s.clone(),
                    _ => return Err(ExecError::new(format!("FIELD_GET_NAMED: invalid name_idx {}", name_idx))),
                };
                let v = match &rec {
                    Value::Record(fields) => fields
                        .iter()
                        .find(|(k, _)| k == &field_name)
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| ExecError::new(format!("FIELD_GET_NAMED: field '{}' not found", field_name)))?,
                    other => return Err(ExecError::new(format!("FIELD_GET_NAMED: expected record, got {}", other))),
                };
                frame.write(*dst, v);
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Variant construction and destructuring
            // ------------------------------------------------------------------
            Instruction::MakeVariant { dst, tag_id, payload } => {
                let name = module.tags.name_of(*tag_id).to_string();
                let vp = match payload {
                    None => VariantPayload::Unit,
                    Some(reg) => VariantPayload::Tuple(Box::new(frame.clone_reg(*reg)?)),
                };
                frame.write(*dst, Value::Variant { name, payload: vp });
                ip += 1;
            }

            Instruction::VariantPayload { dst, src } => {
                let val = frame.clone_reg(*src)?;
                let payload = match val {
                    Value::Variant { payload: VariantPayload::Tuple(v), .. } => *v,
                    Value::Variant { payload: VariantPayload::Record(f), .. } => Value::Record(f),
                    Value::Variant { payload: VariantPayload::Unit, name } =>
                        return Err(ExecError::new(format!("VARIANT_PAYLOAD: '{}' has no payload", name))),
                    other =>
                        return Err(ExecError::new(format!("VARIANT_PAYLOAD: expected variant, got {}", other))),
                };
                frame.write(*dst, payload);
                ip += 1;
            }

            // ------------------------------------------------------------------
            // List construction
            // ------------------------------------------------------------------
            Instruction::MakeList { dst, items } => {
                let vals: Vec<Value> = items
                    .iter()
                    .map(|r| frame.clone_reg(*r))
                    .collect::<Result<_, _>>()?;
                frame.write(*dst, Value::List(vals));
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Pattern matching — conditional jumps
            // ------------------------------------------------------------------
            Instruction::MatchTagEq { tag_id, src, target_ip } => {
                let val = frame.clone_reg(*src)?;
                let matches = match &val {
                    Value::Variant { name, .. } => {
                        module.tags.lookup(name) == Some(*tag_id)
                    }
                    _ => false,
                };
                ip = if matches { *target_ip } else { ip + 1 };
            }

            Instruction::MatchLitEq { const_idx, src, target_ip } => {
                let val = frame.clone_reg(*src)?;
                let expected = module
                    .constants
                    .entries
                    .get(*const_idx as usize)
                    .map(|c| c.to_value())
                    .ok_or_else(|| ExecError::new(format!("MATCH_LIT_EQ: invalid const_idx {}", const_idx)))?;
                ip = if val == expected { *target_ip } else { ip + 1 };
            }

            // ------------------------------------------------------------------
            // Unconditional jump
            // ------------------------------------------------------------------
            Instruction::Jump { target_ip } => {
                ip = *target_ip;
            }

            // ------------------------------------------------------------------
            // Channel operations (sync model)
            // ------------------------------------------------------------------
            Instruction::ChanNew { dst } => {
                use std::cell::RefCell;
                use std::collections::VecDeque;
                use std::rc::Rc;
                let ch = Rc::new(RefCell::new(VecDeque::new()));
                frame.write(*dst, Value::Channel(ch));
                ip += 1;
            }

            Instruction::ChanSend { chan_reg, val_reg } => {
                let val = frame.take(*val_reg)?;
                let chan = frame.clone_reg(*chan_reg)?;
                match chan {
                    Value::Channel(rc) => rc.borrow_mut().push_back(val),
                    other => return Err(ExecError::new(format!("CHAN_SEND: expected channel, got {}", other))),
                }
                ip += 1;
            }

            Instruction::ChanRecv { dst, chan_reg } => {
                let chan = frame.clone_reg(*chan_reg)?;
                let val = match &chan {
                    Value::Channel(rc) => {
                        rc.borrow_mut().pop_front().map(|v| {
                            Value::Variant {
                                name: "Some".to_string(),
                                payload: VariantPayload::Tuple(Box::new(v)),
                            }
                        }).unwrap_or(Value::Variant {
                            name: "None".to_string(),
                            payload: VariantPayload::Unit,
                        })
                    }
                    other => return Err(ExecError::new(format!("CHAN_RECV: expected channel, got {}", other))),
                };
                frame.write(*dst, val);
                ip += 1;
            }

            Instruction::ChanClose { chan_reg } => {
                // Sync model: no-op (channel is just a VecDeque; no close signal)
                let _ = frame.clone_reg(*chan_reg)?;
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Select (sync model: poll first ready arm; timeout as fallback)
            // ------------------------------------------------------------------
            Instruction::Select { dst, arms, timeout } => {
                let mut selected: Option<Value> = None;
                for arm in arms {
                    let chan = frame.clone_reg(arm.channel_reg)?;
                    if let Value::Channel(rc) = &chan {
                        if let Some(v) = rc.borrow_mut().pop_front() {
                            // Bind the received value and jump to arm body
                            frame.write(*dst, v);
                            selected = Some(Value::Unit);
                            // Execute the arm body inline by adjusting ip
                            ip = arm.body_ip;
                            break;
                        }
                    }
                }
                if selected.is_none() {
                    // No arm ready: use timeout fallback or return Unit
                    if let Some(t) = timeout {
                        ip = t.body_ip;
                    } else {
                        frame.write(*dst, Value::Unit);
                        ip += 1;
                    }
                }
            }

            // ------------------------------------------------------------------
            // Partial application
            // ------------------------------------------------------------------
            Instruction::MakePartial { dst, fn_reg, bound_reg } => {
                let fn_val = frame.clone_reg(*fn_reg)?;
                let bound_val = frame.clone_reg(*bound_reg)?;
                let name = fn_ref_name(&fn_val)?;
                let bound = match bound_val {
                    Value::Record(fields) => fields,
                    other => vec![("_0".to_string(), other)],
                };
                frame.write(*dst, Value::PartialFn { name, bound });
                ip += 1;
            }

            // ------------------------------------------------------------------
            // Refinement checks (no-op in release; check in debug)
            // ------------------------------------------------------------------
            Instruction::CheckRange { src, lo, hi } => {
                if let Value::Int(n) = frame.clone_reg(*src)? {
                    if n < *lo || n > *hi {
                        return Err(ExecError::new(format!(
                            "CHECK_RANGE: {} not in {}..{}", n, lo, hi
                        )));
                    }
                }
                ip += 1;
            }
            Instruction::CheckRangeF { src, lo, hi } => {
                if let Value::Float(f) = frame.clone_reg(*src)? {
                    if f < *lo || f > *hi {
                        return Err(ExecError::new(format!(
                            "CHECK_RANGE_F: {} not in {}..{}", f, lo, hi
                        )));
                    }
                }
                ip += 1;
            }
            Instruction::CheckCmp { .. } | Instruction::CheckLen { .. } => {
                ip += 1; // TODO: implement in follow-up
            }
        }
    }
}

// =============================================================================
// Builtin dispatch — delegate to existing stdlib via name
// =============================================================================

fn dispatch_builtin(name: &str, args: Vec<Value>) -> Result<Value, ExecError> {
    // The stdlib requires an Evaluator for IO/effects, but pure builtins work
    // with a no-op evaluator. For Phase 4b we call into the stdlib directly.
    // IO-effect builtins (Http, Task, Channel) are handled separately when needed.
    let mut eval = crate::eval::Evaluator::new();
    stdlib::dispatch(name, args, &mut eval)
        .map_err(|e| ExecError::new(e.message))
}

// =============================================================================
// Arithmetic helpers
// =============================================================================

fn arith_op<Fi, Ff>(a: Value, b: Value, fi: Fi, ff: Ff) -> Result<Value, ExecError>
where
    Fi: Fn(i64, i64) -> i64,
    Ff: Fn(f64, f64) -> f64,
{
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(Value::Int(fi(x, y))),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(ff(x, y))),
        (a, b) => Err(ExecError::new(format!("arithmetic type mismatch: {} and {}", a, b))),
    }
}

fn arith_div(a: Value, b: Value) -> Result<Value, ExecError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if y == 0 { Err(ExecError::new("division by zero")) }
            else { Ok(Value::Int(x / y)) }
        }
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x / y)),
        (a, b) => Err(ExecError::new(format!("division type mismatch: {} and {}", a, b))),
    }
}

fn arith_rem(a: Value, b: Value) -> Result<Value, ExecError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if y == 0 { Err(ExecError::new("modulo by zero")) }
            else { Ok(Value::Int(x % y)) }
        }
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x % y)),
        (a, b) => Err(ExecError::new(format!("modulo type mismatch: {} and {}", a, b))),
    }
}

fn cmp_values(a: &Value, b: &Value) -> Result<i32, ExecError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(x.cmp(y) as i32),
        (Value::Float(x), Value::Float(y)) => Ok(x.partial_cmp(y).map(|o| o as i32).unwrap_or(0)),
        (Value::Str(x), Value::Str(y))     => Ok(x.cmp(y) as i32),
        (Value::Bool(x), Value::Bool(y))   => Ok(x.cmp(y) as i32),
        _ => Err(ExecError::new(format!("comparison not supported between {} and {}", a, b))),
    }
}

fn fn_ref_name(v: &Value) -> Result<String, ExecError> {
    match v {
        Value::FnRef(name) => Ok(name.clone()),
        Value::PartialFn { name, .. } => Ok(name.clone()),
        other => Err(ExecError::new(format!("CallDyn: expected FnRef, got {}", other))),
    }
}

