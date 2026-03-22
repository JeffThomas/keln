use std::fmt;
use lexxor::token::Token;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl ParseError {
    pub fn at(tok: &Token, msg: &str) -> Self {
        ParseError {
            message: format!("{}, got {:?}", msg, tok.value),
            line: tok.line,
            column: tok.column,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at {}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}
