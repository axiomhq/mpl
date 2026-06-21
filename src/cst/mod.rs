//! Lossless concrete-syntax-tree (CST) front end for `MPL`.
//!
//! This is the `rowan` (red/green) implementation of the **whole** grammar —
//! the sole front end now that `pest` is retired. Compared with the old PEG it:
//!
//! * keeps **every** byte of the source — comments and whitespace become
//!   real tokens in the tree, which is the prerequisite for a formatter and
//!   for editor features that need trivia;
//! * never fully fails: a hand-written recursive-descent [`parser`] inserts
//!   [`SyntaxKind::ERROR_NODE`] subtrees on unexpected input, so highlighting
//!   keeps working on incomplete / mid-edit queries;
//! * is lowered to the existing [`crate::query`] AST by a thin [`lower`] pass
//!   (which also hosts the `param_value` external entry point).
//!
//! ## Modal lexer
//!
//! String/interpolation lexing is **fully declarative** in `logos`. The lexer
//! runs in two modes that share one [`LexState`] (`logos` `Extras`):
//!
//! * normal mode — the [`SyntaxKind`] `logos` lexer (MPL code);
//! * string mode — a second `#[derive(Logos)]` enum ([`parser::StrToken`]) for
//!   literal text/escapes, `${`, a lone `$` and the closing `"`.
//!
//! A `"` switches normal → string via [`logos::Lexer::morph`]; a `${` switches
//! string → normal (the embedded expression is lexed by the *same* `SyntaxKind`
//! lexer) and pushes an interpolation frame onto [`LexState::interp`]; the `}`
//! that closes it (at brace-depth 0, tracked by the [`brace_open`]/[`brace_close`]
//! callbacks) switches back to string mode. There is **no** hand-written byte
//! scanning of string bodies — see [`parser::lex`].

use logos::Logos;

pub mod lower;
mod parser;

#[cfg(test)]
mod tests;

pub use parser::{Parse, SyntaxError, parse};

/// Lexer state threaded across [`logos::Lexer::morph`] (the `logos` `Extras`).
///
/// MPL strings interpolate `${ expr }`, and the embedded expression is lexed by
/// the normal [`SyntaxKind`] lexer — so a `}` is ambiguous: it can close an
/// interpolation, an `ifdef` body, or a brace nested inside the expression.
/// `interp` records, for each open `${ … }` (innermost last), the `{ }` nesting
/// depth seen so far. The [`brace_open`]/[`brace_close`] token callbacks keep it
/// current; `closed_interp` is set by [`brace_close`] when a `}` brought the
/// innermost frame's depth below 0, i.e. it closed an interpolation — the one
/// signal the [`parser::lex`] driver needs to morph back into string mode.
#[derive(Debug, Default, Clone)]
pub struct LexState {
    interp: Vec<u32>,
    closed_interp: bool,
}

/// `{` callback: a brace inside an open interpolation deepens its nesting.
fn brace_open(lex: &mut logos::Lexer<SyntaxKind>) {
    if let Some(depth) = lex.extras.interp.last_mut() {
        *depth += 1;
    }
}

/// `}` callback: pop the innermost interpolation frame when its depth is 0
/// (this `}` closes it) and flag it; otherwise unwind one level of nesting.
fn brace_close(lex: &mut logos::Lexer<SyntaxKind>) {
    match lex.extras.interp.last().copied() {
        Some(0) => {
            lex.extras.interp.pop();
            lex.extras.closed_interp = true;
        }
        Some(_) => {
            if let Some(depth) = lex.extras.interp.last_mut() {
                *depth -= 1;
            }
            lex.extras.closed_interp = false;
        }
        None => lex.extras.closed_interp = false,
    }
}

