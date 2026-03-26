use clap::{Parser, Subcommand};
use keln::eval::stdlib::{json_to_value, value_to_json};
use keln::eval::{load_source, Value};
use keln::lexer;
use keln::lexer::tokens::*;
use keln::types::check_source;
use keln::verify::{result::VerificationResult, VerifyExecutor};
use keln::vm::codec;
use std::fs;

#[derive(Parser)]
#[command(name = "keln", about = "Keln language toolchain", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Evaluate a function in a .keln file and print the result
    Run {
        /// Path to the .keln source file
        file: String,
        /// Name of the function to call
        #[arg(long = "fn", short = 'f')]
        func: String,
        /// JSON-encoded argument (default: null → Unit)
        #[arg(long, short = 'a')]
        arg: Option<String>,
    },
    /// Type-check a .keln file and report errors
    Check {
        /// Path to the .keln source file
        file: String,
    },
    /// Run all verify blocks in a .keln file and emit VerificationResult JSON
    Verify {
        /// Path to the .keln source file
        file: String,
    },
    /// Dump the filtered token stream for debugging
    Tokens {
        /// Path to the .keln source file
        file: String,
        /// Only show tokens on this line number
        #[arg(long)]
        line: Option<usize>,
    },
    /// Compile a .keln file to .kbc bytecode
    Compile {
        /// Path to the .keln source file
        file: String,
        /// Output path for the .kbc file (default: <file>.kbc)
        #[arg(long, short = 'o')]
        output: Option<String>,
        /// Name of the entry-point function (optional)
        #[arg(long, short = 'e')]
        entry: Option<String>,
        /// Strip debug info from the output
        #[arg(long)]
        release: bool,
    },
    /// Execute a compiled .kbc bytecode file
    RunBc {
        /// Path to the .kbc file
        file: String,
        /// Name of the function to call (overrides embedded entry point)
        #[arg(long = "fn", short = 'f')]
        func: Option<String>,
        /// JSON-encoded argument (default: null → Unit)
        #[arg(long, short = 'a')]
        arg: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Run { file, func, arg } => cmd_run(&file, &func, arg.as_deref()),
        Command::Check { file } => cmd_check(&file),
        Command::Verify { file } => cmd_verify(&file),
        Command::Tokens { file, line } => cmd_tokens(&file, line),
        Command::Compile { file, output, entry, release } =>
            cmd_compile(&file, output.as_deref(), entry.as_deref(), release),
        Command::RunBc { file, func, arg } =>
            cmd_run_bc(&file, func.as_deref(), arg.as_deref()),
    }
}

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read '{}': {}", path, e);
        std::process::exit(1);
    })
}

