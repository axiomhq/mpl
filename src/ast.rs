#![allow(dead_code)]
use miette::{Diagnostic, MietteDiagnostic, SourceSpan};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxNodeChildren, SyntaxToken};

use crate::{
    Query, STDLIB,
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
    /// Not implemented.
    #[error("not implemented")]
    #[diagnostic(code(mpl_lang::not_implemented))]
    NotImplemented {
        /// The range of the not implemented node.
        #[label("not implemented")]
        span: SourceSpan,
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
}
impl ParserError {
    /// Converts this error into a [`MietteDiagnostic`].
    fn to_diagnostic(&self) -> MietteDiagnostic {
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

#[derive(Debug)]
struct Param {
    name: String,
    ty: ParamType,
}

#[derive(Debug, Clone)]
struct Directive {
    name: String,
    value: Option<TagValue>,
}

#[derive(Debug)]
enum Part {
    Directive(Directive),
    Param(Param),
    Query(()),
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
    fn parse_query(&mut self, node: &SyntaxNode) -> Result<()> {
        self.assert_type(node, SyntaxKind::QUERY)?;
        // todo: self.assert_end(children);
        Ok(())
    }

    fn ident_body(&mut self, node: &SyntaxNode) -> Result<String> {
        let mut children = node.children_with_tokens();
        let Some(node) = children.n() else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::LX_IDENT,
                span: node.span(),
            });
            return Err(Error("missing token"));
        };
        let r = match node.kind() {
            SyntaxKind::LX_IDENT => node.token_string(),
            SyntaxKind::LX_ESCAPED_IDENT => {
                // FIXME: unescape
                node.token_string()
            }
            _ => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_IDENT, SyntaxKind::LX_ESCAPED_IDENT],
                    found: node.kind(),
                    span: node.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        self.assert_end(children);
        Ok(r)
    }
    fn ident(&mut self, node: &SyntaxNode) -> Result<String> {
        self.assert_type(node, SyntaxKind::IDENT)?;
        self.ident_body(node)
    }

    fn variable(&mut self, node: &SyntaxNode) -> Result<String> {
        self.assert_type(node, SyntaxKind::VARIABLE)?;
        let mut children = node.children_with_tokens();
        let Some(node) = children.n() else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::LX_VARIABLE,
                span: node.span(),
            });
            return Err(Error("missing token"));
        };
        let r = match node.kind() {
            SyntaxKind::LX_VARIABLE => node.token_string(),
            SyntaxKind::LX_ESCAPED_VARIABLE => {
                // FIXME: unescape
                node.token_string()
            }
            _ => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_VARIABLE, SyntaxKind::LX_ESCAPED_VARIABLE],
                    found: node.kind(),
                    span: node.span(),
                });
                return Err(Error("unexpected syntax"));
            }
        };
        self.assert_end(children);
        Ok(r)
    }

    fn kw(&mut self, node: &SyntaxNode) -> Result<String> {
        self.assert_type(node, SyntaxKind::KEYWORD)?;
        self.ident_body(node)
    }

    fn parse_bool(&mut self, node: &SyntaxNode) -> Result<TagValue> {
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

    fn parse_null(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::NULL)?;
        Ok(TagValue::Null)
    }

    fn token_of_type(&mut self, node: &SyntaxNode, kind: SyntaxKind) -> Result<String> {
        let mut children = node.children_with_tokens();
        let r = match children.n() {
            Some(token) if token.kind() == kind => Ok(token.token_string()),
            Some(token) => {
                self.errors.push(ParserError::UnexpectedSyntaxRuleOne {
                    expected: kind,
                    found: token.kind(),
                    span: token.span(),
                });
                Err(Error("token of wrong type"))
            }
            _ => {
                self.errors.push(ParserError::MissingToken {
                    expected: kind,
                    span: node.span(),
                });
                Err(Error("missing token"))
            }
        };
        self.assert_end(children);
        r
    }

    fn parse_integer(&mut self, node: &SyntaxNode) -> Result<TagValue> {
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

    fn parse_float(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::FLOAT)?;
        let mut children = node.children_with_tokens();
        let r = match children.n() {
            Some(token) if token.kind() == SyntaxKind::LX_FLOAT => {
                if let Ok(value) = token.token_string().parse::<f64>() {
                    Ok(TagValue::Float(value))
                } else {
                    self.errors
                        .push(ParserError::InvalidFloatConstant { span: node.span() });
                    Err(Error("invalid integer"))
                }
            }
            Some(token) if token.kind() == SyntaxKind::LX_INF => Ok(TagValue::Float(f64::INFINITY)),
            Some(token) => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_FLOAT, SyntaxKind::LX_INF],
                    found: token.kind(),
                    span: token.span(),
                });
                Err(Error("token of wrong type"))
            }
            _ => {
                self.errors.push(ParserError::MissingToken {
                    expected: SyntaxKind::LX_FLOAT,
                    span: node.span(),
                });
                Err(Error("missing token"))
            }
        };
        self.assert_end(children);
        r
    }

    fn parse_string(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::STRING)?;
        self.errors
            .push(ParserError::NotImplemented { span: node.span() });
        // self.assert_end(children);
        todo!()
    }

    fn parse_array(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::ARRAY)?;
        self.errors
            .push(ParserError::NotImplemented { span: node.span() });
        // self.assert_end(children);
        todo!()
    }

    fn constant(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::CONST)?;
        let mut children = node.children();

        let r = match children.n() {
            Some(c) if c.kind() == SyntaxKind::INTEGER => self.parse_integer(&c),
            Some(c) if c.kind() == SyntaxKind::FLOAT => self.parse_float(&c),
            Some(c) if c.kind() == SyntaxKind::STRING => self.parse_string(&c),
            Some(c) if c.kind() == SyntaxKind::BOOL => self.parse_bool(&c),
            Some(c) if c.kind() == SyntaxKind::ARRAY => self.parse_array(&c),
            Some(c) if c.kind() == SyntaxKind::NULL => self.parse_null(&c),
            Some(c) => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[
                        SyntaxKind::INTEGER,
                        SyntaxKind::FLOAT,
                        SyntaxKind::STRING,
                        SyntaxKind::BOOL,
                        SyntaxKind::ARRAY,
                        SyntaxKind::NULL,
                    ],
                    found: c.kind(),
                    span: c.span(),
                });
                Err(Error("unexpected syntax"))
            }
            None => Err(Error("missing token")), // FIXME we need an error here
        };
        self.assert_end(children);
        r
    }

    fn check_kw(
        &mut self,
        n: &SyntaxNode,
        expected: &'static str,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<()> {
        if let Some(c) = children.n() {
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
        } else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::KEYWORD,
                span: n.span(),
            });
            Err(Error("missing token"))
        }
    }

    fn require_ident(
        &mut self,
        node: &SyntaxNode,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<String> {
        if let Some(c) = children.n() {
            Ok(self.ident(&c)?)
        } else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::IDENT,
                span: node.span(),
            });
            Err(Error("missing token"))
        }
    }

    fn require_variable(
        &mut self,
        node: &SyntaxNode,
        children: &mut SyntaxNodeChildren<Lang>,
    ) -> Result<String> {
        if let Some(c) = children.n() {
            Ok(self.variable(&c)?)
        } else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::VARIABLE,
                span: node.span(),
            });
            Err(Error("missing token"))
        }
    }

    fn parse_type(&mut self, node: &SyntaxNode) -> Result<ParamType> {
        self.assert_type(node, SyntaxKind::TYPE)?;
        let mut children = node.children();
        match children.n() {
            Some(c) if c.kind() == SyntaxKind::OTEL_TYPE => {
                let t = c.token_string();
                match t.as_str() {
                    "int" => Ok(ParamType::Terminal(TerminalParamType::Tag(TagType::Int))),
                    "float" => Ok(ParamType::Terminal(TerminalParamType::Tag(TagType::Float))),
                    "bool" => Ok(ParamType::Terminal(TerminalParamType::Tag(TagType::Bool))),
                    "string" => Ok(ParamType::Terminal(TerminalParamType::Tag(TagType::String))),
                    "array" => Ok(ParamType::Terminal(TerminalParamType::Tag(TagType::Array))),
                    _ => {
                        self.errors.push(ParserError::InvalidType {
                            span: node.span(),
                            t,
                        });
                        Err(Error("invalid type"))
                    }
                }
            }
            Some(c) if c.kind() == SyntaxKind::MPL_TYPE => {
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
            Some(c) if c.kind() == SyntaxKind::OPTION_TYPE => {
                let mut children = c.children();
                let Some(inner) = children.n() else {
                    self.errors.push(ParserError::MissingToken {
                        expected: SyntaxKind::TYPE,
                        span: node.span(),
                    });
                    return Err(Error("invalid option type"));
                };
                let ParamType::Terminal(inner) = self.parse_type(&inner)? else {
                    self.errors
                        .push(ParserError::NestedOption { span: inner.span() });
                    return Err(Error("invalid option type"));
                };
                Ok(ParamType::Optional(inner))
            }
            Some(c) => {
                self.errors.push(ParserError::InvalidType {
                    span: node.span(),
                    t: c.token_string(),
                });
                Err(Error("invalid type"))
            }
            None => {
                self.errors.push(ParserError::MissingToken {
                    expected: SyntaxKind::TYPE,
                    span: node.span(),
                });
                Err(Error("missing token"))
            }
        }
    }

    fn parse_param(&mut self, node: &SyntaxNode) -> Result<Param> {
        self.assert_type(node, SyntaxKind::PARAM)?;
        let mut children = node.children();
        self.check_kw(node, "param", &mut children)?;

        let name = self.require_variable(node, &mut children)?;

        let ty = if let Some(c) = children.n() {
            self.parse_type(&c)?
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

    fn parse_directive(&mut self, node: &SyntaxNode) -> Result<Directive> {
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

    /// Lower the Syntax Tree into a [`Query`] ast
    pub fn lower(&mut self) -> Result<Query> {
        for child in self.root.children() {
            // We do not abort on an error this way we can keep parsing and potentially
            // collect multiple errors before returning.
            match child.kind() {
                SyntaxKind::DIRECTIVE => {
                    if let Ok(d) = self.parse_directive(&child) {
                        self.parts.push(Part::Directive(d));
                    }
                }

                SyntaxKind::PARAM => {
                    if let Ok(p) = self.parse_param(&child) {
                        self.parts.push(Part::Param(p));
                    }
                }
                SyntaxKind::QUERY => {
                    let _ = self.parse_query(&child);
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
        Err(Error("parsing error"))
    }
}

#[cfg(test)]
mod tests {

    use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource, Report};

    use super::*;

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
        let input = r"
            // test
            set a = 43;
            set b;
            set c = 1.2;
            param $test: string;
            param $test2: Option<string>;
            a:b
            ";
        let mut parser = Parser::new(input);
        let query = parser.lower();
        for error in &parser.errors {
            eprintln!("{}", report("test", input, &[error]));
        }
        dbg!(&parser.parts);
        assert!(parser.errors.is_empty());
        let _query = query?;
        Ok(())
    }
}