/// The kinds of tokens and nodes in the `MPL` syntax tree.
///
/// Variants carrying a `#[token]` / `#[regex]` attribute are produced by the
/// [`logos`] lexer. The remaining "token" variants ([`SyntaxKind::KEYWORD`] …
/// [`SyntaxKind::TIME_UNIT`]) are *semantic relabelings* the parser assigns
/// while building the tree (e.g. an `IDENT` used as the `filter` keyword is
/// emitted as `KEYWORD`). The `*_NODE` / composite variants are interior
/// nodes. Screaming-case mirrors the rust-analyzer convention.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[logos(extras = LexState)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // ── trivia (lexer) ───────────────────────────────────────────
    /// Run of spaces, tabs, carriage returns and newlines.
    #[regex(r"[ \t\r\n]+")]
    WHITESPACE,
    /// `// …` line comment.
    #[regex(r"//[^\n]*", allow_greedy = true)]
    COMMENT,

    // ── literals & identifiers (lexer) ───────────────────────────
    /// RFC3339 timestamp literal, e.g. `2025-03-01T13:00:00Z`. Lexed ahead of
    /// `INT`/`FLOAT` because it is the longest match for a leading digit run.
    #[regex(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z?")]
    RFC3339,
    /// Floating point literal, e.g. `3.14`. The fractional part requires at
    /// least one digit so a trailing `..` (range operator) after an integer
    /// timestamp is not swallowed as a float.
    #[regex(r"[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?")]
    FLOAT,
    /// Integer literal, e.g. `42`.
    #[regex(r"[0-9]+")]
    INT,
    // A `"` opens a string. Unlike a classic full-string regex, the lexer only
    // matches the opening quote here and then *morphs* into string mode (see
    // [`parser::StrToken`] / [`parser::lex`]): the body is lexed declaratively,
    // and `${ … }` interpolations morph back to this lexer for the embedded
    // expression. The raw quote token never reaches the tree — the driver folds
    // it into the leading [`SyntaxKind::STRING_FRAGMENT`]. `STRING` is reused as
    // the *node* kind wrapping the fragments / delimiters / embedded `EXPR`, so
    // the CST stays lossless down into interpolations.
    /// Opening `"` of a string (morph trigger); reused as the string node kind.
    #[token("\"")]
    STRING,
    /// A run of literal string text inside a [`SyntaxKind::STRING`] node. The
    /// boundary fragments carry the surrounding `"` quotes. Driver-emitted.
    STRING_FRAGMENT,
    /// The `${` opening an interpolation inside a string. Driver-emitted.
    DOLLAR_BRACE,
    /// Regex literal `#/…/`.
    #[regex(r"#/([^/\\]|\\.)*/")]
    REGEX,
    /// Regex-replace literal `#s/…/…/`.
    #[regex(r"#s/([^/\\]|\\.)*/([^/\\]|\\.)*/")]
    REGEX_REPLACE,
    /// Parameter identifier, e.g. `$dur` or `` $`weird name` ``.
    #[regex(r#"\$([A-Za-z_][A-Za-z0-9_]*|`([^`\\]|\\.)*`)"#)]
    PARAM_IDENT,
    /// Backtick-escaped identifier, e.g. `` `my-tag` ``.
    #[regex(r"`([^`\\]|\\.)*`")]
    ESCAPED_IDENT,
    /// Plain identifier, e.g. `cpu`.
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    IDENT,

    // ── punctuation & operators (lexer) ──────────────────────────
    /// `|` pipe.
    #[token("|")]
    PIPE,
    /// `::` module separator.
    #[token("::")]
    COLON_COLON,
    /// `:` colon.
    #[token(":")]
    COLON,
    /// `;` semicolon.
    #[token(";")]
    SEMICOLON,
    /// `,` comma.
    #[token(",")]
    COMMA,
    /// `[` left bracket.
    #[token("[")]
    L_BRACK,
    /// `]` right bracket.
    #[token("]")]
    R_BRACK,
    /// `{` left brace (ifdef bodies / interpolation nesting).
    #[token("{", brace_open)]
    L_BRACE,
    /// `}` right brace (ifdef bodies / interpolation close).
    #[token("}", brace_close)]
    R_BRACE,
    /// `(` left parenthesis.
    #[token("(")]
    L_PAREN,
    /// `)` right parenthesis.
    #[token(")")]
    R_PAREN,
    /// `..` range separator.
    #[token("..")]
    DOT_DOT,
    /// `==` equality.
    #[token("==")]
    EQ_EQ,
    /// `!=` inequality.
    #[token("!=")]
    BANG_EQ,
    /// `<=` less-or-equal.
    #[token("<=")]
    LT_EQ,
    /// `>=` greater-or-equal.
    #[token(">=")]
    GT_EQ,
    /// `<` less-than / `Option<` opener.
    #[token("<")]
    L_ANGLE,
    /// `>` greater-than / `Option<…>` closer.
    #[token(">")]
    R_ANGLE,
    /// `=` assignment (directives, `extend`).
    #[token("=")]
    EQ,
    /// `~` replace operator.
    #[token("~")]
    TILDE,
    /// `+` plus.
    #[token("+")]
    PLUS,
    /// `-` minus.
    #[token("-")]
    MINUS,
    /// `*` star.
    #[token("*")]
    STAR,
    /// `/` slash.
    #[token("/")]
    SLASH,

    // ── semantic relabelings (assigned by the parser) ────────────
    /// An identifier consumed as a keyword (`filter`, `align`, `using`, …).
    KEYWORD,
    /// An identifier consumed as a type name (`string`, `Duration`, `Option`).
    TYPE_NAME,
    /// `true` / `false`.
    BOOL_LIT,
    /// `inf`.
    INF_LIT,
    /// A comparison operator inside a filter (`==`, `!=`, `<`, …).
    CMP_OP,
    /// The unit suffix of a relative time (`m`, `h`, `ms`, …).
    TIME_UNIT,
    /// A byte run the lexer could not classify.
    ERROR,

    // ── interior nodes ───────────────────────────────────────────
    /// Whole-file root node.
    ROOT,
    /// `set …;` directive.
    DIRECTIVE,
    /// `param $n : T;` declaration.
    PARAM_DECL,
    /// The type portion of a `param` declaration.
    PARAM_TYPE,
    /// A simple query body (`source pipe*`).
    QUERY,
    /// A compute query body (`( query , query ) | compute …`).
    COMPUTE_QUERY,
    /// `| compute <name> using <fn>`.
    COMPUTE_RULE,
    /// `metric_id time_range? as?`.
    SOURCE,
    /// `dataset : metric`.
    METRIC_ID,
    /// The dataset side of a metric id.
    DATASET,
    /// The metric-name side of a metric id.
    METRIC_NAME,
    /// `[ rel .. rel? ]`.
    TIME_RANGE,
    /// A relative time (`5m`) or `$param` time.
    REL_TIME,
    /// `as <name>`.
    AS_CLAUSE,
    /// `| filter …` / `| where …`.
    FILTER_RULE,
    /// `| align …`.
    ALIGN_RULE,
    /// `| map …`.
    MAP_RULE,
    /// `| group …`.
    GROUP_RULE,
    /// `| bucket …`.
    BUCKET_RULE,
    /// `| join …` (parsed, lowered to `NotSupported`).
    JOIN_RULE,
    /// `| replace …` (parsed, lowered to `NotSupported`).
    REPLACE_RULE,
    /// `| ifdef(...) { … } else { … }`.
    IFDEF_RULE,
    /// `| sample <n>`.
    SAMPLE_RULE,
    /// `| extend <tag> = <expr>, …`.
    EXTEND_RULE,
    /// A single `<tag> = <expr>` inside an extend rule.
    EXTEND_EXPR,
    /// A comma-separated list of tag idents.
    TAGS,
    /// A numeric literal (optionally signed / `inf`).
    NUMBER,
    /// A bucket function call, e.g. `histogram(max)`.
    BUCKET_FN,
    /// A single bucket spec (`count`, `avg`, a percentile, …).
    BUCKET_SPEC,
    /// A unix-timestamp time (`1747077736092`).
    TIME_TIMESTAMP,
    /// An RFC3339 time (`2025-03-01T13:00:00Z`).
    TIME_RFC3339,
    /// A `+`/`-` modifier time (`+1h`).
    TIME_MODIFIER,
    /// `filter_and ( or filter_and )*`.
    FILTER_OR,
    /// `filter_not ( and filter_not )*`.
    FILTER_AND,
    /// `not? filter_clause`.
    FILTER_NOT,
    /// `filter_atom | ( filter_or )`.
    FILTER_CLAUSE,
    /// `tag ( value | regex | is )`.
    FILTER_ATOM,
    /// `cmp expr`.
    VALUE_FILTER,
    /// `cmp_re regex`.
    REGEX_FILTER,
    /// `is tag_type`.
    IS_FILTER,
    /// A filter / extend value expression.
    EXPR,
    /// A (possibly module-qualified) function reference.
    FUNC,
    /// A recovery subtree wrapping unexpected tokens.
    ERROR_NODE,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

