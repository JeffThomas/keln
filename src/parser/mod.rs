pub mod error;

use lexxor::token::Token;

use crate::ast::*;
use crate::lexer::tokens::*;
use self::error::ParseError;

/// Parser state: walks through a filtered token stream (no whitespace/comments).
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    // =========================================================================
    // Token cursor helpers
    // =========================================================================

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_type(&self) -> Option<u16> {
        self.peek().map(|t| t.token_type)
    }

    fn peek_value(&self) -> Option<&str> {
        self.peek().map(|t| t.value.as_str())
    }

    fn peek_nth(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.pos + n)
    }

    /// Returns true if the current `{` token's matching `}` is immediately
    /// followed by `->`. Used to distinguish `Name { fields }` record
    /// constructors from anonymous record patterns that open the next match arm.
    fn peek_brace_arrow(&self) -> bool {
        if !matches!(self.peek(), Some(t) if t.value == "{") {
            return false;
        }
        let mut depth = 0i32;
        let mut i = 0;
        while let Some(t) = self.peek_nth(i) {
            match t.value.as_str() {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.peek_nth(i + 1),
                            Some(t2) if t2.token_type == TT_OPERATOR && t2.value == "->"
                        );
                    }
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Advance and return an owned clone of the current token.
    fn advance(&mut self) -> Result<Token, ParseError> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Ok(tok)
        } else {
            Err(self.error_eof("token"))
        }
    }

    fn current_span(&self) -> Span {
        if let Some(t) = self.peek() {
            Span { line: t.line, column: t.column }
        } else if let Some(t) = self.tokens.last() {
            Span { line: t.line, column: t.column + t.len }
        } else {
            Span { line: 1, column: 1 }
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<Span, ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_KEYWORD && tok.value == kw {
            Ok(Span { line: tok.line, column: tok.column })
        } else {
            Err(ParseError::at(&tok, &format!("expected keyword '{}'", kw)))
        }
    }

    fn expect_symbol(&mut self, sym: &str) -> Result<Span, ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_SYMBOL && tok.value == sym {
            Ok(Span { line: tok.line, column: tok.column })
        } else {
            Err(ParseError::at(&tok, &format!("expected '{}'", sym)))
        }
    }

    fn expect_operator(&mut self, op: &str) -> Result<Span, ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_OPERATOR && tok.value == op {
            Ok(Span { line: tok.line, column: tok.column })
        } else {
            Err(ParseError::at(&tok, &format!("expected '{}'", op)))
        }
    }

    fn expect_lower_ident(&mut self) -> Result<(String, Span), ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_WORD && tok.value.starts_with(|c: char| c.is_ascii_lowercase()) {
            Ok((tok.value.clone(), Span { line: tok.line, column: tok.column }))
        } else {
            Err(ParseError::at(&tok, "expected lower_snake_case identifier"))
        }
    }

    fn expect_upper_ident(&mut self) -> Result<(String, Span), ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_WORD && tok.value.starts_with(|c: char| c.is_ascii_uppercase()) {
            Ok((tok.value.clone(), Span { line: tok.line, column: tok.column }))
        } else {
            Err(ParseError::at(&tok, "expected UpperCamelCase identifier"))
        }
    }

    fn expect_string_literal(&mut self) -> Result<(String, Span), ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_STRING {
            let inner = tok.value[1..tok.value.len() - 1].to_string();
            Ok((inner, Span { line: tok.line, column: tok.column }))
        } else {
            Err(ParseError::at(&tok, "expected string literal"))
        }
    }

    fn expect_word(&mut self, word: &str) -> Result<Span, ParseError> {
        let tok = self.advance()?;
        if tok.value == word {
            Ok(Span { line: tok.line, column: tok.column })
        } else {
            Err(ParseError::at(&tok, &format!("expected '{}'", word)))
        }
    }

    fn check_keyword(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(t) if t.token_type == TT_KEYWORD && t.value == kw)
    }

    fn check_symbol(&self, sym: &str) -> bool {
        matches!(self.peek(), Some(t) if t.token_type == TT_SYMBOL && t.value == sym)
    }

    fn check_operator(&self, op: &str) -> bool {
        matches!(self.peek(), Some(t) if t.token_type == TT_OPERATOR && t.value == op)
    }

    fn check_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(t) if t.value == word)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn error_eof(&self, expected: &str) -> ParseError {
        let span = self.current_span();
        ParseError {
            message: format!("unexpected end of input, expected {}", expected),
            line: span.line,
            column: span.column,
        }
    }

    fn error_here(&self, msg: &str) -> ParseError {
        let span = self.current_span();
        ParseError {
            message: msg.to_string(),
            line: span.line,
            column: span.column,
        }
    }

    // =========================================================================
    // Program
    // =========================================================================

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut declarations = Vec::new();
        while !self.at_end() {
            declarations.push(self.parse_top_level_decl()?);
        }
        Ok(Program { declarations })
    }

    fn parse_top_level_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        match self.peek_value() {
            Some("type") => Ok(TopLevelDecl::TypeDecl(self.parse_type_decl()?)),
            Some("fn") => Ok(TopLevelDecl::FnDecl(self.parse_fn_decl()?)),
            Some("module") => Ok(TopLevelDecl::ModuleDecl(self.parse_module_decl()?)),
            Some("trusted") => Ok(TopLevelDecl::TrustedModuleDecl(self.parse_trusted_module_decl()?)),
            Some("effect") => Ok(TopLevelDecl::EffectDecl(self.parse_effect_decl()?)),
            Some("let") => Ok(TopLevelDecl::LetBinding(self.parse_let_binding()?)),
            _ => Err(self.error_here("expected top-level declaration (type, fn, module, trusted, effect, let)")),
        }
    }

    // =========================================================================
    // Type declarations
    // =========================================================================

    fn parse_type_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let span = self.expect_keyword("type")?;
        let (name, _) = self.expect_upper_ident()?;
        let type_params = if self.check_symbol("<") {
            self.parse_type_params()?
        } else {
            vec![]
        };
        self.expect_symbol("=")?;
        let def = self.parse_type_def()?;
        Ok(TypeDecl { name, type_params, def, span })
    }

    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect_symbol("<")?;
        let mut params = vec![];
        let (first, _) = self.expect_upper_ident()?;
        params.push(first);
        while self.check_symbol(",") {
            self.advance()?;
            let (p, _) = self.expect_upper_ident()?;
            params.push(p);
        }
        self.expect_symbol(">")?;
        Ok(params)
    }

    fn parse_type_def(&mut self) -> Result<TypeDef, ParseError> {
        if self.check_symbol("|") {
            return Ok(TypeDef::Sum(self.parse_sum_type_def()?));
        }
        if self.check_symbol("{") {
            return Ok(TypeDef::Product(self.parse_product_type_fields()?));
        }
        if let Some(t) = self.peek() {
            if t.token_type == TT_WORD && t.value.starts_with(|c: char| c.is_ascii_uppercase()) {
                if let Some(next) = self.peek_nth(1) {
                    if next.token_type == TT_SYMBOL && next.value == "|" {
                        return Ok(TypeDef::Sum(self.parse_sum_type_def()?));
                    }
                    if (next.token_type == TT_SYMBOL && (next.value == "{" || next.value == "("))
                        && !self.is_generic_type_start()
                    {
                        return Ok(TypeDef::Sum(self.parse_sum_type_def()?));
                    }
                }
            }
        }
        let type_expr = self.parse_type_expr()?;
        if self.check_keyword("where") {
            self.advance()?;
            let constraint = self.parse_refinement_constraint()?;
            Ok(TypeDef::Refinement { base: type_expr, constraint })
        } else {
            Ok(TypeDef::Alias(type_expr))
        }
    }

    fn is_generic_type_start(&self) -> bool {
        if let (Some(t), Some(n)) = (self.peek(), self.peek_nth(1)) {
            t.token_type == TT_WORD
                && t.value.starts_with(|c: char| c.is_ascii_uppercase())
                && n.token_type == TT_SYMBOL
                && n.value == "<"
        } else {
            false
        }
    }

    fn parse_sum_type_def(&mut self) -> Result<Vec<VariantDecl>, ParseError> {
        let mut variants = vec![];
        if self.check_symbol("|") { self.advance()?; }
        variants.push(self.parse_variant_decl()?);
        while self.check_symbol("|") {
            self.advance()?;
            variants.push(self.parse_variant_decl()?);
        }
        Ok(variants)
    }

    fn parse_variant_decl(&mut self) -> Result<VariantDecl, ParseError> {
        let (name, span) = self.expect_upper_ident()?;
        let payload = if self.check_symbol("(") {
            self.advance()?;
            let inner = self.parse_type_expr()?;
            self.expect_symbol(")")?;
            VariantPayload::Tuple(inner)
        } else if self.check_symbol("{") {
            VariantPayload::Record(self.parse_braced_field_type_list()?)
        } else {
            VariantPayload::Unit
        };
        Ok(VariantDecl { name, payload, span })
    }

    fn parse_braced_field_type_list(&mut self) -> Result<Vec<FieldTypeDecl>, ParseError> {
        self.expect_symbol("{")?;
        let mut fields = vec![];
        while !self.check_symbol("}") {
            if !fields.is_empty() && self.check_symbol(",") { self.advance()?; }
            fields.push(self.parse_field_type_decl()?);
        }
        self.expect_symbol("}")?;
        Ok(fields)
    }

    fn parse_product_type_fields(&mut self) -> Result<Vec<FieldTypeDecl>, ParseError> {
        self.parse_braced_field_type_list()
    }

    fn parse_field_type_decl(&mut self) -> Result<FieldTypeDecl, ParseError> {
        let (name, span) = self.expect_lower_ident()?;
        self.expect_symbol(":")?;
        let type_expr = self.parse_type_expr()?;
        let refinement = if self.check_keyword("where") {
            self.advance()?;
            Some(self.parse_refinement_constraint()?)
        } else {
            None
        };
        Ok(FieldTypeDecl { name, type_expr, refinement, span })
    }

    // =========================================================================
    // Type expressions
    // =========================================================================

    pub fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        if self.check_symbol("{") {
            let span = self.current_span();
            let fields = self.parse_product_type_fields()?;
            return Ok(TypeExpr::Product(fields, span));
        }
        let (name, span) = self.expect_upper_ident()?;
        match name.as_str() {
            "Int" => return Ok(TypeExpr::Primitive(PrimitiveType::Int, span)),
            "Float" => return Ok(TypeExpr::Primitive(PrimitiveType::Float, span)),
            "Bool" => return Ok(TypeExpr::Primitive(PrimitiveType::Bool, span)),
            "String" => return Ok(TypeExpr::Primitive(PrimitiveType::String, span)),
            "Bytes" => return Ok(TypeExpr::Primitive(PrimitiveType::Bytes, span)),
            "Unit" => return Ok(TypeExpr::Primitive(PrimitiveType::Unit, span)),
            "Never" => return Ok(TypeExpr::Never(span)),
            _ => {}
        }
        if self.check_symbol("<") {
            if name == "FunctionRef" {
                return self.parse_function_ref_type(span);
            }
            self.advance()?;
            let mut args = vec![self.parse_type_expr()?];
            while self.check_symbol(",") {
                self.advance()?;
                args.push(self.parse_type_expr()?);
            }
            self.expect_symbol(">")?;
            return Ok(TypeExpr::Generic { name, args, span });
        }
        Ok(TypeExpr::Named(name, span))
    }

    fn parse_function_ref_type(&mut self, span: Span) -> Result<TypeExpr, ParseError> {
        self.expect_symbol("<")?;
        let effect = self.parse_effect_set()?;
        self.expect_symbol(",")?;
        let input = self.parse_type_expr()?;
        self.expect_symbol(",")?;
        let output = self.parse_type_expr()?;
        self.expect_symbol(">")?;
        Ok(TypeExpr::FunctionRef {
            effect,
            input: Box::new(input),
            output: Box::new(output),
            span,
        })
    }

    // =========================================================================
    // Effects
    // =========================================================================

    fn parse_effect_set(&mut self) -> Result<EffectSet, ParseError> {
        let span = self.current_span();
        let mut effects = vec![];
        let (name, _) = self.expect_upper_ident()?;
        effects.push(name);
        while self.check_symbol("&") {
            self.advance()?;
            let (name, _) = self.expect_upper_ident()?;
            effects.push(name);
        }
        Ok(EffectSet { effects, span })
    }

    // =========================================================================
    // Refinement constraints
    // =========================================================================

    fn parse_refinement_constraint(&mut self) -> Result<RefinementConstraint, ParseError> {
        if self.check_keyword("matches") {
            self.advance()?;
            self.expect_symbol("(")?;
            let (name, _) = self.expect_upper_ident()?;
            self.expect_symbol(")")?;
            return Ok(RefinementConstraint::Format(name));
        }
        if matches!(self.peek(), Some(t) if t.value == "len") {
            self.advance()?;
            let op = self.parse_comparison_op()?;
            let n = self.parse_int_literal()?;
            return Ok(RefinementConstraint::Length(op, n));
        }
        if self.is_comparison_op() {
            let op = self.parse_comparison_op()?;
            let n = self.parse_number()?;
            return Ok(RefinementConstraint::Comparison(op, n));
        }
        let first = self.parse_number()?;
        if self.check_operator("..") {
            self.advance()?;
            let second = self.parse_number()?;
            return Ok(RefinementConstraint::Range(first, second));
        }
        Err(self.error_here("expected refinement constraint"))
    }

    fn is_comparison_op(&self) -> bool {
        matches!(self.peek(), Some(t) if
            (t.token_type == TT_OPERATOR && matches!(t.value.as_str(), "==" | "!=" | ">=" | "<="))
            || (t.token_type == TT_SYMBOL && matches!(t.value.as_str(), ">" | "<"))
        )
    }

    fn parse_comparison_op(&mut self) -> Result<ComparisonOp, ParseError> {
        let tok = self.advance()?;
        match tok.value.as_str() {
            "==" => Ok(ComparisonOp::Eq),
            "!=" => Ok(ComparisonOp::Ne),
            ">=" => Ok(ComparisonOp::Ge),
            "<=" => Ok(ComparisonOp::Le),
            ">" => Ok(ComparisonOp::Gt),
            "<" => Ok(ComparisonOp::Lt),
            _ => Err(ParseError::at(&tok, "expected comparison operator")),
        }
    }

    fn parse_number(&mut self) -> Result<Number, ParseError> {
        let tok = self.advance()?;
        match tok.token_type {
            TT_INTEGER => {
                let n: i64 = tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid integer"))?;
                Ok(Number::Int(n))
            }
            TT_FLOAT => {
                let n: f64 = tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid float"))?;
                Ok(Number::Float(n))
            }
            _ => Err(ParseError::at(&tok, "expected number")),
        }
    }

    fn parse_int_literal(&mut self) -> Result<i64, ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_INTEGER {
            tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid integer"))
        } else {
            Err(ParseError::at(&tok, "expected integer literal"))
        }
    }

    fn parse_float_literal(&mut self) -> Result<f64, ParseError> {
        let tok = self.advance()?;
        if tok.token_type == TT_FLOAT {
            tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid float"))
        } else {
            Err(ParseError::at(&tok, "expected float literal"))
        }
    }

    // =========================================================================
    // Function declarations
    // =========================================================================

    fn parse_fn_decl(&mut self) -> Result<FnDecl, ParseError> {
        let span = self.expect_keyword("fn")?;
        let (name, _) = self.expect_lower_ident()?;
        self.expect_symbol("{")?;
        let signature = self.parse_fn_signature()?;

        self.expect_keyword("in")?;
        self.expect_symbol(":")?;
        let in_clause = self.parse_pattern()?;

        self.expect_keyword("out")?;
        self.expect_symbol(":")?;
        let out_clause = self.parse_expr()?;

        let confidence = if self.check_keyword("confidence") {
            self.advance()?;
            self.expect_symbol(":")?;
            Some(self.parse_confidence()?)
        } else { None };

        let reason = if self.check_keyword("reason") {
            self.advance()?;
            self.expect_symbol(":")?;
            let (s, _) = self.expect_string_literal()?;
            Some(s)
        } else { None };

        let proves = if self.check_keyword("proves") {
            self.advance()?;
            self.expect_symbol(":")?;
            Some(self.parse_proves_block()?)
        } else { None };

        let provenance = if self.check_keyword("provenance") {
            self.advance()?;
            self.expect_symbol(":")?;
            Some(self.parse_provenance_block()?)
        } else { None };

        let verify = if self.check_keyword("verify") {
            self.advance()?;
            self.expect_symbol(":")?;
            Some(self.parse_verify_block()?)
        } else { None };

        let helpers = if self.check_keyword("helpers") {
            self.advance()?;
            self.expect_symbol(":")?;
            Some(self.parse_helpers_block()?)
        } else { None };

        self.expect_symbol("}")?;

        Ok(FnDecl {
            name, signature, in_clause, out_clause,
            confidence, reason, proves, provenance, verify, helpers, span,
        })
    }

    fn parse_fn_signature(&mut self) -> Result<FnSignature, ParseError> {
        let span = self.current_span();
        let effects = self.parse_effect_set()?;
        let input_type = self.parse_type_expr()?;
        self.expect_operator("->")?;
        let output_type = self.parse_type_expr()?;
        Ok(FnSignature { effects, input_type, output_type, span })
    }

    fn parse_confidence(&mut self) -> Result<Confidence, ParseError> {
        if self.check_keyword("auto") {
            self.advance()?;
            return Ok(Confidence::Auto);
        }
        if self.peek_type() == Some(TT_FLOAT) {
            let v = self.parse_float_literal()?;
            return Ok(Confidence::Simple(v));
        }
        Err(self.error_here("expected confidence value (auto, float, or structured)"))
    }

    fn parse_proves_block(&mut self) -> Result<Vec<LogicExpr>, ParseError> {
        self.expect_symbol("{")?;
        let mut props = vec![];
        while !self.check_symbol("}") {
            props.push(self.parse_logic_expr()?);
        }
        self.expect_symbol("}")?;
        Ok(props)
    }

    fn parse_provenance_block(&mut self) -> Result<Provenance, ParseError> {
        self.expect_symbol("{")?;
        self.expect_word("description")?;
        self.expect_symbol(":")?;
        let (description, _) = self.expect_string_literal()?;
        let mut prov = Provenance {
            description, pattern_id: None, version: None, source: None,
            uses: None, failures: None, failure_ref: None,
        };
        while !self.check_symbol("}") {
            match self.peek_value() {
                Some("pattern_id") => { self.advance()?; self.expect_symbol(":")?; let (s,_)=self.expect_string_literal()?; prov.pattern_id=Some(s); }
                Some("version") => { self.advance()?; self.expect_symbol(":")?; prov.version=Some(self.parse_int_literal()?); }
                Some("source") => { self.advance()?; self.expect_symbol(":")?; prov.source=Some(self.parse_pattern_source()?); }
                Some("uses") => { self.advance()?; self.expect_symbol(":")?; prov.uses=Some(self.parse_int_literal()?); }
                Some("failures") => { self.advance()?; self.expect_symbol(":")?; prov.failures=Some(self.parse_int_literal()?); }
                Some("failure_ref") => {
                    self.advance()?; self.expect_symbol(":")?; self.expect_symbol("[")?;
                    let mut refs = vec![];
                    while !self.check_symbol("]") {
                        let (s,_)=self.expect_string_literal()?; refs.push(s);
                        if self.check_symbol(",") { self.advance()?; }
                    }
                    self.expect_symbol("]")?;
                    prov.failure_ref = Some(refs);
                }
                _ => return Err(self.error_here("unexpected field in provenance block")),
            }
        }
        self.expect_symbol("}")?;
        Ok(prov)
    }

    fn parse_pattern_source(&mut self) -> Result<PatternSource, ParseError> {
        let tok = self.advance()?;
        if tok.value != "PatternSource" {
            return Err(ParseError::at(&tok, "expected PatternSource"));
        }
        self.expect_symbol(".")?;
        let (variant, _) = self.expect_upper_ident()?;
        match variant.as_str() {
            "Verified" => Ok(PatternSource::Verified),
            "Unverified" => Ok(PatternSource::Unverified),
            "Experimental" => Ok(PatternSource::Experimental),
            _ => Err(self.error_here("expected Verified, Unverified, or Experimental")),
        }
    }

    // =========================================================================
    // Verify block
    // =========================================================================

    fn parse_verify_block(&mut self) -> Result<Vec<VerifyStmt>, ParseError> {
        self.expect_symbol("{")?;
        let mut stmts = vec![];
        while !self.check_symbol("}") {
            stmts.push(self.parse_verify_stmt()?);
        }
        self.expect_symbol("}")?;
        Ok(stmts)
    }

    fn parse_verify_stmt(&mut self) -> Result<VerifyStmt, ParseError> {
        match self.peek_value() {
            Some("mock") => Ok(VerifyStmt::Mock(self.parse_mock_decl()?)),
            Some("given") => Ok(VerifyStmt::Given(self.parse_given_case()?)),
            Some("forall") => Ok(VerifyStmt::ForAll(self.parse_forall_property()?)),
            _ => Err(self.error_here("expected mock, given, or forall in verify block")),
        }
    }

    fn parse_mock_decl(&mut self) -> Result<MockDecl, ParseError> {
        let span = self.expect_keyword("mock")?;
        let tok = self.advance()?;
        if tok.token_type != TT_WORD {
            return Err(ParseError::at(&tok, "expected identifier after mock"));
        }
        let name = tok.value.clone();
        self.expect_symbol("{")?;
        let mut clauses = vec![];
        while !self.check_symbol("}") {
            if self.check_keyword("call") {
                self.advance()?;
                self.expect_symbol("(")?;
                let pattern = self.parse_pattern()?;
                self.expect_symbol(")")?;
                self.expect_operator("->")?;
                let result = self.parse_expr()?;
                clauses.push(MockClause::Call { pattern, result });
            } else {
                let (method, _) = self.expect_lower_ident()?;
                self.expect_symbol("(")?;
                let mut patterns = vec![];
                if !self.check_symbol(")") {
                    patterns.push(self.parse_pattern()?);
                    while self.check_symbol(",") {
                        self.advance()?;
                        patterns.push(self.parse_pattern()?);
                    }
                }
                self.expect_symbol(")")?;
                self.expect_operator("->")?;
                let result = self.parse_expr()?;
                clauses.push(MockClause::Method { method, patterns, result });
            }
        }
        self.expect_symbol("}")?;
        Ok(MockDecl { name, clauses, span })
    }

    fn parse_given_case(&mut self) -> Result<GivenCase, ParseError> {
        let span = self.expect_keyword("given")?;
        self.expect_symbol("(")?;
        let input = self.parse_expr()?;
        self.expect_symbol(")")?;
        self.expect_operator("->")?;
        let expected = self.parse_expr()?;
        Ok(GivenCase { input, expected, span })
    }

    fn parse_forall_property(&mut self) -> Result<ForAllProperty, ParseError> {
        let span = self.expect_keyword("forall")?;
        self.expect_symbol("(")?;
        let mut bindings = vec![self.parse_forall_binding()?];
        while self.check_symbol(",") {
            self.advance()?;
            bindings.push(self.parse_forall_binding()?);
        }
        self.expect_symbol(")")?;
        self.expect_operator("->")?;
        let body = self.parse_logic_expr()?;
        Ok(ForAllProperty { bindings, body, span })
    }

    fn parse_forall_binding(&mut self) -> Result<ForAllBinding, ParseError> {
        let (name, span) = self.expect_lower_ident()?;
        self.expect_symbol(":")?;
        let type_expr = self.parse_type_expr()?;
        let refinement = if self.check_keyword("where") {
            self.advance()?;
            Some(self.parse_refinement_constraint()?)
        } else { None };
        Ok(ForAllBinding { name, type_expr, refinement, span })
    }

    // =========================================================================
    // Logic expressions (forall / proves only)
    // =========================================================================

    fn parse_logic_expr(&mut self) -> Result<LogicExpr, ParseError> {
        match self.peek_value() {
            Some("not") => {
                self.advance()?; self.expect_symbol("(")?;
                let inner = self.parse_logic_expr()?;
                self.expect_symbol(")")?;
                Ok(LogicExpr::Not(Box::new(inner)))
            }
            Some("and") => {
                self.advance()?; self.expect_symbol("(")?;
                let l = self.parse_logic_expr()?; self.expect_symbol(",")?;
                let r = self.parse_logic_expr()?; self.expect_symbol(")")?;
                Ok(LogicExpr::And(Box::new(l), Box::new(r)))
            }
            Some("or") => {
                self.advance()?; self.expect_symbol("(")?;
                let l = self.parse_logic_expr()?; self.expect_symbol(",")?;
                let r = self.parse_logic_expr()?; self.expect_symbol(")")?;
                Ok(LogicExpr::Or(Box::new(l), Box::new(r)))
            }
            Some("implies") => {
                self.advance()?; self.expect_symbol("(")?;
                let l = self.parse_logic_expr()?; self.expect_symbol(",")?;
                let r = self.parse_logic_expr()?; self.expect_symbol(")")?;
                Ok(LogicExpr::Implies(Box::new(l), Box::new(r)))
            }
            _ => {
                let left = self.parse_expr()?;
                if self.is_comparison_op() {
                    let op = self.parse_comparison_op()?;
                    let right = self.parse_expr()?;
                    Ok(LogicExpr::Comparison { left, op, right })
                } else {
                    Ok(LogicExpr::DoesNotCrash(left))
                }
            }
        }
    }

    // =========================================================================
    // Helpers block
    // =========================================================================

    fn parse_helpers_block(&mut self) -> Result<Vec<HelperDecl>, ParseError> {
        self.expect_symbol("{")?;
        let mut helpers = vec![];
        while !self.check_symbol("}") {
            if self.check_keyword("fn") {
                helpers.push(HelperDecl::Full(self.parse_fn_decl()?));
            } else {
                helpers.push(self.parse_compact_helper()?);
            }
        }
        self.expect_symbol("}")?;
        Ok(helpers)
    }

    fn parse_compact_helper(&mut self) -> Result<HelperDecl, ParseError> {
        let (name, span) = self.expect_lower_ident()?;
        self.expect_operator("::")?;
        let effects = self.parse_effect_set()?;
        let input_type = self.parse_type_expr()?;
        self.expect_operator("->")?;
        let output_type = self.parse_type_expr()?;
        self.expect_operator("=>")?;
        let body = self.parse_expr()?;
        let promote_threshold = if self.check_keyword("promote") {
            self.advance()?; self.expect_symbol(":")?;
            self.expect_keyword("threshold")?; self.expect_symbol("(")?;
            let n = self.parse_int_literal()?; self.expect_symbol(")")?;
            Some(n)
        } else { None };
        Ok(HelperDecl::Compact {
            name, effects, input_type, output_type, body, promote_threshold, span,
        })
    }

    // =========================================================================
    // Patterns
    // =========================================================================

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let span = self.current_span();
        let tok = self.peek().ok_or_else(|| self.error_eof("pattern"))?;

        match (tok.token_type, tok.value.as_str()) {
            (TT_SYMBOL, "_") => {
                self.advance()?;
                Ok(Pattern::Wildcard(span))
            }
            (TT_SYMBOL, "{") => {
                self.advance()?;
                let fields = self.parse_field_pattern_list()?;
                self.expect_symbol("}")?;
                Ok(Pattern::Record { fields, span })
            }
            (TT_SYMBOL, "[") => {
                self.advance()?;
                let mut patterns = vec![];
                if !self.check_symbol("]") {
                    patterns.push(self.parse_pattern()?);
                    while self.check_symbol(",") {
                        self.advance()?;
                        patterns.push(self.parse_pattern()?);
                    }
                }
                self.expect_symbol("]")?;
                Ok(Pattern::List(patterns, span))
            }
            (TT_KEYWORD, "true") => {
                self.advance()?;
                Ok(Pattern::Literal(Box::new(Expr::BoolLiteral(true, span))))
            }
            (TT_KEYWORD, "false") => {
                self.advance()?;
                Ok(Pattern::Literal(Box::new(Expr::BoolLiteral(false, span))))
            }
            (TT_INTEGER, _) => {
                let tok = self.advance()?;
                let n: i64 = tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid integer"))?;
                Ok(Pattern::Literal(Box::new(Expr::IntLiteral(n, span))))
            }
            (TT_FLOAT, _) => {
                let tok = self.advance()?;
                let n: f64 = tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid float"))?;
                Ok(Pattern::Literal(Box::new(Expr::FloatLiteral(n, span))))
            }
            (TT_STRING, _) => {
                let tok = self.advance()?;
                let inner = tok.value[1..tok.value.len() - 1].to_string();
                Ok(Pattern::Literal(Box::new(Expr::StringLiteral(inner, span))))
            }
            (TT_WORD, v) if v.starts_with(|c: char| c.is_ascii_uppercase()) => {
                let (name, span) = self.expect_upper_ident()?;
                if self.check_symbol("(") {
                    self.advance()?;
                    let inner = self.parse_pattern()?;
                    self.expect_symbol(")")?;
                    Ok(Pattern::TupleVariant { name, inner: Box::new(inner), span })
                } else if self.check_symbol("{") {
                    self.advance()?;
                    let fields = self.parse_field_pattern_list()?;
                    self.expect_symbol("}")?;
                    Ok(Pattern::RecordVariant { name, fields, span })
                } else {
                    Ok(Pattern::UnitVariant(name, span))
                }
            }
            (TT_WORD, _) => {
                let (name, span) = self.expect_lower_ident()?;
                Ok(Pattern::Binding(name, span))
            }
            _ => Err(self.error_here("expected pattern")),
        }
    }

    fn parse_field_pattern_list(&mut self) -> Result<Vec<FieldPattern>, ParseError> {
        let mut fields = vec![];
        if self.check_symbol("}") { return Ok(fields); }
        fields.push(self.parse_field_pattern()?);
        while self.check_symbol(",") {
            self.advance()?;
            if self.check_symbol("}") { break; }
            fields.push(self.parse_field_pattern()?);
        }
        Ok(fields)
    }

    fn parse_field_pattern(&mut self) -> Result<FieldPattern, ParseError> {
        if self.check_symbol("_") {
            self.advance()?;
            return Ok(FieldPattern::Wildcard);
        }
        let (name, _) = self.expect_lower_ident()?;
        if self.check_symbol(":") {
            self.advance()?;
            let pattern = self.parse_pattern()?;
            Ok(FieldPattern::Named(name, pattern))
        } else {
            Ok(FieldPattern::Shorthand(name))
        }
    }

    // =========================================================================
    // Expressions — precedence climbing
    // =========================================================================

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_pipeline_expr()
    }

    fn parse_pipeline_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_channel_send_expr()?;
        if self.check_operator("|>") {
            let span = self.current_span();
            let mut steps = vec![];
            while self.check_operator("|>") {
                self.advance()?;
                steps.push(self.parse_atom_expr()?);
            }
            left = Expr::Pipeline { left: Box::new(left), steps, span };
        }
        Ok(left)
    }

    fn parse_channel_send_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_comparison_expr()?;
        if self.check_operator("<-") {
            let span = self.current_span();
            self.advance()?;
            let value = self.parse_comparison_expr()?;
            return Ok(Expr::ChannelSend {
                channel: Box::new(left),
                value: Box::new(value),
                span,
            });
        }
        Ok(left)
    }

    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_additive_expr()?;
        if self.is_comparison_op() {
            let span = self.current_span();
            let op = self.parse_binary_op()?;
            let right = self.parse_additive_expr()?;
            return Ok(Expr::BinaryOp {
                left: Box::new(left), op, right: Box::new(right), span,
            });
        }
        Ok(left)
    }

    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;
        while matches!(self.peek(), Some(t) if t.token_type == TT_SYMBOL && (t.value == "+" || t.value == "-")) {
            let span = self.current_span();
            let tok = self.advance()?;
            let op = if tok.value == "+" { BinaryOp::Add } else { BinaryOp::Sub };
            let right = self.parse_multiplicative_expr()?;
            left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_postfix_expr()?;
        while matches!(self.peek(), Some(t) if t.token_type == TT_SYMBOL && (t.value == "*" || t.value == "/" || t.value == "%")) {
            let span = self.current_span();
            let tok = self.advance()?;
            let op = match tok.value.as_str() {
                "*" => BinaryOp::Mul,
                "/" => BinaryOp::Div,
                _   => BinaryOp::Mod,
            };
            let right = self.parse_postfix_expr()?;
            left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_postfix_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_atom_expr()?;
        loop {
            if self.check_symbol(".") {
                if matches!(self.peek_nth(1), Some(t) if t.value == "with") {
                    let span = self.current_span();
                    self.advance()?; // .
                    self.advance()?; // with
                    self.expect_symbol("(")?;
                    let binding = self.parse_with_binding()?;
                    self.expect_symbol(")")?;
                    expr = Expr::With { function: Box::new(expr), binding, span };
                } else {
                    let span = self.current_span();
                    self.advance()?; // .
                    let tok = self.advance()?;
                    let field = tok.value.clone();
                    expr = Expr::FieldAccess { object: Box::new(expr), field, span };
                }
            } else if self.check_symbol("(") {
                let span = self.current_span();
                self.advance()?;
                let args = self.parse_arg_list()?;
                self.expect_symbol(")")?;
                expr = Expr::Call { function: Box::new(expr), args, span };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_with_binding(&mut self) -> Result<WithBinding, ParseError> {
        if self.check_symbol("{") {
            self.advance()?;
            let mut fields = vec![];
            while !self.check_symbol("}") {
                if !fields.is_empty() && self.check_symbol(",") { self.advance()?; }
                let (name, span) = self.expect_lower_ident()?;
                self.expect_symbol(":")?;
                let value = self.parse_expr()?;
                fields.push(FieldValue { name, value: Box::new(value), span });
            }
            self.expect_symbol("}")?;
            Ok(WithBinding::Record(fields))
        } else {
            let (name, _) = self.expect_lower_ident()?;
            self.expect_symbol(":")?;
            let value = self.parse_expr()?;
            Ok(WithBinding::Named(name, Box::new(value)))
        }
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Arg>, ParseError> {
        let mut args = vec![];
        if self.check_symbol(")") { return Ok(args); }
        args.push(self.parse_arg()?);
        while self.check_symbol(",") {
            self.advance()?;
            if self.check_symbol(")") { break; }
            args.push(self.parse_arg()?);
        }
        Ok(args)
    }

    fn parse_arg(&mut self) -> Result<Arg, ParseError> {
        if let Some(t) = self.peek() {
            if t.token_type == TT_WORD && t.value.starts_with(|c: char| c.is_ascii_lowercase()) {
                if let Some(next) = self.peek_nth(1) {
                    if next.token_type == TT_SYMBOL && next.value == ":" {
                        let (name, _) = self.expect_lower_ident()?;
                        self.advance()?; // :
                        let value = self.parse_expr()?;
                        return Ok(Arg::Named(name, Box::new(value)));
                    }
                }
            }
        }
        Ok(Arg::Positional(Box::new(self.parse_expr()?)))
    }

    fn parse_atom_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        let tok = self.peek().ok_or_else(|| self.error_eof("expression"))?;

        match (tok.token_type, tok.value.as_str()) {
            (TT_INTEGER, _) => {
                let tok = self.advance()?;
                let n: i64 = tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid integer"))?;
                Ok(Expr::IntLiteral(n, span))
            }
            (TT_FLOAT, _) => {
                let tok = self.advance()?;
                let n: f64 = tok.value.parse().map_err(|_| ParseError::at(&tok, "invalid float"))?;
                Ok(Expr::FloatLiteral(n, span))
            }
            (TT_STRING, _) => {
                let tok = self.advance()?;
                let inner = tok.value[1..tok.value.len() - 1].to_string();
                Ok(Expr::StringLiteral(inner, span))
            }
            (TT_KEYWORD, "true") => { self.advance()?; Ok(Expr::BoolLiteral(true, span)) }
            (TT_KEYWORD, "false") => { self.advance()?; Ok(Expr::BoolLiteral(false, span)) }
            (TT_KEYWORD, "match") => self.parse_match_expr(),
            (TT_KEYWORD, "do") => self.parse_do_block(),
            (TT_KEYWORD, "select") => self.parse_select_expr(),
            (TT_KEYWORD, "clone") => self.parse_clone_expr(),
            (TT_KEYWORD, "let") => self.parse_let_expr(),
            (TT_KEYWORD, _) => Err(self.error_here(&format!("unexpected keyword '{}' in expression", tok.value))),
            (TT_SYMBOL, "_") => { self.advance()?; Ok(Expr::Wildcard(span)) }
            (TT_SYMBOL, "(") => {
                self.advance()?;
                let inner = self.parse_expr()?;
                self.expect_symbol(")")?;
                Ok(Expr::Paren(Box::new(inner), span))
            }
            (TT_SYMBOL, "{") => self.parse_record_expr(None),
            (TT_SYMBOL, "[") => self.parse_list_expr(),
            (TT_OPERATOR, "<-") => {
                self.advance()?;
                let channel = self.parse_atom_expr()?;
                Ok(Expr::ChannelRecv(Box::new(channel), span))
            }
            (TT_WORD, v) if v.starts_with(|c: char| c.is_ascii_uppercase()) => {
                let (name, span) = self.expect_upper_ident()?;
                if name == "Unit" && !self.check_symbol(".") && !self.check_symbol("{") && !self.check_symbol("<") {
                    return Ok(Expr::UnitLiteral(span));
                }
                if self.check_symbol(".") {
                    let mut parts = vec![name];
                    while self.check_symbol(".") {
                        self.advance()?;
                        let tok = self.advance()?;
                        parts.push(tok.value.clone());
                    }
                    // Channel.new<T>() — special syntax for channel construction
                    if parts.len() == 2 && parts[0] == "Channel" && parts[1] == "new" && self.check_symbol("<") {
                        self.advance()?; // consume <
                        let element_type = self.parse_type_expr()?;
                        self.expect_symbol(">")?;
                        self.expect_symbol("(")?;
                        self.expect_symbol(")")?;
                        return Ok(Expr::ChannelNew { element_type, span });
                    }
                    return Ok(Expr::QualifiedName(parts, span));
                }
                if self.check_symbol("{") && !self.peek_brace_arrow() {
                    return self.parse_record_expr(Some(Box::new(Expr::UpperVar(name, span.clone()))));
                }
                Ok(Expr::UpperVar(name, span))
            }
            (TT_WORD, _) => {
                let (name, span) = self.expect_lower_ident()?;
                Ok(Expr::Var(name, span))
            }
            _ => Err(self.error_here("expected expression")),
        }
    }

    fn parse_binary_op(&mut self) -> Result<BinaryOp, ParseError> {
        let tok = self.advance()?;
        match tok.value.as_str() {
            "==" => Ok(BinaryOp::Eq),
            "!=" => Ok(BinaryOp::Ne),
            ">=" => Ok(BinaryOp::Ge),
            "<=" => Ok(BinaryOp::Le),
            ">" => Ok(BinaryOp::Gt),
            "<" => Ok(BinaryOp::Lt),
            _ => Err(ParseError::at(&tok, "expected comparison operator")),
        }
    }

    // =========================================================================
    // Match
    // =========================================================================

    fn parse_match_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.expect_keyword("match")?;
        let scrutinee = self.parse_expr()?;
        self.expect_symbol("{")?;
        let mut arms = vec![];
        while !self.check_symbol("}") {
            let arm_span = self.current_span();
            let pattern = self.parse_pattern()?;
            self.expect_operator("->")?;
            let body = self.parse_expr()?;
            arms.push(MatchArm { pattern, body: Box::new(body), span: arm_span });
        }
        self.expect_symbol("}")?;
        Ok(Expr::Match { scrutinee: Box::new(scrutinee), arms, span })
    }

    // =========================================================================
    // Do block
    // =========================================================================

    fn parse_do_block(&mut self) -> Result<Expr, ParseError> {
        let span = self.expect_keyword("do")?;
        self.expect_symbol("{")?;
        let mut stmts: Vec<DoStmt> = vec![];
        let mut last_expr: Option<Expr> = None;

        while !self.check_symbol("}") {
            if self.check_keyword("let") {
                if let Some(prev) = last_expr.take() {
                    stmts.push(DoStmt::Expr(Box::new(prev)));
                }
                let lb = self.parse_let_binding()?;
                stmts.push(DoStmt::Let(lb));
                continue;
            }
            let expr = self.parse_expr()?;
            if let Some(prev) = last_expr.take() {
                stmts.push(DoStmt::Expr(Box::new(prev)));
            }
            last_expr = Some(expr);
        }
        self.expect_symbol("}")?;

        let final_expr = last_expr.ok_or_else(|| ParseError {
            message: "do block must have a final expression".to_string(),
            line: span.line, column: span.column,
        })?;
        Ok(Expr::DoBlock { stmts, final_expr: Box::new(final_expr), span })
    }

    // =========================================================================
    // Select
    // =========================================================================

    fn parse_select_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.expect_keyword("select")?;
        self.expect_symbol("{")?;
        let mut arms = vec![];
        let mut timeout = None;
        while !self.check_symbol("}") {
            if self.check_keyword("timeout") {
                let t_span = self.current_span();
                self.advance()?;
                self.expect_symbol("(")?;
                let duration = self.parse_expr()?;
                self.expect_symbol(")")?;
                self.expect_operator("->")?;
                let body = self.parse_expr()?;
                timeout = Some(TimeoutArm {
                    duration: Box::new(duration), body: Box::new(body), span: t_span,
                });
            } else {
                let arm_span = self.current_span();
                let binding = if self.check_symbol("_") {
                    self.advance()?; "_".to_string()
                } else {
                    let (name, _) = self.expect_lower_ident()?; name
                };
                self.expect_symbol("=")?;
                self.expect_operator("<-")?;
                let channel = self.parse_atom_expr()?;
                self.expect_operator("->")?;
                let body = self.parse_expr()?;
                arms.push(SelectArm {
                    binding, channel: Box::new(channel), body: Box::new(body), span: arm_span,
                });
            }
        }
        self.expect_symbol("}")?;
        Ok(Expr::Select { arms, timeout, span })
    }

    // =========================================================================
    // Clone, let, record, list
    // =========================================================================

    fn parse_clone_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.expect_keyword("clone")?;
        self.expect_symbol("(")?;
        let inner = self.parse_expr()?;
        self.expect_symbol(")")?;
        Ok(Expr::Clone(Box::new(inner), span))
    }

    fn parse_let_expr(&mut self) -> Result<Expr, ParseError> {
        Ok(Expr::Let(self.parse_let_binding()?))
    }

    fn parse_let_binding(&mut self) -> Result<LetBinding, ParseError> {
        let span = self.expect_keyword("let")?;
        let pattern = self.parse_pattern()?;
        let type_annotation = if self.check_symbol(":") {
            self.advance()?;
            Some(self.parse_type_expr()?)
        } else { None };
        self.expect_symbol("=")?;
        let value = self.parse_expr()?;
        Ok(LetBinding { pattern, type_annotation, value: Box::new(value), span })
    }

    fn parse_record_expr(&mut self, name: Option<Box<Expr>>) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_symbol("{")?;
        let mut fields = vec![];
        while !self.check_symbol("}") {
            if !fields.is_empty() && self.check_symbol(",") { self.advance()?; }
            if self.check_symbol("}") { break; }
            let (fname, fspan) = self.expect_lower_ident()?;
            self.expect_symbol(":")?;
            let value = self.parse_expr()?;
            fields.push(FieldValue { name: fname, value: Box::new(value), span: fspan });
        }
        self.expect_symbol("}")?;
        Ok(Expr::Record { name, fields, span })
    }

    fn parse_list_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_symbol("[")?;
        let mut items = vec![];
        if !self.check_symbol("]") {
            items.push(self.parse_expr()?);
            while self.check_symbol(",") {
                self.advance()?;
                if self.check_symbol("]") { break; }
                items.push(self.parse_expr()?);
            }
        }
        self.expect_symbol("]")?;
        Ok(Expr::List(items, span))
    }

    // =========================================================================
    // Module declarations
    // =========================================================================

    fn parse_module_decl(&mut self) -> Result<ModuleDecl, ParseError> {
        let span = self.expect_keyword("module")?;
        let (name, _) = self.expect_upper_ident()?;
        self.expect_symbol("{")?;
        let requires = if self.check_keyword("requires") || self.check_word("requires") {
            self.advance()?; self.expect_symbol(":")?;
            self.expect_symbol("{")?;
            let mut fields = vec![];
            while !self.check_symbol("}") {
                if !fields.is_empty() && self.check_symbol(",") { self.advance()?; }
                fields.push(self.parse_field_type_decl()?);
            }
            self.expect_symbol("}")?;
            Some(fields)
        } else { None };
        self.expect_word("provides")?;
        self.expect_symbol(":")?;
        self.expect_symbol("{")?;
        let mut provides = vec![];
        while !self.check_symbol("}") {
            provides.push(self.parse_module_fn_sig()?);
        }
        self.expect_symbol("}")?;
        self.expect_symbol("}")?;
        Ok(ModuleDecl { name, requires, provides, span })
    }

    fn parse_trusted_module_decl(&mut self) -> Result<TrustedModuleDecl, ParseError> {
        let span = self.expect_keyword("trusted")?;
        self.expect_keyword("module")?;
        let (name, _) = self.expect_upper_ident()?;
        self.expect_symbol("{")?;
        self.expect_word("provides")?;
        self.expect_symbol(":")?;
        self.expect_symbol("{")?;
        let mut provides = vec![];
        while !self.check_symbol("}") {
            provides.push(self.parse_module_fn_sig()?);
        }
        self.expect_symbol("}")?;
        self.expect_keyword("reason")?;
        self.expect_symbol(":")?;
        let (reason, _) = self.expect_string_literal()?;
        let fuzz = if self.check_keyword("fuzz") {
            self.advance()?; self.expect_symbol(":")?;
            Some(self.parse_fuzz_block()?)
        } else { None };
        self.expect_symbol("}")?;
        Ok(TrustedModuleDecl { name, provides, reason, fuzz, span })
    }

    fn parse_module_fn_sig(&mut self) -> Result<ModuleFnSig, ParseError> {
        let (name, span) = self.expect_lower_ident()?;
        self.expect_symbol(":")?;
        let effects = self.parse_effect_set()?;
        let input_type = self.parse_type_expr()?;
        self.expect_operator("->")?;
        let output_type = self.parse_type_expr()?;
        Ok(ModuleFnSig { name, effects, input_type, output_type, span })
    }

    fn parse_fuzz_block(&mut self) -> Result<Vec<FuzzDecl>, ParseError> {
        self.expect_symbol("{")?;
        let mut decls = vec![];
        while !self.check_symbol("}") {
            let (fn_name, span) = self.expect_lower_ident()?;
            self.expect_symbol(":")?;
            self.expect_keyword("inputs")?;
            self.expect_symbol("(")?;
            let mut input_types = vec![self.parse_type_expr()?];
            while self.check_symbol(",") {
                self.advance()?;
                input_types.push(self.parse_type_expr()?);
            }
            self.expect_symbol(")")?;
            self.expect_operator("->")?;
            let invariant = self.parse_fuzz_invariant()?;
            decls.push(FuzzDecl { fn_name, input_types, invariant, span });
        }
        self.expect_symbol("}")?;
        Ok(decls)
    }

    fn parse_fuzz_invariant(&mut self) -> Result<FuzzInvariant, ParseError> {
        let tok = self.advance()?;
        match tok.value.as_str() {
            "crashes_never" => Ok(FuzzInvariant::CrashesNever),
            "returns_result" => Ok(FuzzInvariant::ReturnsResult),
            "deterministic" => Ok(FuzzInvariant::Deterministic),
            _ => Err(ParseError::at(&tok, "expected crashes_never, returns_result, or deterministic")),
        }
    }

    // =========================================================================
    // Effect declarations
    // =========================================================================

    fn parse_effect_decl(&mut self) -> Result<EffectDecl, ParseError> {
        let span = self.expect_keyword("effect")?;
        let (name, _) = self.expect_upper_ident()?;
        self.expect_symbol("{")?;
        let mut methods = vec![];
        while !self.check_symbol("}") {
            methods.push(self.parse_module_fn_sig()?);
        }
        self.expect_symbol("}")?;
        Ok(EffectDecl { name, methods, span })
    }
}

