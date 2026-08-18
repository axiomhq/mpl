use std::ops::Deref;

use miette::{Diagnostic, MietteDiagnostic, SourceSpan};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxNodeChildren, SyntaxToken};

use crate::{
    query::{ParamType, TagType, TerminalParamType},
    syntax_tree::{self, Lang, SyntaxError, SyntaxKind, SyntaxNode, SyntaxTree},
    tags::TagValue,
};

#[cfg(test)]
mod tests;

/// Represents a parser warning.
#[derive(thiserror::Error, Debug, Diagnostic)]
pub enum AstWarning {
    /// The parser encountered an unknown escape sequence.
    #[error("Unknown escape sequence")]
    UnknownEscapeSequence {
        /// The unknown escape sequence character.
        char: char,
        /// The span of the unknown escape sequence.
        #[label(
            "The escape sequence `\\{char}` is unknown and will be represented by stripping the \\ leaving the following character unchanged. This behavior is subject to change."
        )]
        span: SourceSpan,
    },
    /// Time must be second aligned
    #[error("Time must be second aligned")]
    TimeNotSecondAligned {
        /// The nanosecond time provided
        time: u64,
        /// the span of hte invalid time
        #[label(
            "A duration of {time}ns does not cleanly translate into seconds, any sub second interval will be truncated"
        )]
        span: SourceSpan,
    },
}
impl AstWarning {
    pub(crate) fn span(&self) -> SourceSpan {
        match self {
            AstWarning::UnknownEscapeSequence { span, .. }
            | AstWarning::TimeNotSecondAligned { span, .. } => *span,
        }
    }
}
#[derive(thiserror::Error, Debug, Diagnostic)]
/// Represents a parser error.
pub enum AstError {
    /// The input could not be parsed.
    #[error("invalid syntax")]
    #[diagnostic(code(mpl_lang::invalid_syntax))]
    InvalidSyntax(SyntaxError),

    /// Failed to create string
    #[error("failed to create string")]
    #[diagnostic(code(mpl_lang::failed_to_create_string))]
    FailedToCreateString {
        /// The error that occurred while creating the string.
        error: strumbra::Error,
        /// The source span of the failed string creation.
        #[label("failed to create string: {error}")]
        span: SourceSpan,
    },

    /// Not implemented.
    #[error("not implemented")]
    #[diagnostic(code(mpl_lang::not_implemented))]
    NotImplemented {
        /// The range of the not implemented node.
        #[label("not implemented {kind:?}")]
        span: SourceSpan,
        /// The kind of the not implemented node.
        kind: SyntaxKind,
    },

    /// Expected a syntax rule but found something else.
    #[error("unexpected syntax rule")]
    #[diagnostic(code(mpl_lang::unexpected_syntax_rule))]
    UnexpectedSyntaxRule {
        /// The expected syntax kind(s).
        expected: &'static [SyntaxKind],
        /// The actual syntax kind found.
        found: SyntaxKind,
        /// The source span of the unexpected syntax rule.
        #[label("Unexpected syntax rule {found:?}, expected one of {expected:?}")]
        span: SourceSpan,
    },

    /// Expected a syntax rule but found something else.
    #[error("unexpected syntax rule")]
    #[diagnostic(code(mpl_lang::unexpected_syntax_rule))]
    UnexpectedSyntaxRuleOne {
        /// The expected syntax kind(s).
        expected: SyntaxKind,
        /// The actual syntax kind found.
        found: SyntaxKind,
        /// The source span of the unexpected syntax rule.
        #[label("Unexpected syntax rule {found:?}, expected {expected:?}")]
        span: SourceSpan,
    },

    /// Garbage at the end a rule.
    #[error("garbage at end of input")]
    #[diagnostic(code(mpl_lang::garbage_at_end))]
    GarbageAtEndOfRule {
        /// The source span of the garbage.
        #[label("garbage at end of rule")]
        span: SourceSpan,
    },

    /// Missing token
    #[error("Expected token of kind {expected:?} but it's missing")]
    #[diagnostic(code(mpl_lang::garbage_at_end))]
    MissingToken {
        /// The expected syntax kind
        expected: SyntaxKind,
        /// The source span of the garbage.
        #[label("missing token of kind {expected:?}")]
        span: SourceSpan,
    },

    /// The integer constant is not a valid integer.
    #[error("invalid integer constant")]
    #[diagnostic(code(mpl_lang::invalid_integer_constant))]
    InvalidIntegerConstant {
        /// The source span of the invalid integer constant.
        #[label("invalid integer constant")]
        span: SourceSpan,
    },

    /// The float constant is not a valid float.
    #[error("invalid float constant")]
    #[diagnostic(code(mpl_lang::invalid_float_constant))]
    InvalidFloatConstant {
        /// The source span of the invalid float constant.
        #[label("invalid float constant")]
        span: SourceSpan,
    },

    /// The bool constant is not a valid bool.
    #[error("invalid bool constant")]
    #[diagnostic(code(mpl_lang::invalid_bool_constant))]
    InvalidBoolConstant {
        /// The source span of the invalid bool constant.
        #[label("invalid bool constant")]
        span: SourceSpan,
    },

    /// The keyword is not the expected keyword.
    #[error("unexpected keyword")]
    #[diagnostic(code(mpl_lang::unexpected_keyword))]
    UnexpectedKeyword {
        /// The expected keyword.
        expected: &'static str,
        /// The found keyword.
        found: String,
        /// The source span of the found keyword.
        #[label("unexpected keyword {found} expected {expected}")]
        span: SourceSpan,
    },

    /// The MPL type is not a valid MPL type.
    #[error("invalid MPL type")]
    #[diagnostic(code(mpl_lang::invalid_mpl_type))]
    InvalidType {
        /// The source span of the invalid MPL type.
        #[label("invalid MPL type: {t}")]
        span: SourceSpan,
        /// The invalid MPL type.
        t: String,
    },

    /// Nested option types are not supported.
    #[error("nested option types are not supported")]
    #[diagnostic(code(mpl_lang::nested_option_types))]
    NestedOption {
        /// The source span of the invalid MPL type.
        #[label("nested option types are not supported")]
        span: SourceSpan,
    },

    /// The rule is not recognized.
    #[error("unknown rule")]
    #[diagnostic(code(mpl_lang::unknown_rule))]
    UnknownRule {
        /// The source span of the unknown rule.
        #[label("unknown rule")]
        span: SourceSpan,
        /// The kind of the unknown rule.
        kind: SyntaxKind,
    },

    /// Expected a constant expression, but got something else.
    #[error("expected const")]
    #[diagnostic(code(mpl_lang::expected_const))]
    ExpectedConst {
        /// The source span of the invalid constant expression.
        #[label("expected const")]
        span: SourceSpan,
    },
    /// The duration is negative.
    #[error("negative duration")]
    #[diagnostic(code(mpl_lang::negative_duration))]
    NegativeDuration {
        /// The source span of the negative duration.
        #[label("negative duration")]
        span: SourceSpan,
    },
    /// The time unit is invalid.
    #[error("invalid time unit")]
    #[diagnostic(code(mpl_lang::invalid_time_unit))]
    InvalidTimeUnit {
        /// The source span of the invalid time unit.
        #[label("invalid time unit")]
        span: SourceSpan,
    },
    /// The filter is empty.
    #[error("empty filter")]
    #[diagnostic(code(mpl_lang::empty_filter))]
    EmptyFilter {
        /// The source span of the empty filter.
        #[label("empty filter")]
        span: SourceSpan,
    },
    /// The string is invalid.
    #[error("invalid string")]
    #[diagnostic(code(mpl_lang::invalid_string))]
    InvalidString {
        /// The source span of the invalid string.
        #[label("invalid string")]
        span: SourceSpan,
    },

