use clap::{Parser, Subcommand};
use keln::eval::stdlib::{json_to_value, value_to_json};
use keln::eval::{load_source, Value};
use keln::lexer;
use keln::lexer::tokens::*;
use keln::types::check_source;
use keln::verify::{result::VerificationResult, VerifyExecutor};
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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Run { file, func, arg } => cmd_run(&file, &func, arg.as_deref()),
        Command::Check { file } => cmd_check(&file),
        Command::Verify { file } => cmd_verify(&file),
        Command::Tokens { file, line } => cmd_tokens(&file, line),
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
                if filter_line.map_or(true, |l| t.line == l) {
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
