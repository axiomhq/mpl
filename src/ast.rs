use miette::{Diagnostic, SourceSpan};
use rowan::{SyntaxNodeChildren, TextRange};

use crate::{
    Query, STDLIB,
    linker::Module,
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
        range: SourceSpan,
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
        range: SourceSpan,
    },

    /// Errors were encountered during parsing.
    #[error("errors encountered")]
    #[diagnostic(code(mpl_lang::errors_encountered))]
    ErrorsEncountered,
    /// Garbage at the end a rule.
    #[error("garbage at end of input")]
    #[diagnostic(code(mpl_lang::garbage_at_end))]
    GarbageAtEndOfRule {
        /// The source span of the garbage.
        #[label("garbage at end of rule")]
        range: SourceSpan,
    },
    /// Garbage at the end a rule.
    #[error("garbage at end of input")]
    #[diagnostic(code(mpl_lang::garbage_at_end))]
    MissingToken {
        /// The expected syntax kind
        expected: SyntaxKind,
        /// The source span of the garbage.
        #[label("missing token of kind {expected:?}")]
        range: SourceSpan,
    },
}

/// Represents a parser error.
#[derive(Debug)]
pub struct Error {}

/// AST parser result type.
pub type Result<T> = std::result::Result<T, Error>;

/// AST parser.
#[allow(dead_code)] // FIXME: delete this
pub struct Parser {
    root: SyntaxNode,
    stdlib: &'static Module,
    errors: Vec<ParserError>,
}

fn skip_trivia(c: &mut SyntaxNodeChildren<Lang>) -> Option<SyntaxNode> {
    for node in c {
        if node.kind().is_trivia() {
            continue;
        }
        return Some(node);
    }
    None
}

impl Parser {
    /// Creates a new parser with the given input.
    pub fn new(input: &str) -> Self {
        let SyntaxTree { root, errors } = syntax_tree::Parser::new(input).parse();
        Parser {
            stdlib: &STDLIB,
            root,
            errors: errors.into_iter().map(ParserError::InvalidSyntax).collect(),
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
                range: range_to_span(node.text_range()),
            });
            Err(Error {})
        }
    }

    fn assert_end(&mut self, mut children: SyntaxNodeChildren<Lang>) {
        let Some(node) = skip_trivia(&mut children) else {
            return;
        };
        self.errors.push(ParserError::GarbageAtEndOfRule {
            range: range_to_span(node.text_range()),
        });
    }
    fn parse_query(&mut self, node: &SyntaxNode) -> Result<()> {
        self.assert_type(node, SyntaxKind::QUERY)?;
        // self.assert_end(children);
        Ok(())
    }
    fn parse_param(&mut self, node: &SyntaxNode) -> Result<()> {
        self.assert_type(node, SyntaxKind::PARAM)?;
        // self.assert_end(children);
        Ok(())
    }
    fn ident(&mut self, node: &SyntaxNode) -> Result<String> {
        self.assert_type(node, SyntaxKind::IDENT)?;
        dbg!(node);
        // self.assert_end(children);
        Ok(String::new())
    }

    fn kw(&mut self, node: &SyntaxNode) -> Result<String> {
        self.assert_type(node, SyntaxKind::KEYWORD)?;
        let mut children = node.children();
        let Some(node) = skip_trivia(&mut children) else {
            self.errors.push(ParserError::MissingToken {
                expected: SyntaxKind::LX_IDENT,
                range: range_to_span(node.text_range()),
            });
            return Err(Error {});
        };
        let r = match node.kind() {
            SyntaxKind::LX_IDENT => node.text().to_string(),
            SyntaxKind::LX_ESCAPED_IDENT => {
                // FIXME: unescape
                node.text().to_string()
            }
            _ => {
                self.errors.push(ParserError::UnexpectedSyntaxRule {
                    expected: &[SyntaxKind::LX_IDENT, SyntaxKind::LX_ESCAPED_IDENT],
                    found: node.kind(),
                    range: range_to_span(node.text_range()),
                });
                return Err(Error {});
            }
        };

        self.assert_end(children);
        Ok(r)
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

    fn parse_integer(&mut self, node: &SyntaxNode) -> Result<TagValue> {
        self.assert_type(node, SyntaxKind::INTEGER)?;
        dbg!(node);
        // self.assert_end(children);
        // let value = node.text().parse::<i64>().unwrap();
        Ok(TagValue::Int(42))
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

        let r = match children.next() {
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
                    range: range_to_span(c.text_range()),
                });
                Err(Error {})
            }
            None => Err(Error {}), // FIXME we need an error here
        };
        self.assert_end(children);
        r
    }

    fn parse_directive(&mut self, node: &SyntaxNode) -> Result<()> {
        self.assert_type(node, SyntaxKind::DIRECTIVE)?;
        dbg!(&node);
        let mut children = node.children();
        if let Some(c) = children.next() {
            let kw = self.kw(&c)?;
            assert_eq!(kw, "set");
        }
        if let Some(c) = children.next() {
            self.ident(&c)?;
        }

        if let Some(c) = children.next() {
            self.constant(&c)?;
        }
        self.assert_end(children);
        Ok(())
    }

    /// Lower the Syntax Tree into a [`Query`] ast
    pub fn lower(&mut self) -> Result<Query> {
        for child in self.root.children() {
            // We do not abort on an error this way we can keep parsing and potentially
            // collect multiple errors before returning.
            match child.kind() {
                SyntaxKind::DIRECTIVE => {
                    let _ = self.parse_directive(&child);
                }
                SyntaxKind::PARAM => {
                    let _ = self.parse_param(&child);
                }
                SyntaxKind::QUERY => {
                    let _ = self.parse_query(&child);
                }
                k if k.is_trivia() => {}
                k => {
                    self.errors.push(ParserError::UnexpectedSyntaxRule {
                        expected: &[SyntaxKind::DIRECTIVE, SyntaxKind::PARAM, SyntaxKind::QUERY],
                        found: k,
                        range: range_to_span(child.text_range()),
                    });
                }
            }
        }
        // FIXME: nope
        Err(Error {})
    }
}

fn range_to_span(range: TextRange) -> SourceSpan {
    let s: usize = range.start().into();
    let e: usize = range.end().into();
    SourceSpan::new(s.into(), e - s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_parse() -> Result<()> {
        let input = r"
            // test
            set a = 42;
            set b;
            a:b
            ";
        let mut parser = Parser::new(input);
        let query = parser.lower();
        dbg!(&parser.errors);
        assert!(parser.errors.is_empty());
        let _query = query?;
        Ok(())
    }
}
