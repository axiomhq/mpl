//! Hand-written recursive-descent parser that builds a lossless `rowan` tree.
//!
//! The parser is deliberately total: on unexpected input it records a
//! [`SyntaxError`] and wraps the offending tokens in an
//! [`SyntaxKind::ERROR_NODE`] rather than bailing out. Every token the lexer
//! produces — including trivia — ends up in the tree, so the result is a
//! faithful, byte-for-byte reconstruction of the input.

use std::ops::Range;

use rowan::{GreenNode, GreenNodeBuilder, TextRange, TextSize};

use super::lexer::lex;
use super::{SyntaxKind, SyntaxNode};

/// A parse diagnostic with the byte range it applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Human-readable description.
    pub message: String,
    /// Byte range in the source the error applies to.
    pub range: TextRange,
}

/// The result of [`parse`]: a green tree plus any recovery diagnostics.
#[derive(Debug)]
pub struct Parse {
    green: GreenNode,
    errors: Vec<SyntaxError>,
}

impl Parse {
    /// The root [`SyntaxNode`] of the parsed tree.
    #[must_use]
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// Recovery diagnostics collected during parsing.
    #[must_use]
    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }
}

/// Parse `input` into a lossless [`Parse`]. Never fails.
#[must_use]
pub fn parse(input: &str) -> Parse {
    let (tokens, unterminated) = lex(input);
    let mut p = Parser {
        text: input,
        tokens,
        pos: 0,
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
        unterminated,
    };
    p.parse_file();
    Parse {
        green: p.builder.finish(),
        errors: p.errors,
    }
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<(SyntaxKind, Range<usize>)>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
    /// Start offsets of strings the lexer found unterminated (it reached EOF
    /// still in string/interpolation mode). [`Parser::string`] turns each into
    /// an `unterminated string` diagnostic over `start..EOF`.
    unterminated: Vec<usize>,
}

