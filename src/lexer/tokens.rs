use lexxor::token::{
    TOKEN_TYPE_FLOAT, TOKEN_TYPE_INTEGER, TOKEN_TYPE_KEYWORD, TOKEN_TYPE_SYMBOL,
    TOKEN_TYPE_WHITESPACE, TOKEN_TYPE_WORD,
};

// Re-export lexxor's built-in token types for convenience
pub const TT_INTEGER: u16 = TOKEN_TYPE_INTEGER; // 1
pub const TT_FLOAT: u16 = TOKEN_TYPE_FLOAT; // 2
pub const TT_WHITESPACE: u16 = TOKEN_TYPE_WHITESPACE; // 3
pub const TT_WORD: u16 = TOKEN_TYPE_WORD; // 4
pub const TT_SYMBOL: u16 = TOKEN_TYPE_SYMBOL; // 5

// Keln-specific token types (100+ to avoid collisions with lexxor built-ins)

// Keywords — reserved words from grammar §17
pub const TT_KEYWORD: u16 = TOKEN_TYPE_KEYWORD; // 7

// Multi-character operators and delimiters — matched by ExactMatcher
pub const TT_OPERATOR: u16 = 100;

// String literals
pub const TT_STRING: u16 = 101;

// Comment (-- to end of line)
pub const TT_COMMENT: u16 = 102;

/// All Keln keywords from the grammar §17 <reserved_keyword>
pub const KEYWORDS: &[&str] = &[
    // Declaration keywords
    "fn",
    "type",
    "module",
    "trusted",
    "effect",
    "let",
    // Function body keywords
    "in",
    "out",
    "match",
    "do",
    "select",
    "timeout",
    "clone",
    "spawn",
    // Verification keywords
    "verify",
    "given",
    "forall",
    "mock",
    "call",
    // Helper keywords
    "helpers",
    "promote",
    "threshold",
    // Metadata keywords
    "confidence",
    "reason",
    "proves",
    "provenance",
    // Constraint keywords
    "where",
    "auto",
    // Logic operators (reserved globally, scoped semantically)
    "not",
    "and",
    "or",
    "implies",
    // Literals that are also keywords
    "true",
    "false",
    // Fuzz keywords (reserved in trusted module context)
    "fuzz",
    "inputs",
    "crashes_never",
    "returns_result",
    "deterministic",
];

/// Reserved type names from grammar §17 <reserved_type_name>
/// These are UpperCamelCase and matched as words, but we want to
/// distinguish them from user-defined type names during parsing.
/// For lexing purposes they come through as TT_WORD (UpperCamelCase).
pub const RESERVED_TYPE_NAMES: &[&str] = &[
    "Int",
    "Float",
    "Bool",
    "String",
    "Bytes",
    "Unit",
    "Never",
    "List",
    "Map",
    "Set",
    "Channel",
    "Maybe",
    "Result",
    "Task",
    "Ordering",
    "FunctionRef",
];

/// Effect names — also UpperCamelCase, come through as TT_WORD
pub const EFFECT_NAMES: &[&str] = &["Pure", "IO", "Log", "Metric", "Clock"];

/// Multi-character operators matched by ExactMatcher.
/// Order matters: longer matches must be listed so ExactMatcher can find them.
/// The ExactMatcher is greedy and will match the longest possible string.
pub const OPERATORS: &[&str] = &[
    // Arrow
    "->",
    // Pipeline
    "|>",
    // Channel operations
    "<-",
    // Comparison operators
    "==",
    "!=",
    ">=",
    "<=",
    // Range
    "..",
    // Compact helper separator
    "::",
    // Fat arrow (compact helper body)
    "=>",
];

/// Single-character delimiters and operators.
/// These are handled by SymbolMatcher as fallback, but we list them
/// here for documentation. The parser will inspect token values.
/// {  }  (  )  [  ]  <  >  ,  :  .  |  &  +  -  *  /  =  _
pub const SINGLE_CHAR_SYMBOLS: &[char] = &[
    '{', '}', '(', ')', '[', ']', '<', '>', ',', ':', '.', '|', '&', '+', '-', '*', '/', '=', '_',
];
