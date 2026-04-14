use std::fmt;
use std::rc::Rc;
use crate::eval::{stdlib, ChannelInner, RuntimeError, Value, VariantPayload};
use crate::vm::ir::{BuiltinTable, CallFrame, Constant, Frame, Instruction, KelnModule};

// =============================================================================
// Error type — wraps RuntimeError for the VM layer
// =============================================================================

#[derive(Debug)]
pub struct ExecError {
    pub message: String,
    pub stack_trace: Vec<String>,
}

impl ExecError {
    pub fn new(msg: impl Into<String>) -> Self {
        ExecError { message: msg.into(), stack_trace: Vec::new() }
    }
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.stack_trace.is_empty() {
            write!(f, "\nstack trace:")?;
            for frame in &self.stack_trace {
                write!(f, "\n{}", frame)?;
            }
        }
        Ok(())
    }
}

impl From<RuntimeError> for ExecError {
    fn from(e: RuntimeError) -> Self {
        ExecError { message: e.message, stack_trace: Vec::new() }
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
// Stack-trace builder
// =============================================================================

fn build_trace(call_stack: &[CallFrame], current_fn: usize, ip: usize, module: &KelnModule) -> Vec<String> {
    let mut trace = Vec::new();
    if let Some(f) = module.fns.get(current_fn) {
        trace.push(format!("  at {} (ip={})", f.name, ip));
    }
    for caller in call_stack.iter().rev() {
        if let Some(f) = module.fns.get(caller.fn_idx) {
            trace.push(format!("  at {} (ip={})", f.name, caller.ip.saturating_sub(1)));
        }
    }
    trace
}

// =============================================================================
// Core interpreter loop — explicit Vec<CallFrame> call stack
// =============================================================================

/// Execute a single KelnFn by index using an explicit call stack.
///
/// - `TAIL_CALL` resets the frame in-place (no stack growth).
/// - `CALL` pushes a `CallFrame` to `call_stack` (no Rust stack growth).
/// - `RETURN` pops from `call_stack`; returns to caller when stack is empty.
fn execute(module: &KelnModule, fn_idx: usize, arg: Value) -> Result<Value, ExecError> {
    let mut call_stack: Vec<CallFrame> = Vec::new();
    let mut current_fn = fn_idx;
    let mut frame = Frame::new(module.fns[current_fn].register_count);
    frame.write(0, arg);
    let mut ip = 0usize;

    loop {
        match exec_step(module, &mut call_stack, &mut current_fn, &mut frame, &mut ip) {
            Ok(None) => {}
            Ok(Some(v)) => return Ok(v),
            Err(e) => {
                let trace = build_trace(&call_stack, current_fn, ip, module);
                return Err(ExecError { message: e.message, stack_trace: trace });
            }
        }
    }
}

/// Execute one instruction, mutating interpreter state in-place.
///
/// Returns:
/// - `Ok(None)`    — continue to next instruction (ip already updated)
/// - `Ok(Some(v))` — final return value (call_stack was empty on RETURN)
/// - `Err(e)`      — runtime error (stack_trace not yet populated)
fn exec_step(
    module: &KelnModule,
    call_stack: &mut Vec<CallFrame>,
    current_fn: &mut usize,
    frame: &mut Frame,
    ip: &mut usize,
) -> Result<Option<Value>, ExecError> {
    let instr = &module.fns[*current_fn].instructions[*ip];
    match instr {
        // ------------------------------------------------------------------
        // Tail call — frame reset; no stack growth
        // ------------------------------------------------------------------
        Instruction::TailCall { fn_idx: target, arg_reg } => {
            let new_arg = frame.take(*arg_reg)?;
            *current_fn = *target;
            *frame = Frame::new(module.fns[*current_fn].register_count);
            frame.write(0, new_arg);
            *ip = 0;
        }

        // ------------------------------------------------------------------
        // Return — pop call_stack; final return when stack is empty
        // ------------------------------------------------------------------
        Instruction::Return { src } => {
            let result = frame.take(*src)?;
            return match call_stack.pop() {
                None => Ok(Some(result)),
                Some(caller) => {
                    *current_fn = caller.fn_idx;
                    *ip = caller.ip;
                    *frame = caller.frame;
                    frame.write(caller.dst, result);
                    Ok(None)
                }
            };
        }

        // ------------------------------------------------------------------
        // Non-tail call — push caller state to explicit call_stack
        // ------------------------------------------------------------------
        Instruction::Call { dst, fn_idx: target, arg_reg } => {
            let arg = frame.clone_reg(*arg_reg)?;
            let target = *target;
            let dst = *dst;
            let old_frame = std::mem::replace(frame, Frame::new(module.fns[target].register_count));
            call_stack.push(CallFrame { fn_idx: *current_fn, ip: *ip + 1, frame: old_frame, dst });
            *current_fn = target;
            frame.write(0, arg);
            *ip = 0;
        }

        // ------------------------------------------------------------------
        // Builtin call — delegate to stdlib::dispatch via name lookup.
        // Higher-order list operations that receive a user FnRef are handled
        // directly so they can dispatch back into the bytecode VM.
        // ------------------------------------------------------------------
        Instruction::CallBuiltin { dst, builtin, args } => {
            let builtin_name = BuiltinTable::name_of(*builtin);
            let vals: Vec<Value> = args
                .iter()
                .map(|r| frame.clone_reg(*r))
                .collect::<Result<Vec<_>, _>>()?;
            let result = exec_builtin_with_module(module, builtin_name, vals)?;
            frame.write(*dst, result);
            *ip += 1;
        }

        // ------------------------------------------------------------------
        // Dynamic (FnRef) calls
        // ------------------------------------------------------------------
        Instruction::TailCallDyn { fn_reg, arg_reg } => {
            let fn_val = frame.clone_reg(*fn_reg)?;
            let new_arg = frame.take(*arg_reg)?;
            match fn_val {
                Value::VmClosure { fn_idx, captures } => {
                    let merged = build_closure_call_arg(new_arg, &captures);
                    *current_fn = fn_idx;
                    *frame = Frame::new(module.fns[*current_fn].register_count);
                    frame.write(0, merged);
                    *ip = 0;
                }
                _ => {
                    let name = fn_ref_name(&fn_val)?;
                    let target = module
                        .fn_idx(&name)
                        .ok_or_else(|| ExecError::new(format!("TailCallDyn: unknown fn '{}'", name)))?;
                    *current_fn = target;
                    *frame = Frame::new(module.fns[*current_fn].register_count);
                    frame.write(0, new_arg);
                    *ip = 0;
                }
            }
        }

        Instruction::CallDyn { dst, fn_reg, arg_reg } => {
            let fn_val = frame.clone_reg(*fn_reg)?;
            let arg = frame.clone_reg(*arg_reg)?;
            let dst = *dst;
            match fn_val {
                Value::VmClosure { fn_idx, captures } => {
                    let merged = build_closure_call_arg(arg, &captures);
                    let old_frame = std::mem::replace(frame, Frame::new(module.fns[fn_idx].register_count));
                    call_stack.push(CallFrame { fn_idx: *current_fn, ip: *ip + 1, frame: old_frame, dst });
                    *current_fn = fn_idx;
                    frame.write(0, merged);
                    *ip = 0;
                }
                _ => {
                    let name = fn_ref_name(&fn_val)?;
                    let target = module
                        .fn_idx(&name)
                        .ok_or_else(|| ExecError::new(format!("CallDyn: unknown fn '{}'", name)))?;
                    let old_frame = std::mem::replace(frame, Frame::new(module.fns[target].register_count));
                    call_stack.push(CallFrame { fn_idx: *current_fn, ip: *ip + 1, frame: old_frame, dst });
                    *current_fn = target;
                    frame.write(0, arg);
                    *ip = 0;
                }
            }
        }

        // ------------------------------------------------------------------
        // Load instructions
        // ------------------------------------------------------------------
        Instruction::LoadInt   { dst, val } => { frame.write(*dst, Value::Int(*val));   *ip += 1; }
        Instruction::LoadFloat { dst, val } => { frame.write(*dst, Value::Float(*val)); *ip += 1; }
        Instruction::LoadBool  { dst, val } => { frame.write(*dst, Value::Bool(*val));  *ip += 1; }
        Instruction::LoadUnit  { dst }      => { frame.write(*dst, Value::Unit);        *ip += 1; }
        Instruction::LoadStr   { dst, const_idx } => {
            if let Some(Constant::Str(s)) = module.constants.entries.get(*const_idx as usize) {
                frame.write(*dst, Value::Str(s.clone()));
            } else {
                return Err(ExecError::new(format!("LoadStr: invalid const_idx {}", const_idx)));
            }
            *ip += 1;
        }
        Instruction::LoadReg { dst, src } => {
            let v = frame.clone_reg(*src)?;
            frame.write(*dst, v);
            *ip += 1;
        }
        Instruction::LoadFnRef { dst, name } => {
            frame.write(*dst, Value::FnRef(name.clone()));
            *ip += 1;
        }
        Instruction::Clone { dst, src } => {
            let v = frame.clone_reg(*src)?;
            frame.write(*dst, v);
            *ip += 1;
        }

        // ------------------------------------------------------------------
        // Arithmetic (clone both sources; type mismatch → ExecError)
        // ------------------------------------------------------------------
        Instruction::Add { dst, src1, src2 } => {
            let v = arith_op(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?, |a,b| a+b, |a,b| a+b)?;
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Sub { dst, src1, src2 } => {
            let v = arith_op(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?, |a,b| a-b, |a,b| a-b)?;
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Mul { dst, src1, src2 } => {
            let v = arith_op(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?, |a,b| a*b, |a,b| a*b)?;
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Div { dst, src1, src2 } => {
            let v = arith_div(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?)?;
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Rem { dst, src1, src2 } => {
            let v = arith_rem(frame.clone_reg(*src1)?, frame.clone_reg(*src2)?)?;
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Neg { dst, src } => {
            let v = match frame.clone_reg(*src)? {
                Value::Int(n)   => Value::Int(-n),
                Value::Float(f) => Value::Float(-f),
                other => return Err(ExecError::new(format!("NEG: expected number, got {}", other))),
            };
            frame.write(*dst, v); *ip += 1;
        }

        // ------------------------------------------------------------------
        // Comparison (clone both; result Bool)
        // ------------------------------------------------------------------
        Instruction::Eq { dst, src1, src2 } => {
            let v = Value::Bool(frame.clone_reg(*src1)? == frame.clone_reg(*src2)?);
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Ne { dst, src1, src2 } => {
            let v = Value::Bool(frame.clone_reg(*src1)? != frame.clone_reg(*src2)?);
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Lt { dst, src1, src2 } => {
            let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? < 0);
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Le { dst, src1, src2 } => {
            let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? <= 0);
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Gt { dst, src1, src2 } => {
            let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? > 0);
            frame.write(*dst, v); *ip += 1;
        }
        Instruction::Ge { dst, src1, src2 } => {
            let v = Value::Bool(cmp_values(&frame.clone_reg(*src1)?, &frame.clone_reg(*src2)?)? >= 0);
            frame.write(*dst, v); *ip += 1;
        }

        // ------------------------------------------------------------------
        // Record construction
        // ------------------------------------------------------------------
        Instruction::MakeRecord { dst, layout_idx, fields } => {
            let field_names = module
                .layouts
                .fields_of(*layout_idx)
                .ok_or_else(|| ExecError::new(format!("MakeRecord: unknown layout {}", layout_idx)))?;
            let global_idx = crate::eval::intern_layout(field_names);
            let values: Vec<Value> = fields
                .iter()
                .map(|r| frame.clone_reg(*r))
                .collect::<Result<_, _>>()?;
            frame.write(*dst, Value::Record(global_idx, values));
            *ip += 1;
        }

        Instruction::FieldGet { dst, src, field_idx } => {
            let v = match frame.read(*src)? {
                Value::Record(_, values) => values
                    .get(*field_idx)
                    .cloned()
                    .ok_or_else(|| ExecError::new(format!("FIELD_GET: index {} out of range", field_idx)))?,
                other => return Err(ExecError::new(format!("FIELD_GET: expected record, got {}", other))),
            };
            frame.write(*dst, v);
            *ip += 1;
        }

        Instruction::FieldGetNamed { dst, src, name_idx } => {
            let field_name = match module.constants.entries.get(*name_idx as usize) {
                Some(Constant::Str(s)) => s.as_str(),
                _ => return Err(ExecError::new(format!("FIELD_GET_NAMED: invalid name_idx {}", name_idx))),
            };
            let v = match frame.read(*src)? {
                Value::Record(layout, values) => {
                    let pos = crate::eval::field_pos(*layout, field_name)
                        .ok_or_else(|| ExecError::new(format!("FIELD_GET_NAMED: field '{}' not found", field_name)))?;
                    values.get(pos).cloned()
                        .ok_or_else(|| ExecError::new(format!("FIELD_GET_NAMED: index {} out of range", pos)))?
                },
                other => return Err(ExecError::new(format!("FIELD_GET_NAMED: expected record, got {}", other))),
            };
            frame.write(*dst, v);
            *ip += 1;
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
            *ip += 1;
        }

        Instruction::VariantPayload { dst, src } => {
            let val = frame.clone_reg(*src)?;
            let payload = match val {
                Value::Variant { payload: VariantPayload::Tuple(v), .. } => *v,
                Value::Variant { payload: VariantPayload::Record(l, v), .. } => Value::Record(l, v),
                Value::Variant { payload: VariantPayload::Unit, name } =>
                    return Err(ExecError::new(format!("VARIANT_PAYLOAD: '{}' has no payload", name))),
                other =>
                    return Err(ExecError::new(format!("VARIANT_PAYLOAD: expected variant, got {}", other))),
            };
            frame.write(*dst, payload);
            *ip += 1;
        }

        // ------------------------------------------------------------------
        // List construction
        // ------------------------------------------------------------------
        Instruction::MakeList { dst, items } => {
            let vals: Vec<Value> = items
                .iter()
                .map(|r| frame.clone_reg(*r))
                .collect::<Result<_, _>>()?;
            frame.write(*dst, Value::List(Rc::new(vals)));
            *ip += 1;
        }

        // ------------------------------------------------------------------
        // Pattern matching — conditional jumps
        // ------------------------------------------------------------------
        Instruction::MatchTagEq { tag_id, src, target_ip } => {
            let val = frame.clone_reg(*src)?;
            let matches = match &val {
                Value::Variant { name, .. } => module.tags.lookup(name) == Some(*tag_id),
                _ => false,
            };
            *ip = if matches { *target_ip } else { *ip + 1 };
        }

        Instruction::MatchLitEq { const_idx, src, target_ip } => {
            let val = frame.clone_reg(*src)?;
            let expected = module
                .constants
                .entries
                .get(*const_idx as usize)
                .map(|c| c.to_value())
                .ok_or_else(|| ExecError::new(format!("MATCH_LIT_EQ: invalid const_idx {}", const_idx)))?;
            *ip = if val == expected { *target_ip } else { *ip + 1 };
        }

        // ------------------------------------------------------------------
        // Unconditional jump
        // ------------------------------------------------------------------
        Instruction::Jump { target_ip } => {
            *ip = *target_ip;
        }

        // ------------------------------------------------------------------
        // Channel operations (sync model)
        // ------------------------------------------------------------------
        Instruction::ChanNew { dst } => {
            use std::cell::RefCell;
            use std::rc::Rc;
            let ch = Rc::new(RefCell::new(ChannelInner::new()));
            frame.write(*dst, Value::Channel(ch));
            *ip += 1;
        }

        Instruction::ChanNewCloseable { dst } => {
            use std::cell::RefCell;
            use std::rc::Rc;
            let ch = Rc::new(RefCell::new(ChannelInner::new_closeable()));
            frame.write(*dst, Value::Channel(ch));
            *ip += 1;
        }

        Instruction::ChanSend { chan_reg, val_reg } => {
            let val = frame.take(*val_reg)?;
            let chan = frame.clone_reg(*chan_reg)?;
            match chan {
                Value::Channel(rc) => {
                    let mut inner = rc.borrow_mut();
                    if inner.closed {
                        return Err(ExecError::new("CHAN_SEND: channel is closed"));
                    }
                    inner.queue.push_back(val);
                }
                other => return Err(ExecError::new(format!("CHAN_SEND: expected channel, got {}", other))),
            }
            *ip += 1;
        }

        Instruction::ChanRecv { dst, chan_reg } => {
            let chan = frame.clone_reg(*chan_reg)?;
            let val = match &chan {
                Value::Channel(rc) => {
                    let mut inner = rc.borrow_mut();
                    if inner.closed {
                        return Err(ExecError::new("CHAN_RECV: channel is closed"));
                    }
                    inner.queue.pop_front()
                        .ok_or_else(|| ExecError::new("CHAN_RECV: channel is empty"))?
                }
                other => return Err(ExecError::new(format!("CHAN_RECV: expected channel, got {}", other))),
            };
            frame.write(*dst, val);
            *ip += 1;
        }

        Instruction::ChanRecvMaybe { dst, chan_reg } => {
            let chan = frame.clone_reg(*chan_reg)?;
            let val = match &chan {
                Value::Channel(rc) => {
                    let mut inner = rc.borrow_mut();
                    match inner.queue.pop_front() {
                        Some(v) => Value::Variant {
                            name: "Some".to_string(),
                            payload: VariantPayload::Tuple(Box::new(v)),
                        },
                        None if inner.closed => Value::Variant {
                            name: "None".to_string(),
                            payload: VariantPayload::Unit,
                        },
                        None => return Err(ExecError::new("CHAN_RECV_MAYBE: channel is open and empty")),
                    }
                }
                other => return Err(ExecError::new(format!("CHAN_RECV_MAYBE: expected channel, got {}", other))),
            };
            frame.write(*dst, val);
            *ip += 1;
        }

        Instruction::ChanClose { chan_reg } => {
            let chan = frame.clone_reg(*chan_reg)?;
            match chan {
                Value::Channel(rc) => {
                    let mut inner = rc.borrow_mut();
                    if !inner.closeable {
                        return Err(ExecError::new("CHAN_CLOSE: channel was not created with Channel.newCloseable"));
                    }
                    inner.closed = true;
                }
                other => return Err(ExecError::new(format!("CHAN_CLOSE: expected channel, got {}", other))),
            }
            *ip += 1;
        }

        // ------------------------------------------------------------------
        // Select (sync model: poll first ready arm; timeout as fallback)
        // ------------------------------------------------------------------
        Instruction::Select { dst, arms, timeout } => {
            let mut selected = false;
            for arm in arms {
                let chan = frame.clone_reg(arm.channel_reg)?;
                if let Value::Channel(rc) = &chan
                    && let Some(v) = rc.borrow_mut().queue.pop_front()
                {
                    if arm.binding_reg != 0 {
                        frame.write(arm.binding_reg, v);
                    }
                    selected = true;
                    *ip = arm.body_ip;
                    break;
                }
            }
            if !selected {
                if let Some(t) = timeout {
                    *ip = t.body_ip;
                } else {
                    frame.write(*dst, Value::Unit);
                    *ip += 1;
                }
            }
        }

        // ------------------------------------------------------------------
        // VM closure construction
        // ------------------------------------------------------------------
        Instruction::MakeClosure { dst, fn_idx, capture_regs } => {
            let mut captures = Vec::with_capacity(capture_regs.len());
            for (name, reg) in capture_regs {
                captures.push((name.clone(), frame.clone_reg(*reg)?));
            }
            frame.write(*dst, Value::VmClosure { fn_idx: *fn_idx, captures });
            *ip += 1;
        }

        // ------------------------------------------------------------------
        // Partial application
        // ------------------------------------------------------------------
        Instruction::MakePartial { dst, fn_reg, bound_reg } => {
            let fn_val = frame.clone_reg(*fn_reg)?;
            let bound_val = frame.clone_reg(*bound_reg)?;
            let result = match fn_val {
                // Record update: base.with(field: val) or base.with({ ... })
                Value::Record(mut base_layout, mut base_values) => {
                    let (ovr_layout, ovr_values) = match bound_val {
                        Value::Record(l, v) => (l, v),
                        other => {
                            let l = crate::eval::intern_layout(&["_0".to_string()]);
                            (l, vec![other])
                        }
                    };
                    let ovr_names = crate::eval::fields_of_layout(ovr_layout);
                    let mut base_names = crate::eval::fields_of_layout(base_layout);
                    for (name, val) in ovr_names.into_iter().zip(ovr_values.into_iter()) {
                        if let Some(pos) = crate::eval::field_pos(base_layout, &name) {
                            base_values[pos] = val;
                        } else {
                            base_names.push(name);
                            base_values.push(val);
                            base_layout = crate::eval::intern_layout(&base_names);
                        }
                    }
                    Value::Record(base_layout, base_values)
                }
                // Function partial application: fn.with(param: val)
                other => {
                    let fn_name = fn_ref_name(&other)?;
                    let bound = match bound_val {
                        Value::Record(l, v) => {
                            let names = crate::eval::fields_of_layout(l);
                            names.into_iter().zip(v).collect()
                        },
                        other => vec![("_0".to_string(), other)],
                    };
                    Value::PartialFn { name: fn_name, bound }
                }
            };
            frame.write(*dst, result);
            *ip += 1;
        }

        // ------------------------------------------------------------------
        // Refinement checks
        // ------------------------------------------------------------------
        Instruction::CheckRange { src, lo, hi } => {
            if let Value::Int(n) = frame.clone_reg(*src)?
                && (n < *lo || n > *hi)
            {
                return Err(ExecError::new(format!(
                    "CHECK_RANGE: {} not in {}..{}", n, lo, hi
                )));
            }
            *ip += 1;
        }
        Instruction::CheckRangeF { src, lo, hi } => {
            if let Value::Float(f) = frame.clone_reg(*src)?
                && (f < *lo || f > *hi)
            {
                return Err(ExecError::new(format!(
                    "CHECK_RANGE_F: {} not in {}..{}", f, lo, hi
                )));
            }
            *ip += 1;
        }
        Instruction::CheckCmp { .. } | Instruction::CheckLen { .. } => {
            *ip += 1; // TODO: implement in follow-up
        }
    }
    Ok(None)
}

// =============================================================================
// =============================================================================
// Higher-order builtin dispatch with module context
// =============================================================================

/// Route builtin calls through the module so user FnRefs in higher-order
/// list operations (fold, map, filter) can call back into the bytecode VM.
fn exec_builtin_with_module(module: &KelnModule, name: &str, args: Vec<Value>) -> Result<Value, ExecError> {
    match name {
        "List.fold" | "List.foldl" => {
            if let [list, init, Value::FnRef(fn_name)] = &args[..]
                && module.fn_idx(fn_name.as_str()).is_some() {
                return exec_fold_user(module, list.clone(), init.clone(), fn_name);
            }
            if let [list, init, Value::VmClosure { fn_idx, captures }] = &args[..] {
                return exec_fold_closure(module, list.clone(), init.clone(), *fn_idx, captures.clone());
            }
            dispatch_builtin(name, args)
        }
        "List.map" => {
            if let [list, Value::FnRef(fn_name)] = &args[..]
                && module.fn_idx(fn_name.as_str()).is_some() {
                return exec_map_user(module, list.clone(), fn_name);
            }
            if let [list, Value::VmClosure { fn_idx, captures }] = &args[..] {
                return exec_map_closure(module, list.clone(), *fn_idx, captures.clone());
            }
            dispatch_builtin(name, args)
        }
        "List.filter" => {
            if let [list, Value::FnRef(fn_name)] = &args[..]
                && module.fn_idx(fn_name.as_str()).is_some() {
                return exec_filter_user(module, list.clone(), fn_name);
            }
            if let [list, Value::VmClosure { fn_idx, captures }] = &args[..] {
                return exec_filter_closure(module, list.clone(), *fn_idx, captures.clone());
            }
            dispatch_builtin(name, args)
        }
        "List.foldUntil" => {
            if let [list, init, Value::FnRef(step_name), Value::FnRef(stop_name)] = &args[..]
                && module.fn_idx(step_name.as_str()).is_some()
                && module.fn_idx(stop_name.as_str()).is_some() {
                return exec_fold_until_user(module, list.clone(), init.clone(), step_name, stop_name);
            }
            if let [list, init, Value::VmClosure { fn_idx: si, captures: sc }, Value::VmClosure { fn_idx: pi, captures: pc }] = &args[..] {
                return exec_fold_until_closure(module, list.clone(), init.clone(), *si, sc.clone(), *pi, pc.clone());
            }
            if let [list, init, Value::FnRef(step_name), Value::VmClosure { fn_idx: pi, captures: pc }] = &args[..]
                && module.fn_idx(step_name.as_str()).is_some() {
                return exec_fold_until_mixed(module, list.clone(), init.clone(), step_name, *pi, pc.clone());
            }
            dispatch_builtin(name, args)
        }
        "Map.fold" => {
            if let [map, init, Value::FnRef(fn_name)] = &args[..]
                && module.fn_idx(fn_name.as_str()).is_some() {
                return exec_map_fold_user(module, map.clone(), init.clone(), fn_name);
            }
            if let [map, init, Value::VmClosure { fn_idx, captures }] = &args[..] {
                return exec_map_fold_closure(module, map.clone(), init.clone(), *fn_idx, captures.clone());
            }
            dispatch_builtin(name, args)
        }
        _ => dispatch_builtin(name, args),
    }
}

fn exec_fold_user(module: &KelnModule, list: Value, init: Value, fn_name: &str) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.fold: expected List")),
    };
    let fn_idx = module.fn_idx(fn_name)
        .ok_or_else(|| ExecError::new(format!("List.fold: unknown fn '{}'", fn_name)))?;
    let mut acc = init;
    for item in items {
        let arg = Value::make_record(&["acc", "item"], vec![acc, item]);
        acc = execute(module, fn_idx, arg)?;
    }
    Ok(acc)
}

fn exec_map_user(module: &KelnModule, list: Value, fn_name: &str) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.map: expected List")),
    };
    let fn_idx = module.fn_idx(fn_name)
        .ok_or_else(|| ExecError::new(format!("List.map: unknown fn '{}'", fn_name)))?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        result.push(execute(module, fn_idx, item)?);
    }
    Ok(Value::List(Rc::new(result)))
}

fn exec_fold_until_user(module: &KelnModule, list: Value, init: Value, step_name: &str, stop_name: &str) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.foldUntil: expected List")),
    };
    let step_idx = module.fn_idx(step_name)
        .ok_or_else(|| ExecError::new(format!("List.foldUntil: unknown fn '{}'", step_name)))?;
    let stop_idx = module.fn_idx(stop_name)
        .ok_or_else(|| ExecError::new(format!("List.foldUntil: unknown fn '{}'", stop_name)))?;
    let mut acc = init;
    for item in items {
        let arg = Value::make_record(&["acc", "item"], vec![acc, item]);
        acc = execute(module, step_idx, arg)?;
        if execute(module, stop_idx, acc.clone())? == Value::Bool(true) {
            break;
        }
    }
    Ok(acc)
}

fn exec_filter_user(module: &KelnModule, list: Value, fn_name: &str) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.filter: expected List")),
    };
    let fn_idx = module.fn_idx(fn_name)
        .ok_or_else(|| ExecError::new(format!("List.filter: unknown fn '{}'", fn_name)))?;
    let mut result = Vec::new();
    for item in items {
        if execute(module, fn_idx, item.clone())? == Value::Bool(true) {
            result.push(item);
        }
    }
    Ok(Value::List(Rc::new(result)))
}