/// Parse Keln source code into an AST.
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = crate::lexer::tokenize_filtered(source)
        .map_err(|e| ParseError {
            message: format!("Lexer error: {}", e),
            line: 0,
            column: 0,
        })?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_fn() {
        let source = r#"fn parsePort {
    Pure String -> Result<Port, PortError>
    in:  s
    out: Result.ok(s)
}"#;
        let program = parse(source).expect("should parse");
        assert_eq!(program.declarations.len(), 1);
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                assert_eq!(f.name, "parsePort");
                assert_eq!(f.signature.effects.effects, vec!["Pure"]);
                match &f.signature.input_type {
                    TypeExpr::Primitive(PrimitiveType::String, _) => {}
                    other => panic!("expected String input type, got {:?}", other),
                }
                match &f.signature.output_type {
                    TypeExpr::Generic { name, args, .. } => {
                        assert_eq!(name, "Result");
                        assert_eq!(args.len(), 2);
                    }
                    other => panic!("expected Result<Port, PortError> output type, got {:?}", other),
                }
                match &f.in_clause {
                    Pattern::Binding(name, _) => assert_eq!(name, "s"),
                    other => panic!("expected binding pattern, got {:?}", other),
                }
                // out: Result.ok(s)
                match &f.out_clause {
                    Expr::Call { function, args, .. } => {
                        match function.as_ref() {
                            Expr::QualifiedName(parts, _) => {
                                assert_eq!(parts, &["Result", "ok"]);
                            }
                            other => panic!("expected QualifiedName, got {:?}", other),
                        }
                        assert_eq!(args.len(), 1);
                    }
                    other => panic!("expected call expr, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_decl_sum() {
        let source = "type HttpMethod = GET | POST | PUT | DELETE";
        let program = parse(source).expect("should parse");
        assert_eq!(program.declarations.len(), 1);
        match &program.declarations[0] {
            TopLevelDecl::TypeDecl(td) => {
                assert_eq!(td.name, "HttpMethod");
                match &td.def {
                    TypeDef::Sum(variants) => {
                        assert_eq!(variants.len(), 4);
                        assert_eq!(variants[0].name, "GET");
                        assert_eq!(variants[1].name, "POST");
                        assert_eq!(variants[2].name, "PUT");
                        assert_eq!(variants[3].name, "DELETE");
                    }
                    other => panic!("expected Sum, got {:?}", other),
                }
            }
            other => panic!("expected TypeDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_decl_generic_alias() {
        let source = "type Ports = List<Port>";
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::TypeDecl(td) => {
                assert_eq!(td.name, "Ports");
                match &td.def {
                    TypeDef::Alias(TypeExpr::Generic { name, args, .. }) => {
                        assert_eq!(name, "List");
                        assert_eq!(args.len(), 1);
                    }
                    other => panic!("expected alias to List<Port>, got {:?}", other),
                }
            }
            other => panic!("expected TypeDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_match_expr() {
        let source = r#"fn isZero {
    Pure Int -> Bool
    in: n
    out: match n {
        0 -> true
        _ -> false
    }
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                assert_eq!(f.name, "isZero");
                match &f.out_clause {
                    Expr::Match { arms, .. } => {
                        assert_eq!(arms.len(), 2);
                    }
                    other => panic!("expected match, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_identifiers_with_digits_and_underscores() {
        let source = r#"fn add1 {
    Pure Int -> Int
    in: my_val
    out: my_val + 1
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                assert_eq!(f.name, "add1");
                match &f.in_clause {
                    Pattern::Binding(name, _) => assert_eq!(name, "my_val"),
                    other => panic!("expected binding, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_arithmetic() {
        let source = r#"fn addOne {
    Pure Int -> Int
    in: n
    out: n + 1
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                match &f.out_clause {
                    Expr::BinaryOp { op, .. } => {
                        assert!(matches!(op, BinaryOp::Add));
                    }
                    other => panic!("expected BinaryOp::Add, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_let_in_do() {
        let source = r#"fn example {
    Pure Int -> Int
    in: n
    out: do {
        let x = n + 1
        x + 2
    }
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                match &f.out_clause {
                    Expr::DoBlock { stmts, final_expr, .. } => {
                        assert_eq!(stmts.len(), 1);
                        assert!(matches!(&stmts[0], DoStmt::Let(_)));
                        assert!(matches!(final_expr.as_ref(), Expr::BinaryOp { .. }));
                    }
                    other => panic!("expected DoBlock, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_verify_given() {
        let source = r#"fn identity {
    Pure Int -> Int
    in: n
    out: n
    verify: {
        given(0) -> 0
        given(42) -> 42
    }
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                let verify = f.verify.as_ref().expect("should have verify");
                assert_eq!(verify.len(), 2);
                assert!(matches!(&verify[0], VerifyStmt::Given(_)));
                assert!(matches!(&verify[1], VerifyStmt::Given(_)));
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_record_construction() {
        let source = r#"fn makePoint {
    Pure { x: Int, y: Int } -> Point
    in: input
    out: Point { x: input.x, y: input.y }
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                assert_eq!(f.name, "makePoint");
                match &f.signature.input_type {
                    TypeExpr::Product(fields, _) => {
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].name, "x");
                        assert_eq!(fields[1].name, "y");
                    }
                    other => panic!("expected Product type, got {:?}", other),
                }
                match &f.out_clause {
                    Expr::Record { name: Some(n), fields, .. } => {
                        match n.as_ref() {
                            Expr::UpperVar(name, _) => assert_eq!(name, "Point"),
                            other => panic!("expected UpperVar, got {:?}", other),
                        }
                        assert_eq!(fields.len(), 2);
                    }
                    other => panic!("expected Record, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_with_variants() {
        let source = r#"type Result<T, E> = Ok(T) | Err(E)"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::TypeDecl(td) => {
                assert_eq!(td.name, "Result");
                assert_eq!(td.type_params, vec!["T", "E"]);
                match &td.def {
                    TypeDef::Sum(variants) => {
                        assert_eq!(variants.len(), 2);
                        assert_eq!(variants[0].name, "Ok");
                        assert!(matches!(&variants[0].payload, VariantPayload::Tuple(_)));
                        assert_eq!(variants[1].name, "Err");
                        assert!(matches!(&variants[1].payload, VariantPayload::Tuple(_)));
                    }
                    other => panic!("expected Sum, got {:?}", other),
                }
            }
            other => panic!("expected TypeDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_module_decl() {
        let source = r#"module Db {
    provides: {
        query: IO String -> String
    }
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::ModuleDecl(m) => {
                assert_eq!(m.name, "Db");
                assert!(m.requires.is_none());
                assert_eq!(m.provides.len(), 1);
                assert_eq!(m.provides[0].name, "query");
                assert_eq!(m.provides[0].effects.effects, vec!["IO"]);
            }
            other => panic!("expected ModuleDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_effect_decl() {
        let source = r#"effect Logging {
            log: Log String -> Unit
        }"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::EffectDecl(e) => {
                assert_eq!(e.name, "Logging");
                assert_eq!(e.methods.len(), 1);
                assert_eq!(e.methods[0].name, "log");
            }
            other => panic!("expected EffectDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_list_expr() {
        let source = r#"fn makeList {
    Pure Unit -> List<Int>
    in: _
    out: [1, 2, 3]
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                match &f.out_clause {
                    Expr::List(items, _) => assert_eq!(items.len(), 3),
                    other => panic!("expected List, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_pipeline() {
        let source = r#"fn process {
    Pure String -> String
    in: s
    out: s |> trim |> toUpper
}"#;
        let program = parse(source).expect("should parse");
        match &program.declarations[0] {
            TopLevelDecl::FnDecl(f) => {
                match &f.out_clause {
                    Expr::Pipeline { steps, .. } => {
                        assert_eq!(steps.len(), 2);
                    }
                    other => panic!("expected Pipeline, got {:?}", other),
                }
            }
            other => panic!("expected FnDecl, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_multiple_decls() {
        let source = r#"type Color = Red | Green | Blue
fn identity {
    Pure Int -> Int
    in: n
    out: n
}"#;
        let program = parse(source).expect("should parse");
        assert_eq!(program.declarations.len(), 2);
        assert!(matches!(&program.declarations[0], TopLevelDecl::TypeDecl(_)));
        assert!(matches!(&program.declarations[1], TopLevelDecl::FnDecl(_)));
    }
}
