use lexxor::matcher::{Matcher, MatcherResult};
use lexxor::token::{TOKEN_TYPE_WORD, Token};
use std::collections::HashMap;
use std::fmt::Debug;

/// Custom identifier matcher for Keln.
/// Matches identifiers that start with a letter and continue with letters, digits, or underscores.
/// This replaces lexxor's built-in WordMatcher which only matches alphabetic characters.
#[derive(Debug, Clone, Copy)]
pub struct IdentifierMatcher {
    pub index: usize,
    pub precedence: u8,
    pub running: bool,
}

impl Matcher for IdentifierMatcher {
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
        match oc {
            Some(c) => {
                if self.index == 0 {
                    // First character must be a letter
                    if c.is_alphabetic() {
                        self.index += 1;
                        MatcherResult::Running()
                    } else {
                        self.running = false;
                        MatcherResult::Failed()
                    }
                } else {
                    // Subsequent characters: letter, digit, or underscore
                    if c.is_alphanumeric() || c == '_' {
                        self.index += 1;
                        MatcherResult::Running()
                    } else {
                        // End of identifier
                        self.running = false;
                        self.make_token(value)
                    }
                }
            }
            None => {
                // EOF
                self.running = false;
                self.make_token(value)
            }
        }
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn precedence(&self) -> u8 {
        self.precedence
    }
}

impl IdentifierMatcher {
    #[inline(always)]
    fn make_token(&self, value: &[char]) -> MatcherResult {
        if self.index > 0 {
            MatcherResult::Matched(Token {
                value: value[0..self.index].iter().collect(),
                token_type: TOKEN_TYPE_WORD,
                len: self.index,
                line: 0,
                column: self.index,
                precedence: self.precedence,
            })
        } else {
            MatcherResult::Failed()
        }
    }
}
