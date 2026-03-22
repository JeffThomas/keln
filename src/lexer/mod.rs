pub mod comment_matcher;
pub mod identifier_matcher;
pub mod string_matcher;
pub mod tokens;

use lexxor::input::InputString;
use lexxor::matcher::exact::ExactMatcher;
use lexxor::matcher::float::FloatMatcher;
use lexxor::matcher::integer::IntegerMatcher;
use lexxor::matcher::keyword::KeywordMatcher;
use lexxor::matcher::symbol::SymbolMatcher;
use lexxor::matcher::whitespace::WhitespaceMatcher;
use self::identifier_matcher::IdentifierMatcher;
use lexxor::token::Token;
use lexxor::{LexxError, Lexxer, Lexxor};

use self::comment_matcher::CommentMatcher;
use self::string_matcher::StringMatcher;
use self::tokens::{KEYWORDS, OPERATORS, TT_KEYWORD, TT_OPERATOR};

/// Create a configured Keln lexer for the given source code.
///
/// Matcher precedence strategy:
/// - precedence 0: WordMatcher, WhitespaceMatcher, SymbolMatcher (base-level catch-alls)
/// - precedence 1: IntegerMatcher, FloatMatcher (numeric literals)
/// - precedence 2: ExactMatcher for multi-char operators (->  |>  <-  ==  !=  >=  <=  ..  ::  =>)
/// - precedence 3: KeywordMatcher for reserved keywords (fn, type, let, match, etc.)
/// - precedence 4: CommentMatcher (-- comments must beat the subtraction operator)
/// - precedence 5: StringMatcher (string literals)
///
/// The precedence ensures:
/// - "fn" is recognized as a keyword, not a word
/// - "->" is recognized as an operator, not "-" then ">"
/// - "--" starts a comment, not two minus signs
pub fn create_lexer(source: &str) -> Box<dyn Lexxer> {
    let input = InputString::new(source.to_string());

    Box::new(Lexxor::<4096>::new(
        Box::new(input),
        vec![
            // Base matchers (precedence 0)
            Box::new(IdentifierMatcher {
                index: 0,
                precedence: 0,
                running: true,
            }),
            Box::new(WhitespaceMatcher {
                index: 0,
                column: 0,
                line: 0,
                precedence: 0,
                running: true,
            }),
            Box::new(SymbolMatcher {
                index: 0,
                precedence: 0,
                running: true,
            }),
            // Numeric literals (precedence 1)
            Box::new(IntegerMatcher {
                index: 0,
                precedence: 1,
                running: true,
            }),
            Box::new(FloatMatcher {
                index: 0,
                precedence: 1,
                dot: false,
                float: false,
                running: true,
            }),
            // Multi-character operators (precedence 2)
            Box::new(ExactMatcher::build_exact_matcher(
                OPERATORS.to_vec(),
                TT_OPERATOR,
                2,
            )),
            // Keywords (precedence 3) — must beat WordMatcher
            Box::new(KeywordMatcher::build_matcher_keyword(
                KEYWORDS.to_vec(),
                TT_KEYWORD,
                3,
            )),
            // Comments (precedence 4) — "--" must beat ExactMatcher's potential matches
            Box::new(CommentMatcher::new(4)),
            // String literals (precedence 5)
            Box::new(StringMatcher::new(5)),
        ],
    ))
}

/// Tokenize source code, returning all non-whitespace, non-comment tokens.
/// Useful for testing and debugging.
pub fn tokenize(source: &str) -> Result<Vec<Token>, LexxError> {
    let mut lexer = create_lexer(source);
    let mut tokens = Vec::new();

    loop {
        match lexer.next_token() {
            Ok(Some(token)) => tokens.push(token),
            Ok(None) => break,
            Err(e) => return Err(e),
        }
    }

    Ok(tokens)
}

