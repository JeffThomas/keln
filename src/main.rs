use keln::lexer;

fn main() {
    let source = r#"fn parsePort {
    Pure String -> Result<Port, PortError>
    in:  s
    out: Result.ok(s)
}"#;

    println!("Tokenizing Keln source:");
    println!("---");
    println!("{}", source);
    println!("---\n");

    match lexer::tokenize(source) {
        Ok(tokens) => {
            for token in &tokens {
                let type_name = match token.token_type {
                    lexer::tokens::TT_INTEGER => "INTEGER",
                    lexer::tokens::TT_FLOAT => "FLOAT",
                    lexer::tokens::TT_WHITESPACE => "WHITESPACE",
                    lexer::tokens::TT_WORD => "WORD",
                    lexer::tokens::TT_SYMBOL => "SYMBOL",
                    lexer::tokens::TT_KEYWORD => "KEYWORD",
                    lexer::tokens::TT_OPERATOR => "OPERATOR",
                    lexer::tokens::TT_STRING => "STRING",
                    lexer::tokens::TT_COMMENT => "COMMENT",
                    _ => "UNKNOWN",
                };
                println!(
                    "  {:>10} {:>12} ln:{:<3} col:{:<3} {:?}",
                    type_name,
                    format!("({})", token.token_type),
                    token.line,
                    token.column,
                    token.value
                );
            }
            println!("\nTotal tokens: {}", tokens.len());
        }
        Err(e) => {
            eprintln!("Tokenization error: {}", e);
        }
    }
}
