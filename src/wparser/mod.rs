//! The hand-written `winnow` parser for the MPL grammar.
//!
//! This is the production parser (it replaced the former `pest` grammar). It
//! delivers the four goals that motivated dropping `pest`: parsing, editor
//! highlighting, multi-error diagnostics, and a (future) formatter.
//!
//! Two layers:
//! - [`lex`] — a flat, trivia-preserving lexer that drives syntax
//!   highlighting and never fails (robust on incomplete input).
//! - [`grammar`] — structural combinators that build the real
//!   [`crate::query`] AST, with pipe-boundary error recovery so one bad clause
//!   does not sink the whole parse.

pub mod grammar;
pub mod lex;

pub use grammar::{ParseOutput, parse_file, parse_param_value};
pub use lex::highlight;

/// A highlight token classification. Trivia (`Whitespace`, `Comment`) and
/// `Unknown` are returned for losslessness but the editor renders only the
/// meaningful kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlKind {
    /// Identifiers, params (`$x`), backtick idents.
    Variable,
    /// `"…"` string literal.
    String,
    /// Numbers and relative times (`5m`).
    Number,
    /// `true` / `false`.
    Bool,
    /// `#/…/` regex literal.
    Regexp,
    /// Comparison operators.
    Operator,
    /// `|` pipe (and other structural punctuation).
    Punctuation,
    /// Reserved words.
    Keyword,
    /// Type names (`int`, `Duration`, `Option`, …).
    Type,
    /// `// …` line comment (trivia).
    Comment,
    /// Whitespace (trivia).
    Whitespace,
    /// Anything the lexer did not recognise (keeps the lexer total).
    Unknown,
}

/// A highlight token: a byte range (`start..end`, into the original source)
/// and its classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HlToken {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
    /// What kind of token this is.
    pub kind: HlKind,
}
