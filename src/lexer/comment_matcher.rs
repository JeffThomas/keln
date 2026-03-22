use lexxor::matcher::{Matcher, MatcherResult};
use lexxor::token::Token;
use std::collections::HashMap;
use std::fmt::Debug;

use super::tokens::TT_COMMENT;

/// Matches Keln single-line comments: -- to end of line.
/// The comment token includes the -- prefix but not the newline.
#[derive(Debug, Clone)]
pub struct CommentMatcher {
    pub index: usize,
    pub precedence: u8,
    pub running: bool,
    in_comment: bool,
}

impl CommentMatcher {
    pub fn new(precedence: u8) -> Self {
        CommentMatcher {
            index: 0,
            precedence,
            running: true,
            in_comment: false,
        }
    }
}

impl Matcher for CommentMatcher {
    fn reset(&mut self, _ctx: &mut Box<HashMap<String, i32>>) {
        self.index = 0;
        self.running = true;
        self.in_comment = false;
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
                if self.in_comment && self.index >= 2 {
                    // EOF while in comment — return what we have
                    let matched: String = value.iter().collect();
                    MatcherResult::Matched(Token {
                        token_type: TT_COMMENT,
                        value: matched,
                        len: self.index,
                        line: 0,
                        column: self.index,
                        precedence: self.precedence,
                    })
                } else {
                    MatcherResult::Failed()
                }
            }
            Some(ch) => {
                self.index += 1;

                if self.index == 1 {
                    if ch == '-' {
                        return MatcherResult::Running();
                    } else {
                        self.running = false;
                        return MatcherResult::Failed();
                    }
                }

                if self.index == 2 {
                    if ch == '-' {
                        self.in_comment = true;
                        // We have "--" — keep running to consume the rest of the line.
                        // If EOF comes next, the None branch will produce the match.
                        return MatcherResult::Running();
                    } else {
                        self.running = false;
                        return MatcherResult::Failed();
                    }
                }

                // Inside comment body
                if ch == '\n' || ch == '\r' {
                    // End of comment (don't include the newline)
                    self.running = false;
                    let matched: String = value[..value.len() - 1].iter().collect();
                    return MatcherResult::Matched(Token {
                        token_type: TT_COMMENT,
                        value: matched,
                        len: self.index - 1,
                        line: 0,
                        column: self.index - 1,
                        precedence: self.precedence,
                    });
                }

                // Continue consuming comment characters — must return Running
                // so lexxor doesn't stop the token here. The match is only
                // produced on newline (above) or EOF (None branch).
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