impl SyntaxKind {
    /// Is this a trivia token (whitespace or comment)?
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }

    fn from_raw(raw: rowan::SyntaxKind) -> Self {
        assert!(
            raw.0 <= SyntaxKind::ERROR_NODE as u16,
            "raw syntax kind out of range"
        );
        // SAFETY: `SyntaxKind` is `#[repr(u16)]` with contiguous discriminants
        // `0..=ERROR_NODE`; the bounds check above guarantees `raw.0` names a
        // valid variant. rowan only ever round-trips kinds we produced.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }
}

/// The `MPL` language marker for `rowan`'s typed tree API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MplLanguage {}

impl rowan::Language for MplLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        SyntaxKind::from_raw(raw)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// A red-tree node over [`MplLanguage`].
pub type SyntaxNode = rowan::SyntaxNode<MplLanguage>;
/// A red-tree token over [`MplLanguage`].
pub type SyntaxToken = rowan::SyntaxToken<MplLanguage>;
/// A red-tree element (node or token) over [`MplLanguage`].
pub type SyntaxElement = rowan::SyntaxElement<MplLanguage>;

/// Classifies a bare identifier as a keyword / type / literal when it appears
/// outside a recognised slice construct (i.e. inside an [`SyntaxKind::ERROR_NODE`]).
///
/// The parser already relabels keywords it understands; this is the fallback
/// used by the editor highlighter so that *out-of-slice* constructs (`map`,
/// `group`, `bucket`, …) still light up. It is the single Rust source of truth
/// for the keyword list that the editor previously duplicated as a JS regex.
#[must_use]
pub fn keyword_syntax_kind(text: &str) -> Option<SyntaxKind> {
    Some(match text {
        "true" | "false" => SyntaxKind::BOOL_LIT,
        "inf" => SyntaxKind::INF_LIT,
        "string" | "int" | "float" | "bool" | "Duration" | "duration" | "Dataset" | "Regex"
        | "Option" | "Metric" => SyntaxKind::TYPE_NAME,
        "filter"
        | "where"
        | "map"
        | "group"
        | "by"
        | "using"
        | "align"
        | "to"
        | "over"
        | "from"
        | "bucket"
        | "join"
        | "compute"
        | "set"
        | "replace"
        | "as"
        | "extend"
        | "and"
        | "or"
        | "not"
        | "is"
        | "param"
        | "ifdef"
        | "else"
        | "sample"
        | "rate"
        | "increase"
        | "histogram"
        | "interpolate_delta_histogram"
        | "interpolate_cumulative_histogram"
        | "count"
        | "avg"
        | "sum"
        | "min"
        | "max" => SyntaxKind::KEYWORD,
        _ => return None,
    })
}