impl Parser<'_> {
    // ── low-level token access ───────────────────────────────────

    /// Index of the `n`-th non-trivia token at or after the cursor.
    fn nth_index(&self, n: usize) -> Option<usize> {
        let mut seen = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            if !self.tokens[i].0.is_trivia() {
                if seen == n {
                    return Some(i);
                }
                seen += 1;
            }
            i += 1;
        }
        None
    }

    fn nth(&self, n: usize) -> Option<SyntaxKind> {
        self.nth_index(n).map(|i| self.tokens[i].0)
    }

    fn nth_text(&self, n: usize) -> &str {
        self.nth_index(n)
            .map_or("", |i| &self.text[self.tokens[i].1.clone()])
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.nth(0) == Some(kind)
    }

    fn nth_at(&self, n: usize, kind: SyntaxKind) -> bool {
        self.nth(n) == Some(kind)
    }

    /// At a plain identifier whose text equals `kw`.
    fn at_kw(&self, kw: &str) -> bool {
        self.at(SyntaxKind::IDENT) && self.nth_text(0) == kw
    }

    fn at_cmp(&self) -> bool {
        matches!(
            self.nth(0),
            Some(
                SyntaxKind::EQ_EQ
                    | SyntaxKind::BANG_EQ
                    | SyntaxKind::LT_EQ
                    | SyntaxKind::GT_EQ
                    | SyntaxKind::L_ANGLE
                    | SyntaxKind::R_ANGLE
            )
        )
    }

    fn at_end(&self) -> bool {
        self.nth_index(0).is_none()
    }

    // ── builder helpers ──────────────────────────────────────────

    /// Emit pending trivia tokens into the currently open node.
    fn eat_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].0.is_trivia() {
            let (kind, range) = self.tokens[self.pos].clone();
            self.builder.token(kind.into(), &self.text[range]);
            self.pos += 1;
        }
    }

    /// Open a node after attaching any leading trivia to the parent.
    fn start(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        self.builder.start_node(kind.into());
    }

    fn finish(&mut self) {
        self.builder.finish_node();
    }

    /// Consume the current token, keeping its lexer kind.
    fn bump(&mut self) {
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            let (kind, range) = self.tokens[self.pos].clone();
            self.builder.token(kind.into(), &self.text[range]);
            self.pos += 1;
        }
    }

    /// Consume the current token but relabel it to `kind` in the tree.
    fn bump_as(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            let range = self.tokens[self.pos].1.clone();
            self.builder.token(kind.into(), &self.text[range]);
            self.pos += 1;
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        let range = self.nth_index(0).map_or_else(
            || {
                let end = TextSize::new(u32_len(self.text));
                TextRange::empty(end)
            },
            |i| to_text_range(&self.tokens[i].1),
        );
        self.errors.push(SyntaxError {
            message: message.into(),
            range,
        });
    }

    /// Record an error at an explicit byte range (rather than the cursor).
    fn error_at(&mut self, range: TextRange, message: impl Into<String>) {
        self.errors.push(SyntaxError {
            message: message.into(),
            range,
        });
    }

    fn expect(&mut self, kind: SyntaxKind, message: &str) {
        if self.at(kind) {
            self.bump();
        } else {
            self.error(message);
        }
    }

    // ── grammar ──────────────────────────────────────────────────

    /// `file = directive* param* query`
    fn parse_file(&mut self) {
        self.builder.start_node(SyntaxKind::ROOT.into());
        self.eat_trivia();

        while self.at_kw("set") {
            self.directive();
        }
        while self.at_kw("param") {
            self.param_decl();
        }

        if !self.at_end() {
            self.query();
        }

        // Anything left over is unparseable: keep it in the tree.
        if !self.at_end() {
            self.error("unexpected trailing input");
            self.start(SyntaxKind::ERROR_NODE);
            while !self.at_end() {
                self.bump();
            }
            self.finish();
        }
        self.eat_trivia();
        self.finish();
    }

    /// `set ident (= (const | ident))? ;`
    fn directive(&mut self) {
        self.start(SyntaxKind::DIRECTIVE);
        self.bump_as(SyntaxKind::KEYWORD); // set
        if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
            self.bump();
        } else {
            self.error("expected directive name");
        }
        if self.at(SyntaxKind::EQ) {
            self.bump();
            self.directive_value();
        }
        self.expect(SyntaxKind::SEMICOLON, "expected `;`");
        self.finish();
    }

    fn directive_value(&mut self) {
        match self.nth(0) {
            Some(SyntaxKind::IDENT) if self.at_kw("true") || self.at_kw("false") => {
                self.bump_as(SyntaxKind::BOOL_LIT);
            }
            Some(SyntaxKind::IDENT) if self.at_kw("inf") => self.bump_as(SyntaxKind::INF_LIT),
            Some(SyntaxKind::PLUS | SyntaxKind::MINUS)
                if self.nth_at(1, SyntaxKind::INT)
                    || self.nth_at(1, SyntaxKind::FLOAT)
                    || (self.nth_at(1, SyntaxKind::IDENT) && self.nth_text(1) == "inf") =>
            {
                self.bump();
                if self.at_kw("inf") {
                    self.bump_as(SyntaxKind::INF_LIT);
                } else {
                    self.bump();
                }
            }
            Some(SyntaxKind::STRING_FRAGMENT) => self.string(),
            Some(
                SyntaxKind::INT | SyntaxKind::FLOAT | SyntaxKind::IDENT | SyntaxKind::ESCAPED_IDENT,
            ) => {
                self.bump();
            }
            _ => self.error("expected a directive value"),
        }
    }

    /// `param $name : param_type ;`
    fn param_decl(&mut self) {
        self.start(SyntaxKind::PARAM_DECL);
        self.bump_as(SyntaxKind::KEYWORD); // param
        self.expect(SyntaxKind::PARAM_IDENT, "expected `$name`");
        self.expect(SyntaxKind::COLON, "expected `:`");
        self.param_type();
        self.expect(SyntaxKind::SEMICOLON, "expected `;`");
        self.finish();
    }

    /// `param_type = Option< inner > | inner`  (lenient: any type ident accepted)
    fn param_type(&mut self) {
        self.start(SyntaxKind::PARAM_TYPE);
        if self.at_kw("Option") {
            self.bump_as(SyntaxKind::TYPE_NAME);
            self.expect(SyntaxKind::L_ANGLE, "expected `<`");
            if self.at(SyntaxKind::IDENT) {
                self.bump_as(SyntaxKind::TYPE_NAME);
            } else {
                self.error("expected a type");
            }
            self.expect(SyntaxKind::R_ANGLE, "expected `>`");
        } else if self.at(SyntaxKind::IDENT) {
            self.bump_as(SyntaxKind::TYPE_NAME);
        } else {
            self.error("expected a param type");
        }
        self.finish();
    }

    /// `query = simple_query | compute_query`
    fn query(&mut self) {
        if self.at(SyntaxKind::L_PAREN) {
            self.compute_query();
        } else {
            self.simple_query();
        }
    }

    /// `compute_query = ( query , query ,? ) (| compute …) pipe_rule*`
    fn compute_query(&mut self) {
        self.start(SyntaxKind::COMPUTE_QUERY);
        self.expect(SyntaxKind::L_PAREN, "expected `(`");
        self.query();
        self.expect(SyntaxKind::COMMA, "expected `,` between compute queries");
        self.query();
        if self.at(SyntaxKind::COMMA) {
            self.bump();
        }
        self.expect(SyntaxKind::R_PAREN, "expected `)`");
        while self.at(SyntaxKind::PIPE) {
            self.pipe_rule();
        }
        self.finish();
    }

    /// `simple_query = source pipe_rule*`
    fn simple_query(&mut self) {
        self.start(SyntaxKind::QUERY);
        self.source();
        while self.at(SyntaxKind::PIPE) {
            self.pipe_rule();
        }
        self.finish();
    }

    /// `source = metric_id time_range? as?`
    fn source(&mut self) {
        self.start(SyntaxKind::SOURCE);
        self.metric_id();
        if self.at(SyntaxKind::L_BRACK) {
            self.time_range();
        }
        if self.at_kw("as") {
            self.as_clause();
        }
        self.finish();
    }

    fn metric_id(&mut self) {
        self.start(SyntaxKind::METRIC_ID);
        // dataset
        self.start(SyntaxKind::DATASET);
        let dataset_range = self.nth_index(0).map(|i| to_text_range(&self.tokens[i].1));
        if self.at(SyntaxKind::PARAM_IDENT)
            || self.at(SyntaxKind::IDENT)
            || self.at(SyntaxKind::ESCAPED_IDENT)
        {
            self.bump();
        } else {
            self.error("expected a dataset name");
        }
        self.finish();
        // A missing `:` is reported on the dataset itself (matching the old
        // `pest` error position); a present `:` with a missing metric name is
        // reported at the following token / EOF.
        if self.at(SyntaxKind::COLON) {
            self.bump();
            self.start(SyntaxKind::METRIC_NAME);
            if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
                self.bump();
            } else {
                self.error("expected a metric name");
            }
            self.finish();
        } else if let Some(range) = dataset_range {
            self.error_at(
                range,
                "expected a metric identifier (e.g. `dataset:metric`)",
            );
        } else {
            self.error("expected a metric identifier (e.g. `dataset:metric`)");
        }
        self.finish();
    }

    /// `time_range = [ time .. time? ]`
    fn time_range(&mut self) {
        self.start(SyntaxKind::TIME_RANGE);
        self.bump(); // [
        self.time();
        self.expect(SyntaxKind::DOT_DOT, "expected `..`");
        if !self.at(SyntaxKind::R_BRACK) && !self.at_end() {
            self.time();
        }
        self.expect(SyntaxKind::R_BRACK, "expected `]`");
        self.finish();
    }

    /// One absolute or relative point in time inside a `time_range`.
    fn time(&mut self) {
        if self.at(SyntaxKind::RFC3339) {
            self.start(SyntaxKind::TIME_RFC3339);
            self.bump();
            self.finish();
        } else if self.at(SyntaxKind::PLUS) || self.at(SyntaxKind::MINUS) {
            // `+1h` / `-30m` time modifier.
            self.start(SyntaxKind::TIME_MODIFIER);
            self.bump(); // sign
            self.rel_time_inner();
            self.finish();
        } else if self.at(SyntaxKind::INT) {
            // A digit run followed by a unit is a relative time; otherwise it
            // is a unix timestamp.
            if self.nth_at(1, SyntaxKind::IDENT) {
                self.rel_time();
            } else {
                self.start(SyntaxKind::TIME_TIMESTAMP);
                self.bump();
                self.finish();
            }
        } else if self.at(SyntaxKind::PARAM_IDENT) {
            self.rel_time();
        } else {
            self.error("expected a time");
        }
    }

    /// `rel_time = digits unit | $param`
    fn rel_time(&mut self) {
        self.start(SyntaxKind::REL_TIME);
        if self.at(SyntaxKind::PARAM_IDENT) {
            self.bump();
        } else {
            self.rel_time_inner();
        }
        self.finish();
    }

    /// The `digits unit` body of a relative time (no `$param`).
    fn rel_time_inner(&mut self) {
        if self.at(SyntaxKind::INT) {
            self.bump();
            if self.at(SyntaxKind::IDENT) {
                self.bump_as(SyntaxKind::TIME_UNIT);
            } else {
                self.error("expected a time unit");
            }
        } else {
            self.error("expected a relative time");
        }
    }

    fn as_clause(&mut self) {
        self.start(SyntaxKind::AS_CLAUSE);
        self.bump_as(SyntaxKind::KEYWORD); // as
        self.start(SyntaxKind::METRIC_NAME);
        if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
            self.bump();
        } else {
            self.error("expected a metric name after `as`");
        }
        self.finish();
        self.finish();
    }

    /// Dispatch a `| …` pipe rule, recovering on unknown rules.
    fn pipe_rule(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.eat_trivia();
        self.bump(); // |
        if self.at_kw("filter") || self.at_kw("where") {
            self.wrap(checkpoint, SyntaxKind::FILTER_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.filter_or();
            self.finish();
        } else if self.at_kw("ifdef") {
            self.wrap(checkpoint, SyntaxKind::IFDEF_RULE);
            self.ifdef_body();
            self.finish();
        } else if self.at_kw("sample") {
            self.wrap(checkpoint, SyntaxKind::SAMPLE_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.number();
            self.finish();
        } else if self.at_kw("align") {
            self.wrap(checkpoint, SyntaxKind::ALIGN_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.align_body();
            self.finish();
        } else if self.at_kw("map") {
            self.wrap(checkpoint, SyntaxKind::MAP_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.map_body();
            self.finish();
        } else if self.at_kw("group") {
            self.wrap(checkpoint, SyntaxKind::GROUP_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.group_body();
            self.finish();
        } else if self.at_kw("bucket") {
            self.wrap(checkpoint, SyntaxKind::BUCKET_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.bucket_body();
            self.finish();
        } else if self.at_kw("join") {
            self.wrap(checkpoint, SyntaxKind::JOIN_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.join_body();
            self.finish();
        } else if self.at_kw("replace") {
            self.wrap(checkpoint, SyntaxKind::REPLACE_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.replace_body();
            self.finish();
        } else if self.at_kw("compute") {
            self.wrap(checkpoint, SyntaxKind::COMPUTE_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.compute_body();
            self.finish();
        } else if self.at_kw("extend") {
            self.wrap(checkpoint, SyntaxKind::EXTEND_RULE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.extend_body();
            self.finish();
        } else if self.at_kw("as") {
            self.wrap(checkpoint, SyntaxKind::AS_CLAUSE);
            self.bump_as(SyntaxKind::KEYWORD);
            self.start(SyntaxKind::METRIC_NAME);
            if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
                self.bump();
            } else {
                self.error("expected a metric name after `as`");
            }
            self.finish();
            self.finish();
        } else {
            self.wrap(checkpoint, SyntaxKind::ERROR_NODE);
            self.error("unsupported pipe rule");
            while !self.at_end() && !self.at(SyntaxKind::PIPE) {
                self.bump();
            }
            self.finish();
        }
    }

    /// Open a node at `checkpoint`, capturing the already-bumped `|`.
    fn wrap(&mut self, checkpoint: rowan::Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(checkpoint, kind.into());
    }

    /// `ifdef ( $param ) { filter_expr } (else { filter_expr })?`
    fn ifdef_body(&mut self) {
        self.bump_as(SyntaxKind::KEYWORD); // ifdef
        self.expect(SyntaxKind::L_PAREN, "expected `(`");
        self.expect(SyntaxKind::PARAM_IDENT, "expected a `$param`");
        self.expect(SyntaxKind::R_PAREN, "expected `)`");
        self.ifdef_branch();
        if self.at_kw("else") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.ifdef_branch();
        }
    }

    /// `{ (filter|where) filter_or }`
    fn ifdef_branch(&mut self) {
        self.expect(SyntaxKind::L_BRACE, "expected `{`");
        if self.at_kw("filter") || self.at_kw("where") {
            self.bump_as(SyntaxKind::KEYWORD);
        } else {
            self.error("expected `where`");
        }
        self.filter_or();
        self.expect(SyntaxKind::R_BRACE, "expected `}`");
    }

    /// `align (to rel)? (over rel)? using func`
    fn align_body(&mut self) {
        if self.at_kw("to") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.rel_time();
        }
        if self.at_kw("over") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.rel_time();
        }
        if self.at_kw("using") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.func();
        } else {
            self.error("expected `using`");
        }
    }

    /// `map (calc_op number | func ( ( number ) )?)`
    fn map_body(&mut self) {
        if matches!(
            self.nth(0),
            Some(SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH)
        ) {
            self.bump(); // calc op
            self.number();
        } else {
            self.func();
            if self.at(SyntaxKind::L_PAREN) {
                self.bump();
                self.number();
                self.expect(SyntaxKind::R_PAREN, "expected `)`");
            }
        }
    }

    /// `group (by tags)? using func`
    fn group_body(&mut self) {
        if self.at_kw("by") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.tags();
        }
        if self.at_kw("using") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.func();
        } else {
            self.error("expected `using`");
        }
    }

    /// `bucket (by tags)? (to rel)? using bucket_fn_call`
    fn bucket_body(&mut self) {
        if self.at_kw("by") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.tags();
        }
        if self.at_kw("to") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.rel_time();
        }
        if self.at_kw("using") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.bucket_fn_call();
        } else {
            self.error("expected `using`");
        }
    }

    /// `name ( specs )` where name is a bucket function keyword.
    fn bucket_fn_call(&mut self) {
        self.start(SyntaxKind::BUCKET_FN);
        if self.at(SyntaxKind::IDENT) {
            self.bump_as(SyntaxKind::KEYWORD);
        } else {
            self.error("expected a bucket function");
        }
        self.expect(SyntaxKind::L_PAREN, "expected `(`");
        self.bucket_spec();
        while self.at(SyntaxKind::COMMA) {
            self.bump();
            self.bucket_spec();
        }
        self.expect(SyntaxKind::R_PAREN, "expected `)`");
        self.finish();
    }

    fn bucket_spec(&mut self) {
        self.start(SyntaxKind::BUCKET_SPEC);
        if self.at(SyntaxKind::IDENT) {
            self.bump_as(SyntaxKind::KEYWORD);
        } else if matches!(
            self.nth(0),
            Some(SyntaxKind::INT | SyntaxKind::FLOAT | SyntaxKind::PLUS | SyntaxKind::MINUS)
        ) {
            self.number();
        } else {
            self.error("expected a bucket spec");
        }
        self.finish();
    }

    /// `join tags from metric_id by tags` (lowered to `NotSupported`).
    fn join_body(&mut self) {
        self.tags();
        if self.at_kw("from") {
            self.bump_as(SyntaxKind::KEYWORD);
        } else {
            self.error("expected `from`");
        }
        self.metric_id();
        if self.at_kw("by") {
            self.bump_as(SyntaxKind::KEYWORD);
        } else {
            self.error("expected `by`");
        }
        self.tags();
    }

    /// `replace …` (lowered to `NotSupported`). Consumed loosely: every form
    /// reduces to the same error, so the precise shape is irrelevant.
    fn replace_body(&mut self) {
        while !self.at_end() && !self.at(SyntaxKind::PIPE) {
            self.bump();
        }
    }

    /// `compute metric_name using compute_fn`
    fn compute_body(&mut self) {
        self.start(SyntaxKind::METRIC_NAME);
        if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
            self.bump();
        } else {
            self.error("expected a metric name");
        }
        self.finish();
        if self.at_kw("using") {
            self.bump_as(SyntaxKind::KEYWORD);
        } else {
            self.error("expected `using`");
        }
        if matches!(
            self.nth(0),
            Some(SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH)
        ) {
            self.start(SyntaxKind::FUNC);
            self.bump();
            self.finish();
        } else {
            self.func();
        }
    }

    /// `extend extend_expr (, extend_expr)*`
    fn extend_body(&mut self) {
        self.extend_expr();
        while self.at(SyntaxKind::COMMA) {
            self.bump();
            self.extend_expr();
        }
    }

    fn extend_expr(&mut self) {
        self.start(SyntaxKind::EXTEND_EXPR);
        if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
            self.bump();
        } else {
            self.error("expected a tag name");
        }
        self.expect(SyntaxKind::EQ, "expected `=`");
        self.expr();
        self.finish();
    }

    /// `tags = ident (, ident)*`
    fn tags(&mut self) {
        self.start(SyntaxKind::TAGS);
        if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
            self.bump();
            while self.at(SyntaxKind::COMMA) {
                self.bump();
                if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
                    self.bump();
                } else {
                    self.error("expected a tag name");
                    break;
                }
            }
        } else {
            self.error("expected a tag name");
        }
        self.finish();
    }

    fn func(&mut self) {
        self.start(SyntaxKind::FUNC);
        if self.at(SyntaxKind::IDENT) {
            self.bump();
            while self.at(SyntaxKind::COLON_COLON) {
                self.bump();
                if self.at(SyntaxKind::IDENT) {
                    self.bump();
                } else {
                    self.error("expected a function name");
                    break;
                }
            }
        } else {
            self.error("expected a function");
        }
        self.finish();
    }

    /// `number = (+|-)? (int | float) | (+|-)? inf`
    fn number(&mut self) {
        self.start(SyntaxKind::NUMBER);
        if self.at(SyntaxKind::PLUS) || self.at(SyntaxKind::MINUS) {
            self.bump();
        }
        if self.at_kw("inf") {
            self.bump_as(SyntaxKind::INF_LIT);
        } else if self.at(SyntaxKind::FLOAT) || self.at(SyntaxKind::INT) {
            self.bump();
        } else {
            self.error("expected a number");
        }
        self.finish();
    }

    // ── filter expression ────────────────────────────────────────

    fn filter_or(&mut self) {
        self.start(SyntaxKind::FILTER_OR);
        self.filter_and();
        while self.at_kw("or") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.filter_and();
        }
        self.finish();
    }

    fn filter_and(&mut self) {
        self.start(SyntaxKind::FILTER_AND);
        self.filter_not();
        while self.at_kw("and") {
            self.bump_as(SyntaxKind::KEYWORD);
            self.filter_not();
        }
        self.finish();
    }

    fn filter_not(&mut self) {
        self.start(SyntaxKind::FILTER_NOT);
        if self.at_kw("not") {
            self.bump_as(SyntaxKind::KEYWORD);
        }
        self.filter_clause();
        self.finish();
    }

    fn filter_clause(&mut self) {
        self.start(SyntaxKind::FILTER_CLAUSE);
        if self.at(SyntaxKind::L_PAREN) {
            self.bump();
            self.filter_or();
            self.expect(SyntaxKind::R_PAREN, "expected `)`");
        } else {
            self.filter_atom();
        }
        self.finish();
    }

    fn filter_atom(&mut self) {
        self.start(SyntaxKind::FILTER_ATOM);
        if self.at(SyntaxKind::IDENT) || self.at(SyntaxKind::ESCAPED_IDENT) {
            self.bump(); // tag
        } else {
            self.error("expected a tag name");
        }

        if self.at_kw("is") {
            self.start(SyntaxKind::IS_FILTER);
            self.bump_as(SyntaxKind::KEYWORD);
            if self.at(SyntaxKind::IDENT) {
                self.bump_as(SyntaxKind::TYPE_NAME);
            } else {
                self.error("expected a tag type");
            }
            self.finish();
        } else if self.at_cmp() {
            // The `== #/re/` vs `== $param` ambiguity that pest defers to a
            // later pass is resolved here at the token level: a regex literal
            // lexes distinctly from a `$param`/value, so an `==`/`!=` followed
            // by a `REGEX` token is a regex filter, everything else a value
            // filter.
            let eq_or_ne = self.at(SyntaxKind::EQ_EQ) || self.at(SyntaxKind::BANG_EQ);
            let regex_rhs =
                self.nth_at(1, SyntaxKind::REGEX) || self.nth_at(1, SyntaxKind::REGEX_REPLACE);
            if eq_or_ne && regex_rhs {
                self.start(SyntaxKind::REGEX_FILTER);
                self.bump_as(SyntaxKind::CMP_OP);
                self.bump(); // regex literal
                self.finish();
            } else {
                self.start(SyntaxKind::VALUE_FILTER);
                self.bump_as(SyntaxKind::CMP_OP);
                self.expr();
                self.finish();
            }
        } else {
            self.error("expected a comparison or `is`");
        }
        self.finish();
    }

    /// Build a `STRING` node, descending into `${ … }` interpolations.
    ///
    /// The tokenizer pre-split the literal into `STRING_FRAGMENT` runs, `${`/`}`
    /// delimiters and embedded-expression tokens; here we only shape them into
    /// a node, reusing [`Parser::expr`] for each interpolated expression.
    ///
    /// The same shaping applies to an *unterminated* string (no closing quote):
    /// its fragments / interpolation are still structured, and an `unterminated
    /// string` diagnostic is recorded over the whole literal (`start..EOF`) so
    /// `compile` keeps rejecting it. The lexer flags such a string in
    /// [`Parser::unterminated`] when it reaches EOF still in string/interpolation
    /// mode; the node's extent still runs to EOF, exactly as before.
    fn string(&mut self) {
        let start = self.nth_index(0).map(|i| self.tokens[i].1.start);
        self.start(SyntaxKind::STRING);
        self.bump(); // leading STRING_FRAGMENT (carries the opening quote)
        while self.at(SyntaxKind::DOLLAR_BRACE) {
            self.bump(); // ${
            self.expr();
            self.expect(SyntaxKind::R_BRACE, "expected `}` to close `${`");
            if self.at(SyntaxKind::STRING_FRAGMENT) {
                self.bump(); // text after this interpolation
            }
        }
        self.finish();
        if let Some(start) = start
            && self.unterminated.contains(&start)
        {
            self.error_at(
                to_text_range(&(start..self.text.len())),
                "unterminated string",
            );
        }
    }

    /// `expr = const | param_ident | ident | string`
    fn expr(&mut self) {
        self.start(SyntaxKind::EXPR);
        match self.nth(0) {
            Some(SyntaxKind::STRING_FRAGMENT) => self.string(),
            Some(SyntaxKind::PARAM_IDENT | SyntaxKind::ESCAPED_IDENT) => {
                self.bump();
            }
            Some(SyntaxKind::PLUS | SyntaxKind::MINUS)
                if self.nth_at(1, SyntaxKind::INT) || self.nth_at(1, SyntaxKind::FLOAT) =>
            {
                self.bump();
                self.bump();
            }
            Some(SyntaxKind::PLUS | SyntaxKind::MINUS)
                if self.nth_at(1, SyntaxKind::IDENT) && self.nth_text(1) == "inf" =>
            {
                self.bump(); // sign
                self.bump_as(SyntaxKind::INF_LIT);
            }
            Some(SyntaxKind::IDENT) if self.at_kw("true") || self.at_kw("false") => {
                self.bump_as(SyntaxKind::BOOL_LIT);
            }
            Some(SyntaxKind::IDENT) if self.at_kw("inf") => self.bump_as(SyntaxKind::INF_LIT),
            Some(SyntaxKind::INT | SyntaxKind::FLOAT | SyntaxKind::IDENT) => self.bump(),
            _ => self.error("expected a value"),
        }
        self.finish();
    }
}

fn u32_len(text: &str) -> u32 {
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

fn to_text_range(range: &Range<usize>) -> TextRange {
    TextRange::new(
        TextSize::new(u32::try_from(range.start).unwrap_or(u32::MAX)),
        TextSize::new(u32::try_from(range.end).unwrap_or(u32::MAX)),
    )
}
