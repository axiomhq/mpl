#![allow(dead_code)]
use std::ops::Deref;

use miette::{Diagnostic, MietteDiagnostic, SourceSpan};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxNodeChildren, SyntaxToken};

use crate::{
    STDLIB,
    linker::Module,
    query::{ParamType, TagType, TerminalParamType},
    syntax_tree::{self, Lang, SyntaxError, SyntaxKind, SyntaxNode, SyntaxTree},
    tags::TagValue,
};

#[derive(thiserror::Error, Debug, Diagnostic)]
/// Represents a parser error.
pub enum ParserError {
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

    /// Garbage at the end a rule.
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
        found: Ident,
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
}

impl ParserError {
    /// Converts this error into a [`MietteDiagnostic`].
    #[must_use]
    pub fn to_diagnostic(&self) -> MietteDiagnostic {
        if let ParserError::InvalidSyntax(error) = self {
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
pub struct Error(&'static str);

/// AST parser result type.
pub type Result<T> = std::result::Result<T, Error>;

/// A `param` declaration.
#[derive(Debug)]
pub struct Param {
    /// The parameter name, without the leading `$`.
    pub name: Variable,
    /// The declared type.
    pub ty: ParamType,
}

/// A `set` directive.
#[derive(Debug, Clone)]
pub struct Directive {
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

/// AST parser.
#[allow(dead_code)] // FIXME: delete this
pub struct Parser {
    root: SyntaxNode,
    stdlib: &'static Module,
    errors: Vec<ParserError>,
    parts: Vec<Part>,
}

trait NonTrivalItem {
    fn span(&self) -> SourceSpan;
    // FIXME: showd this be a cow?
    fn token_string(&self) -> String;
}

impl NonTrivalItem for SyntaxNode {
    fn span(&self) -> SourceSpan {
        let s: usize = self.text_range().start().into();
        let e: usize = self.text_range().end().into();
        SourceSpan::new(s.into(), e - s)
    }

    fn token_string(&self) -> String {
        self.text().to_string()
    }
}
impl NonTrivalItem for NodeOrToken<SyntaxNode, SyntaxToken<Lang>> {
    fn span(&self) -> SourceSpan {
        let s: usize = self.text_range().start().into();
        let e: usize = self.text_range().end().into();
        SourceSpan::new(s.into(), e - s)
    }

    fn token_string(&self) -> String {
        self.to_string()
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
pub struct Regex(String);

/// Identifier.
#[derive(Debug, Clone)]
pub struct Ident(String);

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Ident {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for Ident {
    fn eq(&self, other: &&str) -> bool {
        self.0 == **other
    }
}

/// Identifier.
#[derive(Debug, Clone)]
pub struct Variable(String);

impl std::fmt::Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Variable {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Variable {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for Variable {
    fn eq(&self, other: &&str) -> bool {
        self.0 == **other
    }
}

/// A filter expression.
#[derive(Debug)]
pub enum Expr {
    /// A literal value.
    Ident(Ident),
    /// A variable reference.
    Var(Variable),
    /// A constant value.
    Const(TagValue),
}

/// A filter comparison rule.
#[derive(Debug)]
pub enum FilterCmp {
    /// A equality
    Eq {
        /// The left-hand side of the equality.
        lhs: Ident,
        /// The right-hand side of the equality.
        rhs: Expr,
    },
    /// A not equality
    Neq {
        /// The left-hand side of the not equality.
        lhs: Ident,
        /// The right-hand side of the not equality.
        rhs: Expr,
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
        rhs: Expr,
    },
    /// A greater than comparison
    Gt {
        /// The left-hand side of the greater than comparison.
        lhs: Ident,
        /// The right-hand side of the greater than comparison.
        rhs: Expr,
    },
    /// A less than or equal comparison
    Lte {
        /// The left-hand side of the less than or equal comparison.
        lhs: Ident,
        /// The right-hand side of the less than or equal comparison.
        rhs: Expr,
    },
    /// A greater than or equal comparison
    Gte {
        /// The left-hand side of the greater than or equal comparison.
        lhs: Ident,
        /// The right-hand side of the greater than or equal comparison.
        rhs: Expr,
    },
    /// An in comparison
    In {
        /// The left-hand side of the in comparison.
        lhs: Ident,
        /// The right-hand side of the in comparison.
        rhs: Expr,
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
pub struct FilterAnd(Vec<FilterNot>);
/// A filter or rule.
#[derive(Debug)]
pub struct FilterOr(Vec<FilterAnd>);

/// A parsed rule.
#[derive(Debug)]
pub enum Rule {
    /// A parsed filter rule.
    Filter(FilterOr),
    /// A parsed sample rule.
    Sample,
    /// A parsed map rule.
    Map,
    /// A parsed align rule.
    Align,
    /// A parsed group rule.
    Group,
    /// A parsed bucket rule.
    Bucket,
    /// A parsed ifdef rule.
    IfDef,
    /// A parsed extern rule.
    Extern,
    /// A parsed as rule.
    As,
}

/// A parsed simple query.
#[derive(Debug)]
pub struct SimpleQuery {
    /// The dataset to compute on.
    dataset: IdentOrVariable,
    /// The metric to compute.
    metric: Ident,
    /// The alias to use for the metric.
    alias: Option<Ident>,
    /// The rules to apply.
    rules: Vec<Rule>,
}

/// A parsed compute query.
#[derive(Debug)]
pub struct ComputeQuery {
    l: Query,
    r: Query,
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
            stdlib: &STDLIB,
            root,
            errors: errors.into_iter().map(ParserError::InvalidSyntax).collect(),
            parts: Vec::new(),
        }
    }

    /// errors that occurred during parsing.
    #[must_use]
    pub fn errors(&self) -> &[ParserError] {
        &self.errors
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
            self.errors.push(ParserError::MissingToken {
                expected: error_kind,
                span: node.span(),
            });
            return Err(Error("expected node"));
        };
        Ok(node)
    }

    fn not_implemented(&mut self, node: &SyntaxNode) {
        self.errors.push(ParserError::NotImplemented {
            span: node.span(),
            kind: node.kind(),
        });
    }

    fn assert_type(&mut self, node: &SyntaxNode, expected: SyntaxKind) -> Result<()> {
        if node.kind() == expected {
            Ok(())
        } else {
            self.errors.push(ParserError::UnexpectedSyntaxRuleOne {
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
            .push(ParserError::GarbageAtEndOfRule { span: node.span() });
    }
    fn ident_or_variable(&mut self, node: &SyntaxNode) -> Result<IdentOrVariable> {
        self.assert_type(node, SyntaxKind::IDENT_OR_VARIABLE)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::IDENT)?;
        let r = match c.kind() {
            SyntaxKind::IDENT => self.ident(&c).map(IdentOrVariable::Ident),
            SyntaxKind::VARIABLE => self.variable(&c).map(IdentOrVariable::Var),
            _ => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
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

    fn string_expr(&mut self, node: &SyntaxNode) -> Result<Expr> {
        self.assert_type(node, SyntaxKind::STRING_START)?;
        self.not_implemented(node);
        Ok(Expr::Const(TagValue::Null))
    }

    fn array_expr(&mut self, node: &SyntaxNode) -> Result<Expr> {
        self.assert_type(node, SyntaxKind::ARRAY)?;
        self.not_implemented(node);
        Ok(Expr::Const(TagValue::Null))
    }

    fn expr_value(&mut self, node: &SyntaxNode) -> Result<Expr> {
        self.assert_type(node, SyntaxKind::CONST)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::CONST)?;

        let r = match c.kind() {
            SyntaxKind::INTEGER => Expr::Const(self.integer_const(&c)?),
            SyntaxKind::FLOAT => Expr::Const(self.float_const(&c)?),
            SyntaxKind::BOOL => Expr::Const(self.bool_const(&c)?),
            SyntaxKind::STRING => Expr::Const(self.string_const(&c)?),
            SyntaxKind::NULL => Expr::Const(self.null_const(&c)?),
            SyntaxKind::STRING_START => self.string_expr(&c)?,
            SyntaxKind::ARRAY => self.array_expr(&c)?,
            found => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[
                        SyntaxKind::INTEGER,
                        SyntaxKind::FLOAT,
                        SyntaxKind::STRING,
                        SyntaxKind::STRING_START,
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

    fn expr(&mut self, node: &SyntaxNode) -> Result<Expr> {
        self.assert_type(node, SyntaxKind::EXPR)?;
        let mut children = node.children();
        let n = self.n(&mut children, node, SyntaxKind::EXPR)?;
        let r = match n.kind() {
            SyntaxKind::CONST => self.expr_value(&n)?,
            SyntaxKind::IDENT => Expr::Ident(self.ident(&n)?),
            SyntaxKind::VARIABLE => Expr::Var(self.variable(&n)?),
            found => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::CONST, SyntaxKind::IDENT, SyntaxKind::VARIABLE],
                    found,
                    span: n.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        Ok(r)
    }

    fn regex(&mut self, node: &SyntaxNode) -> Result<Regex> {
        self.assert_type(node, SyntaxKind::REGEX)?;
        let mut children = node.children_with_tokens();
        let regex = self.n(&mut children, node, SyntaxKind::REGEX)?;
        self.assert_end(children);
        Ok(Regex(regex.token_string()))
    }

    fn filter_cmp(&mut self, node: &SyntaxNode) -> Result<FilterCmp> {
        self.assert_type(node, SyntaxKind::FILTER_CMP)?;
        let mut children = node.children();
        let c = self.n(&mut children, node, SyntaxKind::IDENT)?;
        let lhs = self.ident(&c)?;
        let c = self.n(&mut children, node, SyntaxKind::FILTER_CMP)?;
        let r = match c.kind() {
            SyntaxKind::FILTER_CMP_EQ => {
                let mut children = c.children();
                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                if c.kind() == SyntaxKind::REGEX {
                    let rhs = self.regex(&c)?;
                    Ok(FilterCmp::EqRe { lhs, rhs })
                } else {
                    let rhs = self.expr(&c)?;
                    Ok(FilterCmp::Eq { lhs, rhs })
                }
            }
            SyntaxKind::FILTER_CMP_NEQ => {
                let mut children = c.children();

                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                if c.kind() == SyntaxKind::REGEX {
                    let rhs = self.regex(&c)?;
                    Ok(FilterCmp::NeqRe { lhs, rhs })
                } else {
                    let rhs = self.expr(&c)?;
                    Ok(FilterCmp::Neq { lhs, rhs })
                }
            }
            SyntaxKind::FILTER_CMP_LT => {
                let mut children = c.children();

                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                let rhs = self.expr(&c)?;
                Ok(FilterCmp::Lt { lhs, rhs })
            }
            SyntaxKind::FILTER_CMP_GT => {
                let mut children = c.children();

                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                let rhs = self.expr(&c)?;
                Ok(FilterCmp::Gt { lhs, rhs })
            }
            SyntaxKind::FILTER_CMP_LTE => {
                let mut children = c.children();

                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                let rhs = self.expr(&c)?;
                Ok(FilterCmp::Lte { lhs, rhs })
            }
            SyntaxKind::FILTER_CMP_GTE => {
                let mut children = c.children();

                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                let rhs = self.expr(&c)?;
                Ok(FilterCmp::Gte { lhs, rhs })
            }
            SyntaxKind::FILTER_CMP_IN => {
                let c = self.n(&mut children, node, SyntaxKind::EXPR)?;
                let rhs = self.expr(&c)?;
                Ok(FilterCmp::In { lhs, rhs })
            }
            SyntaxKind::FILTER_CMP_IS => {
                let mut children = c.children();
                let c = self.n(&mut children, node, SyntaxKind::OTEL_TYPE)?;
                let rhs = self.otel_typ(&c)?;
                Ok(FilterCmp::Is { lhs, rhs })
            }
            _ => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
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
            let found = Ident(c.token_string());
            if found == "not" {
                let c = self.n(&mut children, node, SyntaxKind::FILTER_PAREN)?;
                Ok(FilterNot::Not(self.filter_paren(&c)?))
            } else {
                self.errors.push(ParserError::UnexpectedKeyword {
                    expected: "not",
                    found,
                    span: c.span(),
                });
                Err(Error("expected 'not' keyword"))
            }
        } else {
            Ok(FilterNot::Yes(self.filter_paren(&c)?))
        }
    }

    fn filter_and(&mut self, node: &SyntaxNode) -> Result<FilterAnd> {
        self.assert_type(node, SyntaxKind::FILTER_AND)?;
        let mut children = node.children();
        let mut filters = Vec::new();
        while let Some(child) = children.n() {
            filters.push(self.filter_not(&child)?);
        }
        self.assert_end(children);
        Ok(FilterAnd(filters))
    }

    fn filter_or(&mut self, node: &SyntaxNode) -> Result<FilterOr> {
        self.assert_type(node, SyntaxKind::FILTER_OR)?;
        let mut children = node.children();
        let mut filters = Vec::new();
        while let Some(child) = children.n() {
            filters.push(self.filter_and(&child)?);
        }
        self.assert_end(children);
        Ok(FilterOr(filters))
    }

    fn rule_filter(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::FILTER)?;
        let mut children = node.children();
        let kw = self.n(&mut children, node, SyntaxKind::KEYWORD)?;

        let found = self.kw(&kw)?;
        if !matches!(found.as_str(), "filter" | "where") {
            self.errors.push(ParserError::UnexpectedKeyword {
                expected: "filter",
                found,
                span: kw.span(),
            });
            return Err(Error("wrong type"));
        }

        let n = self.n(&mut children, node, SyntaxKind::FILTER_OR)?;
        let f = self.filter_or(&n)?;
        self.assert_end(children);
        Ok(Rule::Filter(f))
    }
    fn rule_sample(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::SAMPLE)?;
        self.not_implemented(node);
        Ok(Rule::Sample)
    }
    fn rule_map(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::MAP)?;
        self.not_implemented(node);
        Ok(Rule::Map)
    }
    fn rule_align(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::ALIGN)?;
        self.not_implemented(node);
        Ok(Rule::Align)
    }
    fn rule_group(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::GROUP)?;
        self.not_implemented(node);
        Ok(Rule::Group)
    }
    fn rule_bucket(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::BUCKET)?;
        self.not_implemented(node);
        Ok(Rule::Bucket)
    }
    fn rule_ifdef(&mut self, node: &SyntaxNode) -> Result<Rule> {
        self.assert_type(node, SyntaxKind::IFDEF)?;
        self.not_implemented(node);
        Ok(Rule::IfDef)
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
            kind => {
                self.errors.push(ParserError::UnknownRule {
                    kind,
                    span: r.span(),
                });
                Err(Error("unknown rule"))
            }
        }
    }

    fn simple_query(&mut self, node: &SyntaxNode) -> Result<SimpleQuery> {
        self.assert_type(node, SyntaxKind::SIMPLE_QUERY)?;
        let mut children = node.children();

        let c = self.n(&mut children, node, SyntaxKind::IDENT)?;
        let dataset = self.dataset(&c)?;
        let c = self.n(&mut children, node, SyntaxKind::IDENT)?;
        let metric = self.ident(&c)?;

        let Some(mut c) = children.n() else {
            return Ok(SimpleQuery {
                dataset,
                metric,
                alias: None,
                rules: Vec::new(),
            });
        };
        let mut alias = None;
        if c.kind() == SyntaxKind::KEYWORD {
            let found = self.kw(&c)?;
            if found == "as" {
                let c = self.n(&mut children, node, SyntaxKind::IDENT)?;
                alias = Some(self.ident(&c)?);
            } else {
                self.errors.push(ParserError::UnexpectedKeyword {
                    expected: "as",
                    found,
                    span: c.span(),
                });
            }
            let Some(n) = children.n() else {
                return Ok(SimpleQuery {
                    dataset,
                    metric,
                    alias,
                    rules: Vec::new(),
                });
            };
            c = n;
        }
        let mut rules = vec![self.rule(&c)?];
        while let Some(c) = children.n() {
            rules.push(self.rule(&c)?);
        }

        Ok(SimpleQuery {
            dataset,
            metric,
            alias,
            rules,
        })
    }

    fn compute_query(&mut self, node: &SyntaxNode) -> Result<ComputeQuery> {
        self.assert_type(node, SyntaxKind::COMPUTE_QUERY)?;
        let mut children = node.children();

        let c = self.n(&mut children, node, SyntaxKind::SIMPLE_QUERY)?;
        let l = self.query(&c)?;
        let c = self.n(&mut children, node, SyntaxKind::SIMPLE_QUERY)?;
        let r = self.query(&c)?;

        let c = self.n(&mut children, node, SyntaxKind::RULE)?;
        self.not_implemented(&c);

        Ok(ComputeQuery { l, r })
    }
    fn query(&mut self, node: &SyntaxNode) -> Result<Query> {
        self.assert_type(node, SyntaxKind::QUERY)?;
        let mut children = node.children();

        let c = self.n(&mut children, node, SyntaxKind::SIMPLE_QUERY)?;

        let r = match c.kind() {
            SyntaxKind::SIMPLE_QUERY => Query::Simple(self.simple_query(&c)?),
            SyntaxKind::COMPUTE_QUERY => Query::Compute(Box::new(self.compute_query(&c)?)),
            found => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
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

    fn ident_body(&mut self, node: &SyntaxNode) -> Result<Ident> {
        let mut children = node.children_with_tokens();
        let node = self.n(&mut children, node, SyntaxKind::LX_IDENT)?;
        let r = match node.kind() {
            SyntaxKind::LX_IDENT => node.token_string(),
            SyntaxKind::LX_ESCAPED_IDENT => {
                // FIXME: unescape
                node.token_string()
            }
            found => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_IDENT, SyntaxKind::LX_ESCAPED_IDENT],
                    found,
                    span: node.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        self.assert_end(children);
        Ok(Ident(r))
    }
    fn ident(&mut self, node: &SyntaxNode) -> Result<Ident> {
        self.assert_type(node, SyntaxKind::IDENT)?;
        self.ident_body(node)
    }

    fn variable(&mut self, node: &SyntaxNode) -> Result<Variable> {
        self.assert_type(node, SyntaxKind::VARIABLE)?;
        let mut children = node.children_with_tokens();
        let node = self.n(&mut children, node, SyntaxKind::LX_VARIABLE)?;
        let r = match node.kind() {
            SyntaxKind::LX_VARIABLE => node.token_string(),
            SyntaxKind::LX_ESCAPED_VARIABLE => {
                // FIXME: unescape
                node.token_string()
            }
            found => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_VARIABLE, SyntaxKind::LX_ESCAPED_VARIABLE],
                    found,
                    span: node.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        self.assert_end(children);
        Ok(Variable(r))
    }

    fn kw(&mut self, node: &SyntaxNode) -> Result<Ident> {
        self.assert_type(node, SyntaxKind::KEYWORD)?;
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
                    .push(ParserError::InvalidBoolConstant { span: node.span() });
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
                self.errors.push(ParserError::UnexpectedSyntaxRuleOne {
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
                .push(ParserError::InvalidIntegerConstant { span: node.span() });
            Err(Error("invalid integer"))
        }
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
                        .push(ParserError::InvalidFloatConstant { span: node.span() });
                    Err(Error("invalid integer"))
                }
            }
            SyntaxKind::LX_INF => Ok(TagValue::Float(f64::INFINITY)),
            found => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
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
        let mut children = node.children_with_tokens();
        // FIXME, unescape
        let node = self.n(&mut children, node, SyntaxKind::LX_STRING)?;
        let s = node.token_string();
        let r = match s.try_into() {
            Ok(s) => Ok(TagValue::String(s)),
            Err(e) => {
                self.errors.push(ParserError::FailedToCreateString {
                    error: e,
                    span: node.span(),
                });
                Err(Error("failed to create string"))
            }
        };
        self.assert_end(children);
        r
    }

    fn const_expr(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::EXPR)?;
        let mut children = node.children();
        let n = self.n(&mut children, node, SyntaxKind::EXPR)?;
        let r = if n.kind() == SyntaxKind::CONST {
            self.constant(&n)
        } else {
            self.errors
                .push(ParserError::ExpectedConst { span: n.span() });
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
                self.errors.push(ParserError::UnexpectedSyntaxRule {
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

    fn check_kw(
        &mut self,
        node: &SyntaxNode,
        expected: &'static str,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<()> {
        let c = self.n(children, node, SyntaxKind::KEYWORD)?;
        let found = self.kw(&c)?;
        if found == expected {
            Ok(())
        } else {
            self.errors.push(ParserError::UnexpectedKeyword {
                expected,
                found,
                span: c.span(),
            });
            Err(Error("unexpected syntax"))
        }
    }

    fn require_ident(
        &mut self,
        node: &SyntaxNode,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<Ident> {
        let c = self.n(children, node, SyntaxKind::IDENT)?;
        self.ident(&c)
    }

    fn require_variable(
        &mut self,
        node: &SyntaxNode,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<Variable> {
        let c = self.n(children, node, SyntaxKind::VARIABLE)?;
        self.variable(&c)
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
            _ => {
                self.errors.push(ParserError::InvalidType {
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
                        self.errors.push(ParserError::InvalidType {
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
                    self.errors.push(ParserError::MissingToken {
                        expected: SyntaxKind::TYPE,
                        span: node.span(),
                    });
                    return Err(Error("invalid option type"));
                };
                let ParamType::Terminal(inner) = self.typ(&inner)? else {
                    self.errors
                        .push(ParserError::NestedOption { span: inner.span() });
                    return Err(Error("invalid option type"));
                };
                Ok(ParamType::Optional(inner))
            }
            _ => {
                self.errors.push(ParserError::InvalidType {
                    span: node.span(),
                    t: c.token_string(),
                });
                Err(Error("invalid type"))
            }
        }
    }

    fn param(&mut self, node: &SyntaxNode) -> Result<Param> {
        self.assert_type(node, SyntaxKind::PARAM)?;
        let mut children = node.children();
        self.check_kw(node, "param", &mut children)?;

        let name = self.require_variable(node, &mut children)?;

        let ty = if let Some(c) = children.n() {
            self.typ(&c)?
        } else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::TYPE,
                span: node.span(),
            });
            return Err(Error("missing type"));
        };
        // self.assert_end(children);
        Ok(Param { name, ty })
    }

    fn directive(&mut self, node: &SyntaxNode) -> Result<Directive> {
        self.assert_type(node, SyntaxKind::DIRECTIVE)?;
        let mut children = node.children();
        self.check_kw(node, "set", &mut children)?;
        let name = self.require_ident(node, &mut children)?;

        let value = if let Some(c) = children.n() {
            Some(self.constant(&c)?)
        } else {
            None
        };
        self.assert_end(children);
        Ok(Directive { name, value })
    }

    /// Lower the Syntax Tree
    pub fn lower(&mut self) -> Result<()> {
        for child in self.root.children() {
            // We do not abort on an error this way we can keep parsing and potentially
            // collect multiple errors before returning.
            match child.kind() {
                SyntaxKind::DIRECTIVE => {
                    if let Ok(d) = self.directive(&child) {
                        self.parts.push(Part::Directive(d));
                    }
                }

                SyntaxKind::PARAM => {
                    if let Ok(p) = self.param(&child) {
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
                    self.errors.push(ParserError::UnexpectedSyntaxRule {
                        expected: &[SyntaxKind::DIRECTIVE, SyntaxKind::PARAM, SyntaxKind::QUERY],
                        found: k,
                        span: child.span(),
                    });
                }
            }
        }
        // FIXME: nope
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource, Report};

    /// Renders the parser's errors the way a user would see them, so a failing example prints a
    /// diagnostic rather than a debug dump.
    fn report(name: &str, content: &str, errors: &[&ParserError]) -> String {
        let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
        let mut out = String::new();
        for error in errors {
            let diagnostic = error.to_diagnostic();
            let report = Report::new(diagnostic)
                .with_source_code(NamedSource::new(name, content.to_string()));
            let mut rendered = String::new();
            if handler
                .render_report(&mut rendered, report.as_ref())
                .is_err()
            {
                rendered = report.to_string();
            }
            out.push_str(&rendered);
        }
        out
    }

    #[test]
    fn test_ast_parse() -> Result<()> {
        let input = r#"
            // test
            set a = 43;
            set b;
            set c = 1.2;
            set d = [1, 2, "snot", [42.0, []]];
            param $test: string;
            param $test2: Option<string>;
            a:b as c
            | where code == #/[123]../
            "#;
        let mut parser = Parser::new(input);
        parser.lower()?;
        for error in &parser.errors {
            eprintln!("{}", report("test", input, &[error]));
        }
        assert!(parser.errors.is_empty());
        dbg!(&parser.parts);
        Ok(())
    }
}