// =============================================================================
// Builtin dispatch — delegate to existing stdlib via name
// =============================================================================

thread_local! {
    static DISPATCH_EVAL: std::cell::RefCell<crate::eval::Evaluator> =
        std::cell::RefCell::new(crate::eval::Evaluator::new());
}

fn dispatch_builtin(name: &str, args: Vec<Value>) -> Result<Value, ExecError> {
    DISPATCH_EVAL.with(|e| {
        stdlib::dispatch(name, args, &mut e.borrow_mut())
            .map_err(|e| ExecError::new(e.message))
    })
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

// =============================================================================
// VM-closure call helpers
// =============================================================================

/// Build the merged record `{ it: arg, cap1: v1, cap2: v2, ... }` passed to a
/// lifted closure function.
fn build_closure_call_arg(arg: Value, captures: &[(String, Value)]) -> Value {
    let names: Vec<String> = std::iter::once("it".to_string())
        .chain(captures.iter().map(|(k, _)| k.clone()))
        .collect();
    let values: Vec<Value> = std::iter::once(arg)
        .chain(captures.iter().map(|(_, v)| v.clone()))
        .collect();
    let layout = crate::eval::intern_layout(&names);
    Value::Record(layout, values)
}

fn exec_fold_closure(
    module: &KelnModule,
    list: Value,
    init: Value,
    fn_idx: usize,
    captures: Vec<(String, Value)>,
) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.fold: expected List")),
    };
    let mut acc = init;
    for item in items {
        let step_arg = Value::make_record(&["acc", "item"], vec![acc, item]);
        let merged = build_closure_call_arg(step_arg, &captures);
        acc = execute(module, fn_idx, merged)?;
    }
    Ok(acc)
}

