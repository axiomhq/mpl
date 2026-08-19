//! Backtick escaping for MPL identifiers.
//!
//! Whether a name can be written bare is decided by lexing it: a name that
//! lexes as exactly one identifier token covering the whole string needs no
//! delimiters. Deriving it means the answer tracks the lexer instead of
//! restating its rules, so words that lex as something else — `true`, `inf` —
//! are escaped, and the leading `_` and non-ASCII letters the lexer accepts
//! are not.

use mpl_lang::lexer::{Lexer, TokenType};

/// Whether `name` has to be backtick-escaped to be read as an identifier.
#[must_use]
pub fn needs_escape(name: &str) -> bool {
    let mut tokens = Lexer::new(name).filter(|t| !t.is_eof());
    let Some(first) = tokens.next() else {
        return true;
    };
    !(first.tpe() == TokenType::Ident && first.text() == name && tokens.next().is_none())
}

/// `name` with the characters that would end an escaped identifier escaped,
/// and no surrounding delimiters.
fn escape_body(name: &str) -> String {
    name.replace('\\', "\\\\").replace('`', "\\`")
}

/// `name` written so it reads as an identifier, adding backticks only where
/// they are required.
#[must_use]
pub fn escape_ident(name: &str) -> String {
    if needs_escape(name) {
        format!("`{}`", escape_body(name))
    } else {
        name.to_string()
    }
}

/// The text a completion inserts for `name`.
///
/// `in_backtick` says the opening backtick is already in the document and the
/// replacement starts after it, so only the closing one is added — otherwise a
/// second opening backtick would be inserted.
#[must_use]
pub fn apply_text_for_ident(name: &str, in_backtick: bool) -> String {
    if in_backtick {
        format!("{}`", escape_body(name))
    } else {
        escape_ident(name)
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_text_for_ident, escape_ident, needs_escape};
    use test_case::test_case;

    #[test_case("cpu"          => false ; "plain lowercase")]
    #[test_case("myMetric"     => false ; "mixed case")]
    #[test_case("a1"           => false ; "trailing digit")]
    #[test_case("foo_bar"      => false ; "underscore inside")]
    #[test_case("_foo"         => false ; "leading underscore")]
    #[test_case("héllo"        => false ; "non-ascii letter")]
    #[test_case("metrixs-dev"  => true  ; "hyphen")]
    #[test_case("dev.metrics"  => true  ; "dot")]
    #[test_case("my app"       => true  ; "space")]
    #[test_case("1foo"         => true  ; "leading digit")]
    #[test_case(""             => true  ; "empty")]
    // These lex as their own token types, so bare they would never be read as
    // an identifier.
    #[test_case("true"         => true  ; "bool literal")]
    #[test_case("false"        => true  ; "false literal")]
    #[test_case("inf"          => true  ; "inf literal")]
    #[test_case("null"         => true  ; "null literal")]
    fn escaping_is_required(name: &str) -> bool {
        needs_escape(name)
    }

    #[test_case("cpu"         => "cpu"                ; "plain name is unchanged")]
    #[test_case("dev.metrics" => "`dev.metrics`"      ; "dotted name is wrapped")]
    #[test_case("has`tick"    => "`has\\`tick`"       ; "backtick in the name is escaped")]
    #[test_case("has\\slash"  => "`has\\\\slash`"     ; "backslash in the name is escaped")]
    fn escaping_a_name(name: &str) -> String {
        escape_ident(name)
    }

    #[test_case("cpu",         false => "cpu"              ; "bare plain name")]
    #[test_case("dev.metrics", false => "`dev.metrics`"    ; "bare name needing escape")]
    // With the opening backtick already typed, only the closing one is added —
    // adding both would leave the document with a double backtick.
    #[test_case("dev.metrics", true  => "dev.metrics`"     ; "in backtick, name needing escape")]
    #[test_case("cpu",         true  => "cpu`"             ; "in backtick, plain name closes anyway")]
    #[test_case("has`tick",    true  => "has\\`tick`"      ; "in backtick, backtick escaped")]
    fn apply_text(name: &str, in_backtick: bool) -> String {
        apply_text_for_ident(name, in_backtick)
    }

    /// What the editor ends up with: the document up to the replacement point
    /// plus the apply text.
    #[test_case("`axi", 1, "dev.metrics" => "`dev.metrics`" ; "opening backtick already typed")]
    #[test_case("axi",  0, "dev.metrics" => "`dev.metrics`" ; "no backtick typed")]
    fn replacing_in_a_document(doc: &str, from: usize, name: &str) -> String {
        let in_backtick = from > 0 && doc.as_bytes()[from - 1] == b'`';
        format!(
            "{}{}",
            &doc[..from],
            apply_text_for_ident(name, in_backtick)
        )
    }
}
