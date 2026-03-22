use lexxor::matcher::{Matcher, MatcherResult};
use lexxor::token::Token;
use std::collections::HashMap;
use std::fmt::Debug;

use super::tokens::TT_STRING;

/// Matches Keln string literals: "..." with escape sequences.
/// Handles: \" \\ \n \t \r \u{XXXX}
#[derive(Debug, Clone)]
pub struct StringMatcher {
    pub index: usize,
    pub precedence: u8,
    pub running: bool,
    in_string: bool,
    escaped: bool,
}

impl StringMatcher {
    pub fn new(precedence: u8) -> Self {
        StringMatcher {
            index: 0,
            precedence,
            running: true,
            in_string: false,
            escaped: false,
        }
    }
}

impl Matcher for StringMatcher {
    fn reset(&mut self, _ctx: &mut Box<HashMap<String, i32>>) {
        self.index = 0;
        self.running = true;
        self.in_string = false;
        self.escaped = false;
    }

    fn find_match(
        &mut self,
        oc: Option<char>,
        value: &[char],
        _ctx: &mut Box<HashMap<String, i32>>,
    ) -> MatcherResult {
        if !self.running {
            return MatcherResult::Failed();
        }

        match oc {
            None => {
                self.running = false;
                MatcherResult::Failed()
            }
            Some(ch) => {
                self.index += 1;

                if self.index == 1 {
                    // First character must be opening quote
                    if ch == '"' {
                        self.in_string = true;
                        return MatcherResult::Running();
                    } else {
                        self.running = false;
                        return MatcherResult::Failed();
                    }
                }

                if self.escaped {
                    // Accept any character after backslash
                    self.escaped = false;
                    return MatcherResult::Running();
                }

                if ch == '\\' {
                    self.escaped = true;
                    return MatcherResult::Running();
                }

                if ch == '"' {
                    // Closing quote found — complete match
                    self.running = false;
                    let matched: String = value.iter().collect();
                    return MatcherResult::Matched(Token {
                        token_type: TT_STRING,
                        value: matched,
                        len: self.index,
                        line: 0,
                        column: self.index,
                        precedence: self.precedence,
                    });
                }

                // Any other character inside the string
                MatcherResult::Running()
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
