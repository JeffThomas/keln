pub mod ir;
pub mod lower;
pub mod exec;
pub mod codec;
#[cfg(test)]
mod tests;

use crate::eval::Value;

/// Parse a Keln source string, lower to bytecode, and call the named function.
/// This is the VM-backend equivalent of `eval::eval_fn`.
pub fn eval_fn(source: &str, fn_name: &str, arg: Value) -> Result<Value, String> {
    let program = crate::parser::parse(source).map_err(|e| format!("{}", e))?;
    let module = lower::lower_program(&program)?;
    exec::execute_fn(&module, fn_name, arg).map_err(|e| format!("{}", e))
}