/// Tokenize source code, filtering out whitespace and comments.
pub fn tokenize_filtered(source: &str) -> Result<Vec<Token>, LexxError> {
    let tokens = tokenize(source)?;
    Ok(tokens
        .into_iter()
        .filter(|t| {
            t.token_type != tokens::TT_WHITESPACE && t.token_type != tokens::TT_COMMENT
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokens::*;

    #[test]
    fn test_parse_port_declaration() {
        let source = r#"fn parsePort {
    Pure String -> Result<Port, PortError>
    in:  s
    out: Result.ok(s)
}"#;

        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        // Expected token sequence (whitespace filtered out):
        // fn parsePort { Pure String -> Result < Port , PortError > in : s out : Result . ok ( s ) }
        let expected: Vec<(u16, &str)> = vec![
            (TT_KEYWORD, "fn"),
            (TT_WORD, "parsePort"),
            (TT_SYMBOL, "{"),
            (TT_WORD, "Pure"),
            (TT_WORD, "String"),
            (TT_OPERATOR, "->"),
            (TT_WORD, "Result"),
            (TT_SYMBOL, "<"),
            (TT_WORD, "Port"),
            (TT_SYMBOL, ","),
            (TT_WORD, "PortError"),
            (TT_SYMBOL, ">"),
            (TT_KEYWORD, "in"),
            (TT_SYMBOL, ":"),
            (TT_WORD, "s"),
            (TT_KEYWORD, "out"),
            (TT_SYMBOL, ":"),
            (TT_WORD, "Result"),
            (TT_SYMBOL, "."),
            (TT_WORD, "ok"),
            (TT_SYMBOL, "("),
            (TT_WORD, "s"),
            (TT_SYMBOL, ")"),
            (TT_SYMBOL, "}"),
        ];

        assert_eq!(
            tokens.len(),
            expected.len(),
            "token count mismatch: got {} expected {}\nActual tokens: {:?}",
            tokens.len(),
            expected.len(),
            tokens
                .iter()
                .map(|t| format!("({}, {:?})", t.token_type, t.value))
                .collect::<Vec<_>>()
        );

        for (i, (expected_type, expected_value)) in expected.iter().enumerate() {
            assert_eq!(
                tokens[i].token_type, *expected_type,
                "token {} type mismatch: expected type {} for {:?}, got type {} for {:?}",
                i, expected_type, expected_value, tokens[i].token_type, tokens[i].value
            );
            assert_eq!(
                tokens[i].value, *expected_value,
                "token {} value mismatch: expected {:?}, got {:?}",
                i, expected_value, tokens[i].value
            );
        }
    }

    #[test]
    fn test_keywords_recognized() {
        let source = "fn type module let match do select verify given forall";
        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        for token in &tokens {
            assert_eq!(
                token.token_type, TT_KEYWORD,
                "{:?} should be a keyword",
                token.value
            );
        }
    }

    #[test]
    fn test_operators_recognized() {
        let source = "-> |> <- == != >= <= .. :: =>";
        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        let expected_ops = vec!["->", "|>", "<-", "==", "!=", ">=", "<=", "..", "::", "=>"];

        assert_eq!(tokens.len(), expected_ops.len());
        for (i, expected) in expected_ops.iter().enumerate() {
            assert_eq!(tokens[i].token_type, TT_OPERATOR, "operator {:?}", expected);
            assert_eq!(tokens[i].value, *expected);
        }
    }

    #[test]
    fn test_numeric_literals() {
        let source = "42 3.14 0 1.0";
        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        assert_eq!(tokens[0].token_type, TT_INTEGER);
        assert_eq!(tokens[0].value, "42");

        assert_eq!(tokens[1].token_type, TT_FLOAT);
        assert_eq!(tokens[1].value, "3.14");

        assert_eq!(tokens[2].token_type, TT_INTEGER);
        assert_eq!(tokens[2].value, "0");

        assert_eq!(tokens[3].token_type, TT_FLOAT);
        assert_eq!(tokens[3].value, "1.0");
    }

    #[test]
    fn test_string_literal() {
        let source = r#""hello world""#;
        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TT_STRING);
        assert_eq!(tokens[0].value, "\"hello world\"");
    }

    #[test]
    fn test_comment() {
        let source = "fn foo -- this is a comment\nfn bar";
        let all_tokens = tokenize(source).expect("tokenization should succeed");

        // Should contain a comment token
        let comments: Vec<_> = all_tokens
            .iter()
            .filter(|t| t.token_type == TT_COMMENT)
            .collect();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].value, "-- this is a comment");

        // Filtered should have no comments
        let filtered = tokenize_filtered(source).expect("tokenization should succeed");
        assert!(filtered.iter().all(|t| t.token_type != TT_COMMENT));
    }

    #[test]
    fn test_line_and_column_tracking() {
        let source = "fn foo {\n    Pure\n}";
        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        // fn at line 1, col 1
        assert_eq!(tokens[0].value, "fn");
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].column, 1);

        // foo at line 1, col 4
        assert_eq!(tokens[1].value, "foo");
        assert_eq!(tokens[1].line, 1);
        assert_eq!(tokens[1].column, 4);

        // { at line 1, col 8
        assert_eq!(tokens[2].value, "{");
        assert_eq!(tokens[2].line, 1);
        assert_eq!(tokens[2].column, 8);

        // Pure at line 2, col 5
        assert_eq!(tokens[3].value, "Pure");
        assert_eq!(tokens[3].line, 2);
        assert_eq!(tokens[3].column, 5);

        // } at line 3, col 1
        assert_eq!(tokens[4].value, "}");
        assert_eq!(tokens[4].line, 3);
        assert_eq!(tokens[4].column, 1);
    }

    #[test]
    fn test_type_names_are_words() {
        // Type names come through as TT_WORD — parser distinguishes them
        let source = "Int Float Bool String Bytes Unit Never Result Maybe List";
        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        for token in &tokens {
            assert_eq!(
                token.token_type, TT_WORD,
                "{:?} should be a word (type name)",
                token.value
            );
        }
    }

    #[test]
    fn test_effect_names_are_words() {
        let source = "Pure IO Log Metric Clock";
        let tokens = tokenize_filtered(source).expect("tokenization should succeed");

        for token in &tokens {
            assert_eq!(
                token.token_type, TT_WORD,
                "{:?} should be a word (effect name)",
                token.value
            );
        }
    }
}