fn exec_map_closure(
    module: &KelnModule,
    list: Value,
    fn_idx: usize,
    captures: Vec<(String, Value)>,
) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.map: expected List")),
    };
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let merged = build_closure_call_arg(item, &captures);
        result.push(execute(module, fn_idx, merged)?);
    }
    Ok(Value::List(Rc::new(result)))
}

fn exec_filter_closure(
    module: &KelnModule,
    list: Value,
    fn_idx: usize,
    captures: Vec<(String, Value)>,
) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.filter: expected List")),
    };
    let mut result = Vec::new();
    for item in items {
        let merged = build_closure_call_arg(item.clone(), &captures);
        if execute(module, fn_idx, merged)? == Value::Bool(true) {
            result.push(item);
        }
    }
    Ok(Value::List(Rc::new(result)))
}

fn exec_fold_until_closure(
    module: &KelnModule,
    list: Value,
    init: Value,
    step_idx: usize,
    step_captures: Vec<(String, Value)>,
    stop_idx: usize,
    stop_captures: Vec<(String, Value)>,
) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.foldUntil: expected List")),
    };
    let mut acc = init;
    for item in items {
        let step_arg = Value::make_record(&["acc", "item"], vec![acc, item]);
        let step_merged = build_closure_call_arg(step_arg, &step_captures);
        acc = execute(module, step_idx, step_merged)?;
        let stop_merged = build_closure_call_arg(acc.clone(), &stop_captures);
        if execute(module, stop_idx, stop_merged)? == Value::Bool(true) {
            break;
        }
    }
    Ok(acc)
}