    /// The regex is invalid.
    #[error("invalid regex: {message}")]
    #[diagnostic(code(mpl_lang::invalid_regex))]
    InvalidRegex {
        /// The source span of the invalid regex.
        #[label("invalid regex: {message}")]
        span: SourceSpan,
        /// The error message from the regex parser.
        message: String,
    },
    /// The identifier is invalid.
    #[error("invalid ident")]
    #[diagnostic(code(mpl_lang::invalid_ident))]
    InvalidIdent {
        /// The source span of the invalid ident.
        #[label("invalid ident")]
        span: SourceSpan,
    },
    /// The variable is invalid.
    #[error("invalid variable")]
    #[diagnostic(code(mpl_lang::invalid_variable))]
    InvalidVariable {
        /// The source span of the invalid variable.
        #[label("invalid variable")]
        span: SourceSpan,
    },
    /// Unicode escape sequence is invalid.
    #[error("unicode escape sequences are not supported")]
    #[diagnostic(code(mpl_lang::unicode_escape_sequence))]
    UnicodeEscape {
        /// The source span of the invalid escape sequence.
        #[label("unicode escape sequences are not supported")]
        span: SourceSpan,
    },
    /// Sub second durations are not supported
    #[error("Sub second intervals are not supported")]
    TimeTooSmall {
        /// The nanosecond interval given
        time: u64,
        /// The source span of the invalid duration.
        #[label("Sub second intervals are not supported, {time}ns provided")]
        span: SourceSpan,
    },
}

impl AstError {
    /// Converts this error into a [`MietteDiagnostic`].
    #[must_use]
    pub fn to_diagnostic(&self) -> MietteDiagnostic {
        if let AstError::InvalidSyntax(error) = self {
            error.to_diagnostic()
        } else {
            MietteDiagnostic {
                message: self.to_string(),
                code: self.code().map(|code| code.to_string()),
                severity: self.severity(),
                help: self.help().map(|help| help.to_string()),
                url: self.url().map(|url| url.to_string()),
                labels: self.labels().map(Iterator::collect),
            }
        }
    }
}

// NOTE: This error isn't user facing it's just for internal aborts.
/// Represents a parser error.
#[derive(Debug)]
pub struct Error(pub &'static str);

/// AST parser result type.
pub type Result<T> = std::result::Result<T, Error>;

/// a function call
#[derive(Debug)]
pub struct FunctionCall {
    /// The syntax node of the function call
    pub node: SyntaxNode,
    /// The function name, split into parts for nested functions.
    pub name: Vec<Ident>,
    /// The function arguments.
    pub args: Vec<SyntaxExpr>,
}
impl FunctionCall {
    pub(crate) fn span(&self) -> SourceSpan {
        self.node.span()
    }
}

/// A `param` declaration.
#[derive(Debug)]
pub struct Param {
    /// The corresponding syntax node
    pub node: SyntaxNode,
    /// The parameter name, without the leading `$`.
    pub name: Variable,
    /// The declared type.
    pub ty: ParamType,
}

/// A `set` directive.
#[derive(Debug, Clone)]
pub struct Directive {
    /// The corresponding syntax node
    pub node: SyntaxNode,
    /// The directive name.
    pub name: Ident,
    /// The assigned constant, absent for a bare `set name;`.
    pub value: Option<TagValue>,
}

/// One top-level item of a lowered program.
#[derive(Debug)]
pub enum Part {
    /// A `set` directive.
    Directive(Directive),
    /// A `param` declaration.
    Param(Param),
    /// The query itself.
    Query(Query),
}
impl Part {
    pub(crate) fn is_directive(&self) -> bool {
        matches!(self, Part::Directive(_))
    }
    pub(crate) fn is_param(&self) -> bool {
        matches!(self, Part::Param(_))
    }
}

/// the parsed AST
pub struct Ast {
    /// errors during parsing
    pub errors: Vec<AstError>,
    /// warnings during parsing
    pub warnings: Vec<AstWarning>,
    /// the parsed AST
    pub parts: Vec<Part>,
}

impl Ast {
    /// returns if the AST produced is valid (free of errors)
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}
/// AST parser.
pub struct Parser {
    root: SyntaxNode,
    errors: Vec<AstError>,
    warnings: Vec<AstWarning>,
    parts: Vec<Part>,
}

pub(crate) trait NonTrivalItem {
    fn span(&self) -> SourceSpan;
    fn token_string(&self) -> String;
}

impl NonTrivalItem for SyntaxNode {
    fn span(&self) -> SourceSpan {
        let s: usize = self.text_range().start().into();
        let e: usize = self.text_range().end().into();
        SourceSpan::new(s.into(), e - s)
    }

    fn token_string(&self) -> String {
        self.children_with_tokens()
            .filter_map(|n| {
                if n.kind().is_trivia() {
                    None
                } else {
                    Some(n.token_string())
                }
            })
            .collect::<String>()
    }
}
impl NonTrivalItem for NodeOrToken<SyntaxNode, SyntaxToken<Lang>> {
    fn span(&self) -> SourceSpan {
        let s: usize = self.text_range().start().into();
        let e: usize = self.text_range().end().into();
        SourceSpan::new(s.into(), e - s)
    }

    fn token_string(&self) -> String {
        match self {
            NodeOrToken::Node(node) => node.token_string(),
            NodeOrToken::Token(token) => token.to_string(),
        }
    }
}

trait Nontrivial {
    type Item: NonTrivalItem;

    fn n(&mut self) -> Option<Self::Item>;
}

impl Nontrivial for SyntaxNodeChildren<Lang> {
    type Item = SyntaxNode;
    fn n(&mut self) -> Option<Self::Item> {
        for node in self {
            if node.kind().is_trivia() {
                continue;
            }
            return Some(node);
        }
        None
    }
}

impl Nontrivial for SyntaxElementChildren<Lang> {
    type Item = NodeOrToken<SyntaxNode, SyntaxToken<Lang>>;

    fn n(&mut self) -> Option<Self::Item> {
        for node in self {
            if node.kind().is_trivia() {
                continue;
            }
            return Some(node);
        }
        None
    }
}

/// Regular expression or variable.
#[derive(Debug)]
pub struct Regex(regex::Regex);

impl From<Regex> for regex::Regex {
    fn from(regex: Regex) -> Self {
        regex.0
    }
}

impl std::fmt::Display for Regex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Regex {
    type Target = regex::Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Regex {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl PartialEq<&str> for Regex {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_str() == *other
    }
}

/// Identifier.
#[derive(Debug, Clone)]
pub struct Ident {
    node: SyntaxNode,
    name: String,
}

impl Ident {
    /// Returns the name of the identifier.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the node of the identifier.
    #[must_use]
    pub fn node(&self) -> &SyntaxNode {
        &self.node
    }
    /// Returns the span of the identifier.
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        self.node.span()
    }
    /// Converts the identifier to a string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.name
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.fmt(f)
    }
}

impl Deref for Ident {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.name
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl PartialEq<&str> for Ident {
    fn eq(&self, other: &&str) -> bool {
        self.name == **other
    }
}

/// Identifier.
#[derive(Debug, Clone)]
pub struct Variable {
    node: SyntaxNode,
    name: String,
}
impl Variable {
    /// Returns the span of the variable.
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        self.node.span()
    }
    /// Returns the name of the variable.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.fmt(f)
    }
}

impl Deref for Variable {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.name
    }
}

impl AsRef<str> for Variable {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl PartialEq<&str> for Variable {
    fn eq(&self, other: &&str) -> bool {
        self.name == **other
    }
}

/// A part of a string value.
#[derive(Debug)]
pub enum StringPart {
    /// A constant string value.
    Const(String),
    /// An expression that evaluates to a string value.
    Expr(SyntaxExpr),
}
/// A parsed expression with its syntax node.
#[derive(Debug)]
pub struct SyntaxExpr {
    /// The syntax node of the expression.
    pub node: SyntaxNode,
    /// The expression.
    pub expr: Expr,
}
/// A filter expression.
#[derive(Debug)]
pub enum Expr {
    /// A literal value.
    Ident(Ident),
    /// String
    String(Vec<StringPart>),
    /// A variable reference.
    Var(Variable),
    /// A constant value.
    Const(TagValue),
    /// An array value.
    Array(Vec<SyntaxExpr>),
}

/// A filter comparison rule.
#[derive(Debug)]
pub enum FilterCmp {
    /// A equality
    Eq {
        /// The left-hand side of the equality.
        lhs: Ident,
        /// The right-hand side of the equality.
        rhs: SyntaxExpr,
    },
    /// A not equality
    Neq {
        /// The left-hand side of the not equality.
        lhs: Ident,
        /// The right-hand side of the not equality.
        rhs: SyntaxExpr,
    },
    /// A regex equality
    EqRe {
        /// The left-hand side of the regex equality.
        lhs: Ident,
        /// The right-hand side of the regex equality.
        rhs: Regex,
    },
    /// A regex not equality
    NeqRe {
        /// The left-hand side of the regex not equality.
        lhs: Ident,
        /// The right-hand side of the regex not equality.
        rhs: Regex,
    },

