use std::collections::HashMap;
use lexxor::matcher::{Matcher, MatcherResult};
use lexxor::token::{TOKEN_TYPE_SYMBOL, Token};

/// A custom symbol matcher that matches exactly ONE symbol character at a time.
/// This replaces lexxor's built-in `SymbolMatcher` which greedily combines
/// consecutive symbol characters (e.g., `>()` becomes one token).
/// For a programming language, we need each symbol to be a separate token.
#[derive(Clone, Debug, Copy)]
pub struct SingleSymbolMatcher {
    pub index: usize,
    pub precedence: u8,
    pub running: bool,
}

impl Matcher for SingleSymbolMatcher {
    fn reset(&mut self, _ctx: &mut Box<HashMap<String, i32>>) {
        self.index = 0;
        self.running = true;
    }

    fn find_match(
        &mut self,
        oc: Option<char>,
        value: &[char],
        _ctx: &mut Box<HashMap<String, i32>>,
    ) -> MatcherResult {
        if self.index == 0 {
            match oc {
                Some(c) if !c.is_whitespace() && !c.is_alphanumeric() => {
                    self.index = 1;
                    // Return Running on the first symbol char
                    MatcherResult::Running()
                }
                _ => {
                    self.running = false;
                    MatcherResult::Failed()
                }
            }
        } else {
            // After matching one symbol char, immediately produce a Matched token
            // regardless of what the next character is
            self.running = false;
            MatcherResult::Matched(Token {
                value: value[0..1].iter().collect(),
                token_type: TOKEN_TYPE_SYMBOL,
                len: 1,
                line: 0,
                column: 1,
                precedence: self.precedence,
            })
        }
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn precedence(&self) -> u8 {
        self.precedence
    }
}