fn exec_fold_until_mixed(
    module: &KelnModule,
    list: Value,
    init: Value,
    step_name: &str,
    stop_idx: usize,
    stop_captures: Vec<(String, Value)>,
) -> Result<Value, ExecError> {
    let items = match list {
        Value::List(v) => Rc::unwrap_or_clone(v),
        _ => return Err(ExecError::new("List.foldUntil: expected List")),
    };
    let step_idx = module.fn_idx(step_name)
        .ok_or_else(|| ExecError::new(format!("List.foldUntil: unknown fn '{}'", step_name)))?;
    let mut acc = init;
    for item in items {
        let arg = Value::make_record(&["acc", "item"], vec![acc, item]);
        acc = execute(module, step_idx, arg)?;
        let stop_merged = build_closure_call_arg(acc.clone(), &stop_captures);
        if execute(module, stop_idx, stop_merged)? == Value::Bool(true) {
            break;
        }
    }
    Ok(acc)
}

#[allow(clippy::mutable_key_type)]
fn exec_map_fold_user(
    module: &KelnModule,
    map: Value,
    init: Value,
    fn_name: &str,
) -> Result<Value, ExecError> {
    let entries = match map {
        Value::Map(m) => m,
        _ => return Err(ExecError::new("Map.fold: expected Map")),
    };
    let fn_idx = module.fn_idx(fn_name)
        .ok_or_else(|| ExecError::new(format!("Map.fold: unknown fn '{}'", fn_name)))?;
    let mut acc = init;
    for (k, v) in entries.iter() {
        let arg = Value::make_record(&["acc", "key", "value"], vec![acc, k.clone(), v.clone()]);
        acc = execute(module, fn_idx, arg)?;
    }
    Ok(acc)
}

#[allow(clippy::mutable_key_type)]
fn exec_map_fold_closure(
    module: &KelnModule,
    map: Value,
    init: Value,
    fn_idx: usize,
    captures: Vec<(String, Value)>,
) -> Result<Value, ExecError> {
    let entries = match map {
        Value::Map(m) => m,
        _ => return Err(ExecError::new("Map.fold: expected Map")),
    };
    let mut acc = init;
    for (k, v) in entries.iter() {
        let step_arg = Value::make_record(&["acc", "key", "value"], vec![acc, k.clone(), v.clone()]);
        let merged = build_closure_call_arg(step_arg, &captures);
        acc = execute(module, fn_idx, merged)?;
    }
    Ok(acc)
}