    /// A less than comparison
    Lt {
        /// The left-hand side of the less than comparison.
        lhs: Ident,
        /// The right-hand side of the less than comparison.
        rhs: SyntaxExpr,
    },
    /// A greater than comparison
    Gt {
        /// The left-hand side of the greater than comparison.
        lhs: Ident,
        /// The right-hand side of the greater than comparison.
        rhs: SyntaxExpr,
    },
    /// A less than or equal comparison
    Lte {
        /// The left-hand side of the less than or equal comparison.
        lhs: Ident,
        /// The right-hand side of the less than or equal comparison.
        rhs: SyntaxExpr,
    },
    /// A greater than or equal comparison
    Gte {
        /// The left-hand side of the greater than or equal comparison.
        lhs: Ident,
        /// The right-hand side of the greater than or equal comparison.
        rhs: SyntaxExpr,
    },
    /// An in comparison
    In {
        /// The left-hand side of the in comparison.
        lhs: Ident,
        /// The right-hand side of the in comparison.
        rhs: SyntaxExpr,
    },
    /// An is comparison
    Is {
        /// The left-hand side of the is comparison.
        lhs: Ident,
        /// The right-hand side of the is comparison.
        rhs: TagType,
    },
}
/// A filter paren rule.
#[derive(Debug)]
pub enum FilterParen {
    /// A parsed paren rule.
    Paren(Box<FilterOr>),
    /// A parsed cmp rule.
    Cmp(FilterCmp),
}
/// A filter not rule.
#[derive(Debug)]
pub enum FilterNot {
    /// A parsed not rule.
    Not(FilterParen),
    /// A parsed and rule.
    Yes(FilterParen),
}
/// A filter and rule.
#[derive(Debug)]
pub struct FilterAnd(pub Vec<FilterNot>);
impl Deref for FilterAnd {
    type Target = [FilterNot];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
/// A filter or rule.
#[derive(Debug)]
pub struct FilterOr(pub Vec<FilterAnd>);

impl Deref for FilterOr {
    type Target = [FilterAnd];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A parsed duration.
#[derive(Debug)]
pub enum Duration {
    /// A parsed duration.
    Const(u64),
    /// A parsed duration variable.
    Var(Variable),
}

/// A parsed extend part.
#[derive(Debug)]
pub struct ExtendPart {
    /// The name of the extend part.
    pub name: Ident,
    /// The body of the extend part.
    pub value: SyntaxExpr,
}

/// A parsed rule.
#[derive(Debug)]
pub enum Rule {
    /// A parsed filter rule.
    Filter(FilterOr),
    /// A parsed sample rule.
    Sample(f64),
    /// A parsed map rule.
    Map(FunctionCall),
    /// A parsed align rule.
    Align {
        /// The duration to align by.
        duration: Option<Duration>,
        /// The function to align by.
        func: FunctionCall,
    },
    /// A parsed group rule.
    Group {
        /// The groups to bucket by.
        groups: Vec<Ident>,
        /// The function to bucket by.
        func: FunctionCall,
    },
    /// A parsed bucket rule.
    Bucket {
        /// The groups to bucket by.
        groups: Vec<Ident>,
        /// The duration to bucket by.
        duration: Option<Duration>,
        /// The function to bucket by.
        func: FunctionCall,
    },
    /// A parsed ifdef rule.
    IfDef {
        /// The variable to check.
        var: Variable,
        /// The branch to execute if the variable is defined.
        if_branch: Box<Rule>,
        /// The branch to execute if the variable is not defined.
        else_branch: Option<Box<Rule>>,
    },
    /// A parsed extern rule.
    Extern(Vec<ExtendPart>),
    /// A parsed as rule.
    As(Ident),
}

/// A rule with attatched syntax node
#[derive(Debug)]
pub struct SyntaxRule {
    /// the corresponding syntax node
    pub node: SyntaxNode,
    /// The actual rule
    pub rule: Rule,
}

impl Deref for SyntaxRule {
    type Target = Rule;

    fn deref(&self) -> &Self::Target {
        &self.rule
    }
}

/// A parsed simple query.
#[derive(Debug)]
pub struct SimpleQuery {
    /// the corresponding syntax node
    pub node: SyntaxNode,
    /// The dataset to compute on.
    pub dataset: IdentOrVariable,
    /// The metric to compute.
    pub metric: Ident,
    /// The alias to use for the metric.
    pub alias: Option<Ident>,
    /// The rules to apply.
    pub rules: Vec<SyntaxRule>,
}

/// A parsed compute query.
#[derive(Debug)]
pub struct ComputeQuery {
    /// the corresponding syntax node
    pub node: SyntaxNode,
    /// left query
    pub l: Query,
    /// right query
    pub r: Query,
    /// metric name of the resulting combined series
    pub name: Ident,
    /// Combination function
    pub func: FunctionCall,
    /// rules following the compute statement
    pub rules: Vec<SyntaxRule>,
}

/// A parsed query.
#[derive(Debug)]
pub enum Query {
    /// A parsed simple query.
    Simple(SimpleQuery),
    /// A parsed compute query.
    Compute(Box<ComputeQuery>),
}

/// A parsed identifier or variable.
#[derive(Debug)]
pub enum IdentOrVariable {
    /// A parsed identifier.
    Ident(Ident),
    /// A parsed variable.
    Var(Variable),
}

impl Parser {
    /// Creates a new parser with the given input.
    pub fn new(input: &str) -> Self {
        let SyntaxTree { root, errors } = syntax_tree::Parser::new(input).parse();
        Parser {
            root,
            errors: errors.into_iter().map(AstError::InvalidSyntax).collect(),
            warnings: Vec::new(),
            parts: Vec::new(),
        }
    }

    /// errors that occurred during parsing.
    #[must_use]
    pub fn errors(&self) -> &[AstError] {
        &self.errors
    }

    /// warnings that occurred during parsing.
    #[must_use]
    pub fn warnings(&self) -> &[AstWarning] {
        &self.warnings
    }

    /// parts of the query that were parsed successfully.
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }
}
impl Parser {
    fn n<T: Nontrivial>(
        &mut self,
        children: &mut T,
        node: &SyntaxNode,
        error_kind: SyntaxKind,
    ) -> Result<T::Item> {
        let Some(node) = children.n() else {
            self.errors.push(AstError::MissingToken {
                expected: error_kind,
                span: node.span(),
            });
            return Err(Error("expected node"));
        };
        Ok(node)
    }

    fn assert_type(&mut self, node: &SyntaxNode, expected: SyntaxKind) -> Result<()> {
        if node.kind() == expected {
            Ok(())
        } else {
            self.errors.push(AstError::UnexpectedSyntaxRuleOne {
                expected,
                found: node.kind(),
                span: node.span(),
            });
            Err(Error("wrong type"))
        }
    }