fn cmd_run(path: &str, func: &str, arg_json: Option<&str>) {
    let source = read_file(path);

    let arg: Value = match arg_json {
        None => Value::Unit,
        Some(s) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(j) => json_to_value(j),
            Err(e) => {
                eprintln!("error: invalid JSON argument: {}", e);
                std::process::exit(1);
            }
        },
    };

    let mut ev = match load_source(&source) {
        Ok(ev) => ev,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    match ev.call_fn(func, arg) {
        Ok(v) => {
            let j = value_to_json(&v);
            println!("{}", serde_json::to_string_pretty(&j).unwrap());
        }
        Err(e) => {
            eprintln!("runtime error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_tokens(path: &str, filter_line: Option<usize>) {
    let source = read_file(path);
    match lexer::tokenize_filtered(&source) {
        Err(e) => {
            eprintln!("lex error: {}", e);
            std::process::exit(1);
        }
        Ok(tokens) => {
            println!("{:>5}  {:>4}  {:>12}  {:>10}  value", "idx", "line", "col", "type");
            println!("{}", "-".repeat(60));
            for (i, t) in tokens.iter().enumerate() {
                if filter_line.is_none_or(|l| t.line == l) {
                    let type_name = match t.token_type {
                        TT_INTEGER   => "INTEGER",
                        TT_FLOAT     => "FLOAT",
                        TT_WHITESPACE => "WHITESPACE",
                        TT_WORD      => "WORD",
                        TT_SYMBOL    => "SYMBOL",
                        TT_KEYWORD   => "KEYWORD",
                        TT_OPERATOR  => "OPERATOR",
                        TT_STRING    => "STRING",
                        TT_COMMENT   => "COMMENT",
                        _            => "UNKNOWN",
                    };
                    println!("{:>5}  {:>4}  {:>12}  {:>10}  {:?}",
                        i, t.line, t.column, type_name, t.value);
                }
            }
            println!("\nTotal filtered tokens: {}", tokens.len());
        }
    }
}

fn cmd_check(path: &str) {
    let source = read_file(path);
    match check_source(&source) {
        Err(e) => {
            eprintln!("parse error: {}", e);
            std::process::exit(1);
        }
        Ok(errors) if errors.is_empty() => println!("ok"),
        Ok(errors) => {
            for e in &errors {
                eprintln!("type error: {}", e);
            }
            std::process::exit(1);
        }
    }
}

fn cmd_verify(path: &str) {
    let source = read_file(path);
    let mut ex = match VerifyExecutor::from_source(&source) {
        Ok(ex) => ex,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let fn_results = ex.verify_all();
    let mut vr = VerificationResult::from_fn_results(&fn_results);
    vr.fuzz_status = ex.fuzz_trusted_modules();
    println!("{}", vr.to_json());
}

fn cmd_compile(path: &str, output: Option<&str>, entry_name: Option<&str>, release: bool) {
    let source = read_file(path);
    let program = match keln::parser::parse(&source) {
        Ok(p) => p,
        Err(e) => { eprintln!("parse error: {}", e); std::process::exit(1); }
    };
    let module = match keln::vm::lower::lower_program(&program) {
        Ok(m) => m,
        Err(e) => { eprintln!("lower error: {}", e); std::process::exit(1); }
    };

    let entry = match entry_name {
        Some(name) => match module.fn_idx(name) {
            Some(idx) => Some(idx),
            None => {
                eprintln!("error: entry function '{}' not found", name);
                std::process::exit(1);
            }
        },
        None => None,
    };

    let flags = if release { 0u16 } else { codec::FLAG_DEBUG_INFO };
    let bytes = match codec::encode(&module, flags, entry) {
        Ok(b) => b,
        Err(e) => { eprintln!("encode error: {}", e); std::process::exit(1); }
    };

    let out_path = output.map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}.kbc", path.trim_end_matches(".keln")));

    if let Err(e) = fs::write(&out_path, &bytes) {
        eprintln!("error: cannot write '{}': {}", out_path, e);
        std::process::exit(1);
    }
    eprintln!("compiled {} → {} ({} bytes)", path, out_path, bytes.len());
}

fn cmd_run_bc(path: &str, func_override: Option<&str>, arg_json: Option<&str>) {
    let bytes = fs::read(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read '{}': {}", path, e);
        std::process::exit(1);
    });

    let (module, _flags, embedded_entry) = match codec::decode(&bytes) {
        Ok(r) => r,
        Err(e) => { eprintln!("decode error: {}", e); std::process::exit(1); }
    };

    let fn_name: String = match func_override {
        Some(name) => name.to_string(),
        None => match embedded_entry {
            Some(idx) => match module.fns.get(idx) {
                Some(f) => f.name.clone(),
                None => {
                    eprintln!("error: entry index {} out of range", idx);
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("error: no entry point; use --fn to specify a function");
                std::process::exit(1);
            }
        },
    };

    let arg: Value = match arg_json {
        None => Value::Unit,
        Some(s) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(j) => json_to_value(j),
            Err(e) => {
                eprintln!("error: invalid JSON argument: {}", e);
                std::process::exit(1);
            }
        },
    };

    match keln::vm::exec::execute_fn(&module, &fn_name, arg) {
        Ok(v) => {
            let j = value_to_json(&v);
            println!("{}", serde_json::to_string_pretty(&j).unwrap());
        }
        Err(e) => {
            eprintln!("runtime error: {}", e.message);
            std::process::exit(1);
        }
    }
}
