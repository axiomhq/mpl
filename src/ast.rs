#![allow(dead_code)]
use miette::{Diagnostic, SourceSpan};
use rowan::{NodeOrToken, SyntaxElementChildren, SyntaxNodeChildren, SyntaxToken};

use crate::{
    Query, STDLIB,
    linker::Module,
    query::ParamType,
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
    NotImplemented,
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

    // fn assert_types(&mut self, node: &SyntaxNode, expected: &'static [SyntaxKind]) -> Result<()> {
    //     let matches = expected.contains(&node.kind());
    //     if matches {
    //         Ok(())
    //     } else {
    //         self.errors.push(ParserError::UnexpectedSyntaxRule {
    //             expected,
    //             found: node.kind(),
    //             range: range_to_span(node.text_range()),
    //         });
    //         Err(Error {})
    //     }
    // }
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
        // self.assert_end(children);
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

    fn kw(&mut self, node: &SyntaxNode) -> Result<String> {
        self.assert_type(node, SyntaxKind::KEYWORD)?;
        self.ident_body(node)
    }

    fn parse_bool(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::BOOL)?;
        // self.assert_end(children);
        todo!()
    }

    fn parse_null(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::NULL)?;
        // self.assert_end(children);
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
        dbg!(node);
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
        // self.assert_end(children);
        todo!()
    }

    fn parse_string(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::STRING)?;
        // self.assert_end(children);
        todo!()
    }

    fn parse_array(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::ARRAY)?;
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

    fn parse_param(&mut self, node: &SyntaxNode) -> Result<Param> {
        self.assert_type(node, SyntaxKind::PARAM)?;
        // self.assert_end(children);
        Err(Error("param not implemented"))
    }

    fn parse_directive(&mut self, node: &SyntaxNode) -> Result<Directive> {
        self.assert_type(node, SyntaxKind::DIRECTIVE)?;
        let mut children = node.children();
        if let Some(c) = children.n() {
            let kw = self.kw(&c)?;
            assert_eq!(kw, "set");
        }
        let name = if let Some(c) = children.n() {
            self.ident(&c)?
        } else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::IDENT,
                span: node.span(),
            });
            return Err(Error("missing token"));
        };

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

    use miette::{GraphicalReportHandler, GraphicalTheme, MietteDiagnostic, NamedSource, Report};

    use super::*;

    /// Renders the parser's errors the way a user would see them, so a failing example prints a
    /// diagnostic rather than a debug dump.
    fn report(name: &str, content: &str, errors: &[&ParserError]) -> String {
        let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
        let mut out = String::new();
        for error in errors {
            // `Report` needs to own its diagnostic, so copy the derived spans and codes across;
            // the labels are what let the handler slice a snippet out of the source.
            let diagnostic = MietteDiagnostic {
                message: error.to_string(),
                code: error.code().map(|code| code.to_string()),
                severity: error.severity(),
                help: error.help().map(|help| help.to_string()),
                url: error.url().map(|url| url.to_string()),
                labels: error.labels().map(Iterator::collect),
            };
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