    fn assert_end(&mut self, mut children: impl Nontrivial) {
        let Some(node) = children.n() else {
            return;
        };
        self.errors
            .push(AstError::GarbageAtEndOfRule { span: node.span() });
    }
    fn ident_or_variable(&mut self, node: &SyntaxNode) -> Result<IdentOrVariable> {
        self.assert_type(node, SyntaxKind::IDENT_OR_VARIABLE)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::IDENT)?;
        let r = match c.kind() {
            SyntaxKind::IDENT => self.ident(c).map(IdentOrVariable::Ident),
            SyntaxKind::VARIABLE => self.variable(c).map(IdentOrVariable::Var),
            _ => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::IDENT, SyntaxKind::VARIABLE],
                    found: node.kind(),
                    span: node.span(),
                });
                Err(Error("wrong type"))
            }
        };
        self.assert_end(children);
        r
    }

    fn dataset(&mut self, node: &SyntaxNode) -> Result<IdentOrVariable> {
        self.ident_or_variable(node)
    }
    fn unescape_ident(&mut self, node: &impl NonTrivalItem, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut escaped = false;
        let span = node.span();
        for c in s.chars() {
            if escaped {
                match c {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0c'),
                    '\\' => out.push('\\'),
                    '`' => out.push('`'),
                    'u' => {
                        self.errors.push(AstError::UnicodeEscape { span });
                        out.push('u');
                    }
                    char => {
                        self.warnings
                            .push(AstWarning::UnknownEscapeSequence { char, span });
                        out.push(c);
                    }
                }
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else {
                out.push(c);
            }
        }
        out
    }
    fn unescape_string(&mut self, node: &impl NonTrivalItem, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut escaped = false;
        let span = node.span();
        for c in s.chars() {
            if escaped {
                match c {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0c'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '$' => out.push('$'),
                    'u' => {
                        self.errors.push(AstError::UnicodeEscape { span });
                        out.push('u');
                    }
                    char => {
                        self.warnings
                            .push(AstWarning::UnknownEscapeSequence { char, span });
                        out.push(c);
                    }
                }
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else {
                out.push(c);
            }
        }
        out
    }

    /// This is just `to_string` with a nice name, the reason  is rusts regex
    /// engine already take scare of un-escaping
    fn unescape_regex<'s>(&self, _node: &impl NonTrivalItem, s: &'s str) -> &'s str {
        let _ = self;
        s
    }

    fn string_expr(&mut self, node: SyntaxNode) -> Result<SyntaxExpr> {
        self.assert_type(&node, SyntaxKind::STRING)?;
        let mut children = node.children_with_tokens();
        let c = self.n(&mut children, &node, SyntaxKind::LX_STRING)?;
        match c.kind() {
            SyntaxKind::LX_STRING => {
                let s = self.string_const(&node)?;
                Ok(SyntaxExpr {
                    node,
                    expr: Expr::Const(s),
                })
            }
            SyntaxKind::LX_STRING_START => {
                let s = c.token_string();
                let s = if let Some(s) = s.strip_prefix('"').and_then(|s| s.strip_suffix("${")) {
                    s
                } else {
                    self.errors.push(AstError::InvalidString { span: c.span() });
                    "__INVALID__"
                };
                let mut parts = vec![StringPart::Const(self.unescape_string(&c, s))];
                while let Some(c) = children.n() {
                    match c.kind() {
                        SyntaxKind::LX_STRING_SEGMENT => {
                            let s = c.token_string();
                            let Some(s) = s.strip_prefix('}').and_then(|s| s.strip_suffix("${"))
                            else {
                                self.errors.push(AstError::InvalidString { span: c.span() });
                                continue;
                            };
                            parts.push(StringPart::Const(self.unescape_string(&c, s)));
                        }
                        SyntaxKind::LX_STRING_END => {
                            let s = c.token_string();
                            let Some(s) = s.strip_prefix('}').and_then(|s| s.strip_suffix('"'))
                            else {
                                self.errors.push(AstError::InvalidString { span: c.span() });
                                continue;
                            };
                            parts.push(StringPart::Const(self.unescape_string(&c, s)));
                            break;
                        }
                        SyntaxKind::EXPR => {
                            let Some(n) = c.into_node() else { continue };
                            if let Ok(e) = self.expr(n) {
                                parts.push(StringPart::Expr(e));
                            }
                        }
                        found => {
                            self.errors.push(AstError::UnexpectedSyntaxRule {
                                expected: &[SyntaxKind::LX_STRING, SyntaxKind::LX_STRING_START],
                                found,
                                span: node.span(),
                            });
                            return Err(Error("garbled string interpolation"));
                        }
                    }
                }
                self.assert_end(children);
                Ok(SyntaxExpr {
                    node,
                    expr: Expr::String(parts),
                })
            }
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_STRING, SyntaxKind::LX_STRING_START],
                    found,
                    span: node.span(),
                });
                Err(Error("unexpected not a string"))
            }
        }
    }

    fn array_expr(&mut self, node: SyntaxNode) -> Result<SyntaxExpr> {
        self.assert_type(&node, SyntaxKind::ARRAY)?;
        let mut elements = Vec::new();
        let mut children = node.children();
        while let Some(c) = children.n() {
            if let Ok(e) = self.expr(c) {
                elements.push(e);
            }
        }
        self.assert_end(children);
        Ok(SyntaxExpr {
            node,
            expr: Expr::Array(elements),
        })
    }

    fn expr_value(&mut self, node: SyntaxNode) -> Result<SyntaxExpr> {
        self.assert_type(&node, SyntaxKind::CONST)?;
        let mut children = node.children();
        let c = self.n(&mut children, &node, SyntaxKind::CONST)?;

        let r = match c.kind() {
            SyntaxKind::INTEGER => SyntaxExpr {
                node,
                expr: Expr::Const(self.integer_const(&c)?),
            },
            SyntaxKind::FLOAT => SyntaxExpr {
                node,
                expr: Expr::Const(self.float_const(&c)?),
            },
            SyntaxKind::BOOL => SyntaxExpr {
                node,
                expr: Expr::Const(self.bool_const(&c)?),
            },
            SyntaxKind::NULL => SyntaxExpr {
                node,
                expr: Expr::Const(self.null_const(&c)?),
            },
            SyntaxKind::STRING => self.string_expr(c)?,
            SyntaxKind::ARRAY => self.array_expr(c)?,
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[
                        SyntaxKind::INTEGER,
                        SyntaxKind::FLOAT,
                        SyntaxKind::STRING,
                        SyntaxKind::BOOL,
                        SyntaxKind::ARRAY,
                        SyntaxKind::NULL,
                    ],
                    found,
                    span: c.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        self.assert_end(children);
        Ok(r)
    }

    fn expr(&mut self, node: SyntaxNode) -> Result<SyntaxExpr> {
        self.assert_type(&node, SyntaxKind::EXPR)?;
        let mut children = node.children();
        let n = self.n(&mut children, &node, SyntaxKind::EXPR)?;
        let expr = match n.kind() {
            SyntaxKind::CONST => self.expr_value(n)?,
            SyntaxKind::IDENT => SyntaxExpr {
                node,
                expr: Expr::Ident(self.ident(n)?),
            },
            SyntaxKind::VARIABLE => SyntaxExpr {
                node,
                expr: Expr::Var(self.variable(n)?),
            },
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::CONST, SyntaxKind::IDENT, SyntaxKind::VARIABLE],
                    found,
                    span: n.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        Ok(expr)
    }

    fn regex(&mut self, node: &SyntaxNode) -> Result<Regex> {
        self.assert_type(node, SyntaxKind::REGEX)?;
        let s = node.token_string();
        let Some(r) = s.strip_prefix("#/").and_then(|s| s.strip_suffix('/')) else {
            self.errors.push(AstError::InvalidRegex {
                span: node.span(),
                message: "invalid boundaries".to_string(),
            });
            return Err(Error("invalid regex boundaries"));
        };
        let r = regex::Regex::new(self.unescape_regex(node, r));
        match r {
            Ok(r) => Ok(Regex(r)),
            Err(e) => {
                self.errors.push(AstError::InvalidRegex {
                    span: node.span(),
                    message: e.to_string(),
                });
                Err(Error("invalid regex"))
            }
        }
    }

    fn filter_cmp(&mut self, node: &SyntaxNode) -> Result<FilterCmp> {
        self.assert_type(node, SyntaxKind::FILTER_CMP)?;
        let mut children = node.children();
        let lhs = self
            .n(&mut children, node, SyntaxKind::IDENT)
            .and_then(|n| self.ident(n));
        let c = self.n(&mut children, node, SyntaxKind::FILTER_CMP)?;
        let r = match c.kind() {
            SyntaxKind::FILTER_CMP_EQ => {
                let mut children = c.children();
                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                if c.kind() == SyntaxKind::REGEX {
                    let rhs = self.regex(&c)?;
                    Ok(FilterCmp::EqRe { lhs: lhs?, rhs })
                } else {
                    let rhs = self.expr(c)?;
                    Ok(FilterCmp::Eq { lhs: lhs?, rhs })
                }
            }
            SyntaxKind::FILTER_CMP_NEQ => {
                let mut children = c.children();
                let c = self.n(&mut children, &c, SyntaxKind::EXPR)?;
                if c.kind() == SyntaxKind::REGEX {
                    let rhs = self.regex(&c)?;
                    Ok(FilterCmp::NeqRe { lhs: lhs?, rhs })
                } else {
                    let rhs = self.expr(c)?;
                    Ok(FilterCmp::Neq { lhs: lhs?, rhs })
                }
            }
            SyntaxKind::FILTER_CMP_LT => {
                let mut children = c.children();
                let c = self.n(&mut children, &c, SyntaxKind::EXPR)?;
                let rhs = self.expr(c)?;
                Ok(FilterCmp::Lt { lhs: lhs?, rhs })
            }
            SyntaxKind::FILTER_CMP_GT => {
                let mut children = c.children();
                let c = self.n(&mut children, &c, SyntaxKind::EXPR)?;
                let rhs = self.expr(c)?;
                Ok(FilterCmp::Gt { lhs: lhs?, rhs })
            }
            SyntaxKind::FILTER_CMP_LTE => {
                let mut children = c.children();
                let c = self.n(&mut children, &c, SyntaxKind::EXPR)?;
                let rhs = self.expr(c)?;
                Ok(FilterCmp::Lte { lhs: lhs?, rhs })
            }
            SyntaxKind::FILTER_CMP_GTE => {
                let mut children = c.children();
                let c = self.n(&mut children, &c, SyntaxKind::EXPR)?;
                let rhs = self.expr(c)?;
                Ok(FilterCmp::Gte { lhs: lhs?, rhs })
            }
            SyntaxKind::FILTER_CMP_IN => {
                let mut children = c.children();
                let c = self.n(&mut children, &c, SyntaxKind::EXPR)?;
                if c.kind() == SyntaxKind::VARIABLE {
                    Ok(FilterCmp::In {
                        lhs: lhs?,
                        rhs: SyntaxExpr {
                            node: c.clone(),
                            expr: Expr::Var(self.variable(c)?),
                        },
                    })
                } else {
                    let rhs = self.expr(c)?;
                    Ok(FilterCmp::In { lhs: lhs?, rhs })
                }
            }
            SyntaxKind::FILTER_CMP_IS => {
                let mut children = c.children();
                let c = self.n(&mut children, &c, SyntaxKind::OTEL_TYPE)?;
                let rhs = self.otel_typ(&c)?;
                Ok(FilterCmp::Is { lhs: lhs?, rhs })
            }
            _ => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[
                        SyntaxKind::FILTER_CMP_EQ,
                        SyntaxKind::FILTER_CMP_NEQ,
                        SyntaxKind::FILTER_CMP_LT,
                        SyntaxKind::FILTER_CMP_GT,
                        SyntaxKind::FILTER_CMP_LTE,
                        SyntaxKind::FILTER_CMP_GTE,
                    ],
                    found: c.kind(),
                    span: c.span(),
                });
                Err(Error("wrong type"))
            }
        };
        self.assert_end(children);
        r
    }

    fn filter_paren(&mut self, node: &SyntaxNode) -> Result<FilterParen> {
        self.assert_type(node, SyntaxKind::FILTER_PAREN)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::FILTER_PAREN)?;
        if c.kind() == SyntaxKind::FILTER_OR {
            Ok(FilterParen::Paren(Box::new(self.filter_or(&c)?)))
        } else {
            Ok(FilterParen::Cmp(self.filter_cmp(&c)?))
        }
    }

    fn filter_not(&mut self, node: &SyntaxNode) -> Result<FilterNot> {
        self.assert_type(node, SyntaxKind::FILTER_NOT)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::FILTER_PAREN)?;
        if c.kind() == SyntaxKind::KEYWORD {
            let c = self.n(&mut children, node, SyntaxKind::FILTER_PAREN)?;
            Ok(FilterNot::Not(self.filter_paren(&c)?))
        } else {
            Ok(FilterNot::Yes(self.filter_paren(&c)?))
        }
    }

    fn filter_and(&mut self, node: &SyntaxNode) -> Result<FilterAnd> {
        self.assert_type(node, SyntaxKind::FILTER_AND)?;
        let mut children = node.children();
        let mut filters = Vec::new();
        while let Some(n) = children.n() {
            if let Ok(f) = self.filter_not(&n) {
                filters.push(f);
            }
        }
        self.assert_end(children);
        if filters.is_empty() {
            self.errors
                .push(AstError::EmptyFilter { span: node.span() });
            Err(Error("empty filter"))
        } else {
            Ok(FilterAnd(filters))
        }
    }

    fn filter_or(&mut self, node: &SyntaxNode) -> Result<FilterOr> {
        self.assert_type(node, SyntaxKind::FILTER_OR)?;
        let mut children = node.children();
        let mut filters = Vec::new();
        while let Some(n) = children.n() {
            if let Ok(f) = self.filter_and(&n) {
                filters.push(f);
            }
        }
        self.assert_end(children);
        if filters.is_empty() {
            self.errors
                .push(AstError::EmptyFilter { span: node.span() });
            Err(Error("empty filter"))
        } else {
            Ok(FilterOr(filters))
        }
    }

    fn rule_filter(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::FILTER)?;
        let mut children = node.children();
        let n = self.n(&mut children, node, SyntaxKind::FILTER_OR)?;
        let f = self.filter_or(&n)?;
        self.assert_end(children);
        Ok(Rule::Filter(f))
    }
    fn rule_sample(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::SAMPLE)?;
        let mut children = node.children();
        let n = self.n(&mut children, node, SyntaxKind::FLOAT)?;
        let TagValue::Float(f) = self.float_const(&n)? else {
            return Err(Error("expected float"));
        };
        self.assert_end(children);
        Ok(Rule::Sample(f))
    }

    fn function_path(&mut self, node: SyntaxNode) -> Result<Vec<Ident>> {
        match node.kind() {
            SyntaxKind::FUNCTION_PATH => {
                let mut children = node.children();
                let mut path = Vec::new();
                while let Some(n) = children.n() {
                    if let Ok(p) = self.ident(n) {
                        path.push(p);
                    }
                }
                Ok(path)
            }
            SyntaxKind::MATH_FN => {
                let name = node.token_string();
                Ok(vec![
                    Ident {
                        node: node.clone(),
                        name: "__MATH__".to_string(),
                    },
                    Ident { node, name },
                ])
            }
            _ => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::FUNCTION_PATH, SyntaxKind::MATH_FN],
                    found: node.kind(),
                    span: node.span(),
                });
                Err(Error("expected function path"))
            }
        }
    }
    fn rule_map(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::MAP)?;
        let mut children = node.children();
        let n = self.n(&mut children, node, SyntaxKind::MAP)?;
        let f = match n.kind() {
            SyntaxKind::MAP_MUL => {
                let mut children = n.children();
                let e = self.n(&mut children, &n, SyntaxKind::MAP_MUL)?;
                let expr = self.expr(e)?;
                FunctionCall {
                    node: n.clone(),
                    name: vec![Ident {
                        node: n,
                        name: "*".to_string(),
                    }],
                    args: vec![expr],
                }
            }
            SyntaxKind::MAP_DIV => {
                let mut children = n.children();
                let e = self.n(&mut children, &n, SyntaxKind::MAP_DIV)?;
                let expr = self.expr(e)?;
                FunctionCall {
                    node: n.clone(),
                    name: vec![Ident {
                        node: n,
                        name: "/".to_string(),
                    }],
                    args: vec![expr],
                }
            }
            SyntaxKind::MAP_PLUS => {
                let mut children = n.children();
                let e = self.n(&mut children, &n, SyntaxKind::MAP_PLUS)?;
                let expr = self.expr(e)?;
                FunctionCall {
                    node: n.clone(),
                    name: vec![Ident {
                        node: n,
                        name: "+".to_string(),
                    }],
                    args: vec![expr],
                }
            }
            SyntaxKind::MAP_MINUS => {
                let mut children = n.children();
                let e = self.n(&mut children, &n, SyntaxKind::MAP_MINUS)?;
                let expr = self.expr(e)?;
                FunctionCall {
                    node: n.clone(),
                    name: vec![Ident {
                        node: n,
                        name: "-".to_string(),
                    }],
                    args: vec![expr],
                }
            }

            SyntaxKind::FUNCTION_CALL => self.function_call(n)?,
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[
                        SyntaxKind::MAP_PLUS,
                        SyntaxKind::MAP_MINUS,
                        SyntaxKind::MAP_MUL,
                        SyntaxKind::MAP_DIV,
                        SyntaxKind::FUNCTION_PATH,
                    ],
                    found,
                    span: n.span(),
                });
                return Err(Error("expected LX_MUL"));
            }
        };
        Ok(Rule::Map(f))
    }

    fn duration(&mut self, node: SyntaxNode) -> Result<Duration> {
        if node.kind() == SyntaxKind::VARIABLE {
            let v = self.variable(node)?;
            return Ok(Duration::Var(v));
        }
        self.assert_type(&node, SyntaxKind::DURATION)?;
        let mut children = node.children();
        let n = self.n(&mut children, &node, SyntaxKind::INTEGER)?;
        let TagValue::Int(i) = self.integer_const(&n)? else {
            return Err(Error("expected integer (this should be unreachable!)"));
        };
        let i = if let Ok(i) = u64::try_from(i) {
            i
        } else {
            self.errors
                .push(AstError::NegativeDuration { span: n.span() });
            0
        };
        let n = self.n(&mut children, &node, SyntaxKind::TIME_UNIT)?;
        let unit = self.time_unit(&n)?;
        let duration = match unit.as_str() {
            "ms" if i < 1000 => {
                self.errors.push(AstError::TimeTooSmall {
                    time: i,
                    span: n.span(),
                });
                1
            }
            "ms" if !i.is_multiple_of(1000) => {
                self.warnings.push(AstWarning::TimeNotSecondAligned {
                    time: i,
                    span: n.span(),
                });
                i / 1000
            }
            "ms" => i / 1000,
            "s" => i,
            "m" => i * 60,
            "h" => i * 60 * 60,
            "d" => i * 60 * 60 * 24,
            "w" => i * 60 * 60 * 24 * 7,
            "M" => i * 60 * 60 * 24 * 30,
            "y" => i * 60 * 60 * 24 * 365,
            _ => {
                self.errors
                    .push(AstError::InvalidTimeUnit { span: n.span() });

                return Err(Error("invalid time unit"));
            }
        };
        Ok(Duration::Const(duration))
    }

    fn tags(&mut self, node: &SyntaxNode) -> Result<Vec<Ident>> {
        self.assert_type(node, SyntaxKind::TAG_LIST)?;
        let mut children = node.children();
        let mut tags = Vec::new();
        while let Some(n) = children.n() {
            if let Ok(tag) = self.ident(n) {
                tags.push(tag);
            }
        }
        Ok(tags)
    }

    fn rule_align(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::ALIGN)?;
        let mut children = node.children();
        let mut n = self.n(&mut children, node, SyntaxKind::KEYWORD)?;
        let mut duration = Ok(None);
        let mut found = self.kw(n.clone())?;
        if found == "to" {
            duration = self
                .n(&mut children, node, SyntaxKind::DURATION)
                .and_then(|n| Ok(Some(self.duration(n)?)));
            n = self.n(&mut children, node, SyntaxKind::KEYWORD)?;
            found = self.kw(n.clone())?;
        }
        if found == "using" {
            let n = self.n(&mut children, node, SyntaxKind::FUNCTION_CALL)?;
            let func = self.function_call(n)?;
            self.assert_end(children);
            Ok(Rule::Align {
                duration: duration?,
                func,
            })
        } else {
            self.errors.push(AstError::UnexpectedKeyword {
                expected: "using",
                found: found.into_string(),
                span: n.span(),
            });
            self.assert_end(children);
            Err(Error("expected 'using'"))
        }
    }
    fn rule_group(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::GROUP)?;
        let mut children = node.children();
        let mut n = self.n(&mut children, node, SyntaxKind::KEYWORD)?;
        let mut groups = Ok(Vec::new());
        let mut found = self.kw(n.clone())?;
        if found == "by" {
            groups = self
                .n(&mut children, node, SyntaxKind::TAG_LIST)
                .and_then(|n| self.tags(&n));
            n = self.n(&mut children, node, SyntaxKind::KEYWORD)?;
            found = self.kw(n.clone())?;
        }
        if found == "using" {
            let n = self.n(&mut children, node, SyntaxKind::FUNCTION_CALL)?;
            let func = self.function_call(n)?;
            self.assert_end(children);
            Ok(Rule::Group {
                groups: groups?,
                func,
            })
        } else {
            self.errors.push(AstError::UnexpectedKeyword {
                expected: "using",
                found: found.into_string(),
                span: n.span(),
            });
            self.assert_end(children);
            Err(Error("expected 'using'"))
        }
    }
    fn function_args(&mut self, node: &SyntaxNode) -> Result<Vec<SyntaxExpr>> {
        self.assert_type(node, SyntaxKind::FUNCTION_ARGS)?;
        let mut res = Vec::new();
        let mut children = node.children();
        while let Some(c) = children.n() {
            if let Ok(arg) = self.expr(c) {
                res.push(arg);
            }
        }
        self.assert_end(children);
        Ok(res)
    }
    fn function_call(&mut self, node: SyntaxNode) -> Result<FunctionCall> {
        self.assert_type(&node, SyntaxKind::FUNCTION_CALL)?;
        let mut children = node.children();
        let name = self
            .n(&mut children, &node, SyntaxKind::FUNCTION_PATH)
            .and_then(|node| self.function_path(node));
        let n = self.n(&mut children, &node, SyntaxKind::FUNCTION_PATH)?;
        let args = self.function_args(&n)?;
        self.assert_end(children);
        Ok(FunctionCall {
            node,
            name: name?,
            args,
        })
    }
    fn rule_bucket(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::BUCKET)?;
        let mut children = node.children();
        let mut n = self.n(&mut children, node, SyntaxKind::KEYWORD)?;
        let mut groups = Ok(Vec::new());
        let mut duration = Ok(None);
        let mut found = self.kw(n.clone())?;
        if found == "by" {
            groups = self
                .n(&mut children, node, SyntaxKind::TAG_LIST)
                .and_then(|n| self.tags(&n));
            n = self.n(&mut children, node, SyntaxKind::KEYWORD)?;
            found = self.kw(n.clone())?;
        }
        if found == "to" {
            duration = self
                .n(&mut children, node, SyntaxKind::DURATION)
                .and_then(|n| Ok(Some(self.duration(n)?)));
            n = self.n(&mut children, node, SyntaxKind::KEYWORD)?;
            found = self.kw(n.clone())?;
        }
        if found == "using" {
            let call = self.n(&mut children, node, SyntaxKind::FUNCTION_CALL)?;
            let func = self.function_call(call)?;
            self.assert_end(children);
            Ok(Rule::Bucket {
                groups: groups?,
                duration: duration?,
                func,
            })
        } else {
            self.errors.push(AstError::UnexpectedKeyword {
                expected: "using",
                found: found.into_string(),
                span: n.span(),
            });
            Err(Error("expected 'using'"))
        }
    }

    fn rule_ifdef(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::IFDEF)?;
        let mut children = node.children();
        let var = self
            .n(&mut children, node, SyntaxKind::VARIABLE)
            .and_then(|n| self.variable(n));
        let if_branch = self
            .n(&mut children, node, SyntaxKind::FILTER)
            .and_then(|n| Ok(Box::new(self.rule_filter(&n)?)));
        let else_branch = if let Some(n) = children.n() {
            self.rule_filter(&n).map(Box::new).map(Some)
        } else {
            Ok(None)
        };
        self.assert_end(children);
        Ok(Rule::IfDef {
            var: var?,
            if_branch: if_branch?,
            else_branch: else_branch?,
        })
    }

    fn rule_as(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::AS)?;
        let mut children = node.children();
        let n = self.n(&mut children, node, SyntaxKind::IDENT)?;
        let name = self.ident(n)?;
        self.assert_end(children);
        Ok(Rule::As(name))
    }

    fn extend_part(&mut self, node: &SyntaxNode) -> Result<ExtendPart> {
        self.assert_type(node, SyntaxKind::EXTEND_PART)?;
        let mut children = node.children();
        let name = self
            .n(&mut children, node, SyntaxKind::IDENT)
            .and_then(|n| self.ident(n));
        let value = self
            .n(&mut children, node, SyntaxKind::EXPR)
            .and_then(|n| self.expr(n));
        self.assert_end(children);
        Ok(ExtendPart {
            name: name?,
            value: value?,
        })
    }

    fn rule_extend(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::EXTEND)?;
        let mut children = node.children();
        let mut parts = Vec::new();
        while let Some(p) = children.n() {
            if let Ok(part) = self.extend_part(&p) {
                parts.push(part);
            }
        }
        self.assert_end(children);
        Ok(Rule::Extern(parts))
    }

    fn rule(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::RULE)?;
        let mut children = node.children();
        let r = self.n(&mut children, node, SyntaxKind::RULE)?;

        match r.kind() {
            SyntaxKind::FILTER => self.rule_filter(&r),
            SyntaxKind::SAMPLE => self.rule_sample(&r),
            SyntaxKind::MAP => self.rule_map(&r),
            SyntaxKind::ALIGN => self.rule_align(&r),
            SyntaxKind::GROUP => self.rule_group(&r),
            SyntaxKind::BUCKET => self.rule_bucket(&r),
            SyntaxKind::IFDEF => self.rule_ifdef(&r),
            SyntaxKind::AS => self.rule_as(&r),
            SyntaxKind::EXTEND => self.rule_extend(&r),
            kind => {
                self.errors.push(AstError::UnknownRule {
                    kind,
                    span: r.span(),
                });
                Err(Error("unknown rule"))
            }
        }
    }

    fn simple_query(&mut self, node: SyntaxNode) -> Result<SimpleQuery> {
        self.assert_type(&node, SyntaxKind::SIMPLE_QUERY)?;
        let mut children = node.children();

        let dataset = self
            .n(&mut children, &node, SyntaxKind::IDENT)
            .and_then(|c| self.dataset(&c));
        let metric = self
            .n(&mut children, &node, SyntaxKind::IDENT)
            .and_then(|c| self.ident(c));

        let Some(mut c) = children.n() else {
            return Ok(SimpleQuery {
                node,
                dataset: dataset?,
                metric: metric?,
                alias: None,
                rules: Vec::new(),
            });
        };
        let mut alias = Ok(None);
        if c.kind() == SyntaxKind::KEYWORD {
            alias = self
                .n(&mut children, &node, SyntaxKind::IDENT)
                .and_then(|c| Ok(Some(self.ident(c)?)));
            let Some(n) = children.n() else {
                return Ok(SimpleQuery {
                    node,
                    dataset: dataset?,
                    metric: metric?,
                    alias: alias?,
                    rules: Vec::new(),
                });
            };
            c = n;
        }
        let mut rules = if let Ok(rule) = self.rule(&c) {
            vec![SyntaxRule { rule, node: c }]
        } else {
            Vec::new()
        };
        while let Some(node) = children.n() {
            if let Ok(rule) = self.rule(&node) {
                rules.push(SyntaxRule { node, rule });
            }
        }

        Ok(SimpleQuery {
            node,
            dataset: dataset?,
            metric: metric?,
            alias: alias?,
            rules,
        })
    }

    fn compute_query(&mut self, node: SyntaxNode) -> Result<ComputeQuery> {
        self.assert_type(&node, SyntaxKind::COMPUTE_QUERY)?;
        let mut children = node.children();

        let l = self
            .n(&mut children, &node, SyntaxKind::SIMPLE_QUERY)
            .and_then(|c| self.query(&c));
        let r = self
            .n(&mut children, &node, SyntaxKind::SIMPLE_QUERY)
            .and_then(|c| self.query(&c));
        let name = self
            .n(&mut children, &node, SyntaxKind::IDENT)
            .and_then(|c| self.ident(c));
        let Some(c) = children.n() else {
            self.errors.push(AstError::MissingToken {
                expected: SyntaxKind::FUNCTION_CALL,
                span: node.span(),
            });
            return Err(Error("expected compute function"));
        };

        let func = self.function_call(c);
        let mut rules = vec![];
        while let Some(c) = children.n() {
            if let Ok(rule) = self.rule(&c) {
                rules.push(SyntaxRule { node: c, rule });
            }
        }

        Ok(ComputeQuery {
            node,
            l: l?,
            r: r?,
            name: name?,
            rules,
            func: func?,
        })
    }
    fn query(&mut self, node: &SyntaxNode) -> Result<Query> {
        self.assert_type(node, SyntaxKind::QUERY)?;
        let mut children = node.children();

        let c = self.n(&mut children, node, SyntaxKind::SIMPLE_QUERY)?;

        let r = match c.kind() {
            SyntaxKind::SIMPLE_QUERY => Query::Simple(self.simple_query(c)?),
            SyntaxKind::COMPUTE_QUERY => Query::Compute(Box::new(self.compute_query(c)?)),
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::SIMPLE_QUERY, SyntaxKind::COMPUTE_QUERY],
                    found,
                    span: c.span(),
                });
                return Err(Error("missing token"));
            }
        };

        self.assert_end(children);
        Ok(r)
    }

    fn ident_body(&mut self, node: SyntaxNode) -> Result<Ident> {
        let mut children = node.children_with_tokens();
        let n = self.n(&mut children, &node, SyntaxKind::LX_IDENT)?;
        let name = match n.kind() {
            SyntaxKind::LX_IDENT => n.token_string(),
            SyntaxKind::LX_ESCAPED_IDENT => {
                let s = n.to_string();
                let Some(s) = s.strip_prefix('`').and_then(|s| s.strip_suffix('`')) else {
                    self.errors.push(AstError::InvalidIdent { span: n.span() });
                    return Err(Error("string start must be followed by a string segment"));
                };

                self.unescape_ident(&n, s)
            }
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_IDENT, SyntaxKind::LX_ESCAPED_IDENT],
                    found,
                    span: n.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        self.assert_end(children);
        Ok(Ident { node, name })
    }

    fn ident(&mut self, node: SyntaxNode) -> Result<Ident> {
        self.assert_type(&node, SyntaxKind::IDENT)?;
        self.ident_body(node)
    }

    fn variable(&mut self, node: SyntaxNode) -> Result<Variable> {
        self.assert_type(&node, SyntaxKind::VARIABLE)?;
        let mut children = node.children_with_tokens();
        let n = self.n(&mut children, &node, SyntaxKind::LX_VARIABLE)?;
        let name = match n.kind() {
            SyntaxKind::LX_VARIABLE => {
                let s = n.token_string();
                let Some(s) = s.strip_prefix('$') else {
                    self.errors
                        .push(AstError::InvalidVariable { span: n.span() });
                    return Err(Error("string start must be followed by a string segment"));
                };

                s.to_string()
            }
            SyntaxKind::LX_ESCAPED_VARIABLE => {
                let s = n.token_string();
                let Some(s) = s.strip_prefix("$`").and_then(|s| s.strip_suffix('`')) else {
                    self.errors
                        .push(AstError::InvalidVariable { span: n.span() });
                    return Err(Error("string start must be followed by a string segment"));
                };
                self.unescape_ident(&n, s)
            }
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_VARIABLE, SyntaxKind::LX_ESCAPED_VARIABLE],
                    found,
                    span: n.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        self.assert_end(children);
        Ok(Variable { node, name })
    }

    fn kw(&mut self, node: SyntaxNode) -> Result<Ident> {
        self.assert_type(&node, SyntaxKind::KEYWORD)?;
        self.ident_body(node)
    }

    fn bool_const(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::BOOL)?;
        let value = self.token_of_type(node, SyntaxKind::LX_BOOL)?;
        match value.as_str() {
            "true" => Ok(TagValue::Bool(true)),
            "false" => Ok(TagValue::Bool(false)),
            _ => {
                self.errors
                    .push(AstError::InvalidBoolConstant { span: node.span() });
                Err(Error("unexpected syntax"))
            }
        }
    }

    fn null_const(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::NULL)?;
        Ok(TagValue::Null)
    }

    fn token_of_type(&mut self, node: &SyntaxNode, kind: SyntaxKind) -> Result<String> {
        let mut children = node.children_with_tokens();
        let c = self.n(&mut children, node, kind)?;
        let r = match c.kind() {
            k if k == kind => Ok(c.token_string()),
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRuleOne {
                    expected: kind,
                    found,
                    span: c.span(),
                });
                Err(Error("token of wrong type"))
            }
        };
        self.assert_end(children);
        r
    }

    fn integer_const(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::INTEGER)?;
        let value = self.token_of_type(node, SyntaxKind::LX_INTEGER)?;
        if let Ok(value) = value.parse::<i64>() {
            Ok(TagValue::Int(value))
        } else {
            self.errors
                .push(AstError::InvalidIntegerConstant { span: node.span() });
            Err(Error("invalid integer"))
        }
    }

    fn time_unit(&mut self, node: &SyntaxNode) -> Result<String> {
        self.assert_type(node, SyntaxKind::TIME_UNIT)?;
        let value = self.token_of_type(node, SyntaxKind::LX_IDENT)?;
        Ok(value)
    }

    fn float_const(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::FLOAT)?;
        let mut children = node.children_with_tokens();
        let c = self.n(&mut children, node, SyntaxKind::FLOAT)?;
        let r = match c.kind() {
            SyntaxKind::LX_FLOAT => {
                if let Ok(value) = c.token_string().parse::<f64>() {
                    Ok(TagValue::Float(value))
                } else {
                    self.errors
                        .push(AstError::InvalidFloatConstant { span: node.span() });
                    Err(Error("invalid integer"))
                }
            }
            SyntaxKind::LX_INF => Ok(TagValue::Float(f64::INFINITY)),
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_FLOAT, SyntaxKind::LX_INF],
                    found,
                    span: c.span(),
                });
                Err(Error("token of wrong type"))
            }
        };
        self.assert_end(children);
        r
    }

    fn string_const(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::STRING)?;
        let s = node.token_string();
        let Some(s) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
            self.errors
                .push(AstError::InvalidString { span: node.span() });
            return Err(Error("string start must be followed by a string segment"));
        };

        let s = self.unescape_string(node, s);
        match s.try_into() {
            Ok(s) => Ok(TagValue::String(s)),
            Err(e) => {
                self.errors.push(AstError::FailedToCreateString {
                    error: e,
                    span: node.span(),
                });
                Err(Error("failed to create string"))
            }
        }
    }

    fn const_expr(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::EXPR)?;
        let mut children = node.children();
        let n = self.n(&mut children, node, SyntaxKind::EXPR)?;
        let r = if n.kind() == SyntaxKind::CONST {
            self.constant(&n)
        } else {
            self.errors.push(AstError::ExpectedConst { span: n.span() });
            Err(Error("expected const"))
        };
        self.assert_end(children);
        r
    }

    fn array_const(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::ARRAY)?;
        let mut children = node.children();
        let mut values = Vec::new();
        while let Some(child) = children.n() {
            values.push(self.const_expr(&child)?);
        }
        self.assert_end(children);
        Ok(TagValue::Array(values))
    }

    fn constant(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::CONST)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::CONST)?;

        let r = match c.kind() {
            SyntaxKind::INTEGER => self.integer_const(&c),
            SyntaxKind::FLOAT => self.float_const(&c),
            SyntaxKind::STRING => self.string_const(&c),
            SyntaxKind::BOOL => self.bool_const(&c),
            SyntaxKind::ARRAY => self.array_const(&c),
            SyntaxKind::NULL => self.null_const(&c),
            found => {
                self.errors.push(AstError::UnexpectedSyntaxRule {
                    expected: &[
                        SyntaxKind::INTEGER,
                        SyntaxKind::FLOAT,
                        SyntaxKind::STRING,
                        SyntaxKind::BOOL,
                        SyntaxKind::ARRAY,
                        SyntaxKind::NULL,
                    ],
                    found,
                    span: c.span(),
                });
                Err(Error("unexpected syntax"))
            }
        };
        self.assert_end(children);
        r
    }

    fn require_ident(
        &mut self,
        node: &SyntaxNode,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<Ident> {
        let c = self.n(children, node, SyntaxKind::IDENT)?;
        self.ident(c)
    }

    fn require_variable(
        &mut self,
        node: &SyntaxNode,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<Variable> {
        let c = self.n(children, node, SyntaxKind::VARIABLE)?;
        self.variable(c)
    }

    fn otel_typ(&mut self, node: &SyntaxNode) -> Result<TagType> {
        self.assert_type(node, SyntaxKind::OTEL_TYPE)?;
        let t = node.token_string();
        match t.as_str() {
            "int" => Ok(TagType::Int),
            "float" => Ok(TagType::Float),
            "bool" => Ok(TagType::Bool),
            "string" => Ok(TagType::String),
            "array" => Ok(TagType::Array),
            "null" => Ok(TagType::Null),
            _ => {
                self.errors.push(AstError::InvalidType {
                    span: node.span(),
                    t,
                });
                Err(Error("invalid type"))
            }
        }
    }

    fn typ(&mut self, node: &SyntaxNode) -> Result<ParamType> {
        self.assert_type(node, SyntaxKind::TYPE)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::TYPE)?;
        match c.kind() {
            SyntaxKind::OTEL_TYPE => Ok(ParamType::Terminal(TerminalParamType::Tag(
                self.otel_typ(&c)?,
            ))),
            SyntaxKind::MPL_TYPE => {
                let t = c.token_string();
                match t.as_str() {
                    "Dataset" => Ok(ParamType::Terminal(TerminalParamType::Dataset)),
                    "Duration" => Ok(ParamType::Terminal(TerminalParamType::Duration)),
                    "Regex" => Ok(ParamType::Terminal(TerminalParamType::Regex)),
                    // "Timestamp" => Ok(ParamType::Terminal(TerminalParamType::Timestamp)),
                    _ => {
                        self.errors.push(AstError::InvalidType {
                            span: node.span(),
                            t,
                        });
                        Err(Error("invalid type"))
                    }
                }
            }
            SyntaxKind::OPTION_TYPE => {
                let mut children = c.children();
                let Some(inner) = children.n() else {
                    self.errors.push(AstError::MissingToken {
                        expected: SyntaxKind::TYPE,
                        span: node.span(),
                    });
                    return Err(Error("invalid option type"));
                };
                let ParamType::Terminal(inner) = self.typ(&inner)? else {
                    self.errors
                        .push(AstError::NestedOption { span: inner.span() });
                    return Err(Error("invalid option type"));
                };
                Ok(ParamType::Optional(inner))
            }
            _ => {
                self.errors.push(AstError::InvalidType {
                    span: node.span(),
                    t: c.token_string(),
                });
                Err(Error("invalid type"))
            }
        }
    }

    fn param(&mut self, node: SyntaxNode) -> Result<Param> {
        self.assert_type(&node, SyntaxKind::PARAM)?;
        let mut children = node.children();
        let name = self.require_variable(&node, &mut children)?;

        let ty = if let Some(c) = children.n() {
            self.typ(&c)?
        } else {
            self.errors.push(AstError::MissingToken {
                expected: SyntaxKind::TYPE,
                span: node.span(),
            });
            return Err(Error("missing type"));
        };
        self.assert_end(children);
        Ok(Param { node, name, ty })
    }

    fn directive(&mut self, node: SyntaxNode) -> Result<Directive> {
        self.assert_type(&node, SyntaxKind::DIRECTIVE)?;
        let mut children = node.children();
        let name = self.require_ident(&node, &mut children)?;

        let value = if let Some(c) = children.n() {
            Some(self.constant(&c)?)
        } else {
            None
        };
        self.assert_end(children);
        Ok(Directive { node, name, value })
    }

    /// Lower the Syntax Tree
    #[must_use]
    pub fn lower(mut self) -> Ast {
        for child in self.root.children() {
            // We do not abort on an error this way we can keep parsing and potentially
            // collect multiple errors before returning.
            match child.kind() {
                SyntaxKind::DIRECTIVE => {
                    if let Ok(d) = self.directive(child) {
                        self.parts.push(Part::Directive(d));
                    }
                }

                SyntaxKind::PARAM => {
                    if let Ok(p) = self.param(child) {
                        self.parts.push(Part::Param(p));
                    }
                }
                SyntaxKind::QUERY => {
                    if let Ok(q) = self.query(&child) {
                        self.parts.push(Part::Query(q));
                    }
                }
                k if k.is_trivia() => {}
                k => {
                    self.errors.push(AstError::UnexpectedSyntaxRule {
                        expected: &[SyntaxKind::DIRECTIVE, SyntaxKind::PARAM, SyntaxKind::QUERY],
                        found: k,
                        span: child.span(),
                    });
                }
            }
        }
        Ast {
            parts: self.parts,
            errors: self.errors,
            warnings: self.warnings,
        }
    }
}
