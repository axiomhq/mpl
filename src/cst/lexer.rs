//! Hand-written lexer for `MPL` — the single home for **every** token rule.
//!
//! There is no lexer generator, regex crate or second grammar here: each token
//! is recognised by an explicit `match` on the next byte(s). The lexer is
//! deliberately *modal*, and the modes are made of plain Rust:
//!
//! * **top-level mode** — [`scan_token`] reads one token from a position in the
//!   source. It owns the only tricky maximal-munch / ordering decisions
//!   (`RFC3339` before `FLOAT` before `INT`; `#/…/` and `#s/…/…/` regex vs the
//!   bare `/` slash; `::` before `:`, `==` before `=`, …).
//! * **string mode** — [`expand_string`] scans the literal text of a `"…"`
//!   string, emitting [`SyntaxKind::STRING_FRAGMENT`] runs and recognising the
//!   `${` that opens an interpolation (and the `\$` escape that does *not*).
//! * **interpolation mode** — [`lex_interp`] lexes a `${ … }` interior with the
//!   *same* top-level [`scan_token`], finding the closing `}` by **counting
//!   brace tokens**. Because the interior is lexed token-by-token, a backtick
//!   ident (`` `a}b` ``), a `#/regex/` or a `// comment` is a single token, so a
//!   `}`/`"` inside one of them is part of that token and can never be miscounted
//!   as a delimiter — the boundary bug a naive byte scanner would have.
//!
//! The "mode stack" is literally the Rust call stack: a nested string inside an
//! interpolation recurses `expand_string → lex_interp → expand_string`, so
//! `"${ "x ${ y }" }"` structures fully. An unterminated string / interpolation
//! (no closing `"`/`}` — the mid-edit case) is detected when a mode reaches EOF;
//! its start offset is recorded so [`super::parser`] can diagnose it while
//! still keeping the interior structured for highlighting.
//!
//! The lexer never drops a byte: whitespace and comments are real trivia
//! tokens and anything it cannot classify becomes a one-char
//! [`SyntaxKind::ERROR`] token. That losslessness is what the `rowan` CST and a
//! future formatter rely on.

use std::ops::Range;

use super::SyntaxKind;

/// Lex `input` into the flat token stream the parser walks, expanding string
/// literals into their interpolation pieces. Returns the tokens plus the start
/// offsets of any *unterminated* strings (used to diagnose mid-edit input).
pub(super) fn lex(input: &str) -> (Vec<(SyntaxKind, Range<usize>)>, Vec<usize>) {
    let mut tokens = Vec::new();
    let mut unterminated = Vec::new();
    let bytes = input.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        // A `"` switches into string mode; everything else is one top-level
        // token. (The string machinery owns boundary detection so a `"` inside
        // a nested construct is never mistaken for a delimiter.)
        if bytes[pos] == b'"' {
            pos = expand_string(input, pos, &mut tokens, &mut unterminated);
        } else {
            let (kind, end) = scan_token(input, pos);
            tokens.push((kind, pos..end));
            pos = end;
        }
    }
    (tokens, unterminated)
}

/// Tokenize `input` into **raw** non-trivia tokens, leaving a `"…"` literal as a
/// single [`SyntaxKind::STRING`] token (no interpolation descent).
///
/// Used by the `param_value` external entry point, which only inspects the
/// leading literal tokens of a caller-supplied runtime value and wants the
/// whole string as one token to unescape.
pub(super) fn tokenize_raw(input: &str) -> Vec<(SyntaxKind, &str)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        let (kind, end) = scan_token(input, pos);
        if !kind.is_trivia() {
            out.push((kind, &input[pos..end]));
        }
        pos = end;
    }
    out
}

/// Read **one** top-level token starting at `start`, returning its kind and the
/// byte offset one past its end. This is the whole token grammar.
fn scan_token(input: &str, start: usize) -> (SyntaxKind, usize) {
    let bytes = input.as_bytes();
    match bytes[start] {
        b' ' | b'\t' | b'\r' | b'\n' => {
            let mut i = start + 1;
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
                i += 1;
            }
            (SyntaxKind::WHITESPACE, i)
        }
        // `//` comment to end of line; a lone `/` is the slash operator.
        b'/' if bytes.get(start + 1) == Some(&b'/') => {
            let mut i = start + 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            (SyntaxKind::COMMENT, i)
        }
        b'/' => (SyntaxKind::SLASH, start + 1),
        // `#/…/` regex and `#s/…/…/` regex-replace. `#` never starts anything
        // else, which is what disambiguates a regex literal from division.
        b'#' => scan_regex(input, start),
        // A raw string literal (used by `tokenize_raw`; `lex`/`lex_interp`
        // intercept `"` before reaching here and descend instead).
        b'"' => scan_raw_string(input, start),
        b'`' => scan_escaped_ident(input, start),
        b'$' => scan_param(input, start),
        b'0'..=b'9' => scan_number(input, start),
        b if is_ident_start(b) => {
            let mut i = start + 1;
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            (SyntaxKind::IDENT, i)
        }
        // Multi-char operators (maximal munch): the two-char form is tried
        // before the one-char form.
        b':' if bytes.get(start + 1) == Some(&b':') => (SyntaxKind::COLON_COLON, start + 2),
        b':' => (SyntaxKind::COLON, start + 1),
        b'.' if bytes.get(start + 1) == Some(&b'.') => (SyntaxKind::DOT_DOT, start + 2),
        b'=' if bytes.get(start + 1) == Some(&b'=') => (SyntaxKind::EQ_EQ, start + 2),
        b'=' => (SyntaxKind::EQ, start + 1),
        b'!' if bytes.get(start + 1) == Some(&b'=') => (SyntaxKind::BANG_EQ, start + 2),
        b'<' if bytes.get(start + 1) == Some(&b'=') => (SyntaxKind::LT_EQ, start + 2),
        b'<' => (SyntaxKind::L_ANGLE, start + 1),
        b'>' if bytes.get(start + 1) == Some(&b'=') => (SyntaxKind::GT_EQ, start + 2),
        b'>' => (SyntaxKind::R_ANGLE, start + 1),
        b'|' => (SyntaxKind::PIPE, start + 1),
        b';' => (SyntaxKind::SEMICOLON, start + 1),
        b',' => (SyntaxKind::COMMA, start + 1),
        b'[' => (SyntaxKind::L_BRACK, start + 1),
        b']' => (SyntaxKind::R_BRACK, start + 1),
        b'{' => (SyntaxKind::L_BRACE, start + 1),
        b'}' => (SyntaxKind::R_BRACE, start + 1),
        b'(' => (SyntaxKind::L_PAREN, start + 1),
        b')' => (SyntaxKind::R_PAREN, start + 1),
        b'~' => (SyntaxKind::TILDE, start + 1),
        b'+' => (SyntaxKind::PLUS, start + 1),
        b'-' => (SyntaxKind::MINUS, start + 1),
        b'*' => (SyntaxKind::STAR, start + 1),
        // Anything else (a lone `.`/`!`, `@`, `%`, …) is an unclassifiable byte:
        // emit a one-char ERROR token so every byte stays in the tree.
        _ => (SyntaxKind::ERROR, start + char_len(input, start)),
    }
}

/// `[0-9]…`: an `RFC3339` timestamp (longest match), a `FLOAT`, or an `INT`.
fn scan_number(input: &str, start: usize) -> (SyntaxKind, usize) {
    // RFC3339 is the longest possible match for a leading digit run, so it is
    // tried first (longest-match-wins, the ordering a generated lexer encodes
    // by token priority).
    if let Some(end) = match_rfc3339(input.as_bytes(), start) {
        return (SyntaxKind::RFC3339, end);
    }
    let bytes = input.as_bytes();
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    // A float needs `.` followed by at least one digit, so a trailing `..`
    // (range operator) after an integer timestamp is *not* swallowed.
    if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        i = scan_exponent(bytes, i);
        return (SyntaxKind::FLOAT, i);
    }
    (SyntaxKind::INT, i)
}

/// Consume a `[eE][+-]?[0-9]+` exponent at `i` if one is fully present; else
/// return `i` unchanged (a dangling `e` is a separate identifier).
fn scan_exponent(bytes: &[u8], i: usize) -> usize {
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if bytes.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
            while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            return j;
        }
    }
    i
}

/// Match `[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z?` at `start`,
/// returning the end offset of the match (`None` if it does not match).
fn match_rfc3339(bytes: &[u8], start: usize) -> Option<usize> {
    let digits =
        |i: usize, n: usize| (0..n).all(|k| bytes.get(i + k).is_some_and(u8::is_ascii_digit));
    let lit = |i: usize, c: u8| bytes.get(i) == Some(&c);

    if !digits(start, 4) {
        return None;
    }
    let mut i = start + 4;
    for (sep, group) in [(b'-', 2), (b'-', 2), (b'T', 2), (b':', 2), (b':', 2)] {
        if !lit(i, sep) {
            return None;
        }
        i += 1;
        if !digits(i, group) {
            return None;
        }
        i += group;
    }
    if lit(i, b'Z') {
        i += 1;
    }
    Some(i)
}

/// `#/…/` regex or `#s/…/…/` regex-replace. A malformed `#` (unterminated body
/// or a bare `#`) becomes a one-char `ERROR`.
fn scan_regex(input: &str, start: usize) -> (SyntaxKind, usize) {
    let bytes = input.as_bytes();
    if bytes.get(start + 1) == Some(&b's') && bytes.get(start + 2) == Some(&b'/') {
        if let Some(end) = scan_regex_body(input, start + 3).and_then(|i| scan_regex_body(input, i))
        {
            return (SyntaxKind::REGEX_REPLACE, end);
        }
    } else if bytes.get(start + 1) == Some(&b'/')
        && let Some(end) = scan_regex_body(input, start + 2)
    {
        return (SyntaxKind::REGEX, end);
    }
    (SyntaxKind::ERROR, start + char_len(input, start))
}

/// Scan a regex body `([^/\\]|\\.)*/`, returning the offset past the closing
/// `/` (`None` if unterminated).
fn scan_regex_body(input: &str, mut i: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = skip_escape(input, i),
            b'/' => return Some(i + 1),
            _ => i += char_len(input, i),
        }
    }
    None
}

/// Scan `"([^"\\]|\\.)*"`, returning `STRING` past the closing quote, or `ERROR`
/// to EOF if the string is unterminated. Only used by [`tokenize_raw`]; `lex`
/// and `lex_interp` descend via [`expand_string`] instead.
fn scan_raw_string(input: &str, start: usize) -> (SyntaxKind, usize) {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = skip_escape(input, i),
            b'"' => return (SyntaxKind::STRING, i + 1),
            _ => i += char_len(input, i),
        }
    }
    (SyntaxKind::ERROR, bytes.len())
}

/// Scan a backtick-escaped identifier: a backtick, a run of non-backtick /
/// escaped chars, then a closing backtick. An *unterminated* backtick (no
/// closing one before EOF) becomes a single `ERROR` token running to EOF — the
/// body matches everything but a backtick, so there is no earlier boundary, and
/// downstream editor features (`is_word_token`, source extraction) rely on the
/// whole still-open run being one backtick-led `ERROR` token mid-edit.
fn scan_escaped_ident(input: &str, start: usize) -> (SyntaxKind, usize) {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = skip_escape(input, i),
            b'`' => return (SyntaxKind::ESCAPED_IDENT, i + 1),
            _ => i += char_len(input, i),
        }
    }
    (SyntaxKind::ERROR, input.len())
}

/// `$ident` or `` $`escaped` `` parameter identifier. A bare `$` (or `$` + an
/// unterminated backtick) is a one-char `ERROR`.
fn scan_param(input: &str, start: usize) -> (SyntaxKind, usize) {
    let bytes = input.as_bytes();
    match bytes.get(start + 1) {
        Some(&b) if is_ident_start(b) => {
            let mut i = start + 2;
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            (SyntaxKind::PARAM_IDENT, i)
        }
        Some(&b'`') => match scan_escaped_ident(input, start + 1) {
            (SyntaxKind::ESCAPED_IDENT, end) => (SyntaxKind::PARAM_IDENT, end),
            _ => (SyntaxKind::ERROR, start + 1),
        },
        _ => (SyntaxKind::ERROR, start + 1),
    }
}

/// Split the string literal beginning at `start` (its opening `"`) into the flat
/// `STRING_FRAGMENT` / `DOLLAR_BRACE` / interior / `R_BRACE` token sequence the
/// parser shapes into a `STRING` node, returning the byte offset one past the
/// closing `"` (or `input.len()` if the string is unterminated, in which case
/// `start` is pushed to `unterminated`).
///
/// Boundary fragments keep their `"` quotes; an escaped `\$` never starts an
/// interpolation. Each `${ … }` interior is delegated to [`lex_interp`], so the
/// closing `}` is found by token counting, not byte scanning.
fn expand_string(
    input: &str,
    start: usize,
    tokens: &mut Vec<(SyntaxKind, Range<usize>)>,
    unterminated: &mut Vec<usize>,
) -> usize {
    let bytes = input.as_bytes();
    let len = input.len();
    let mut frag_start = start; // first fragment includes the opening quote
    let mut i = start + 1;
    while i < len {
        match bytes[i] {
            // Escaped char (incl. `\$`) is literal text.
            b'\\' => i = skip_escape(input, i),
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                if i > frag_start {
                    tokens.push((SyntaxKind::STRING_FRAGMENT, frag_start..i));
                }
                tokens.push((SyntaxKind::DOLLAR_BRACE, i..i + 2));
                if let Some(close) = lex_interp(input, i + 2, tokens, unterminated) {
                    tokens.push((SyntaxKind::R_BRACE, close..close + 1));
                    i = close + 1;
                    frag_start = i;
                } else {
                    // Reached EOF still inside the interpolation: its tokens are
                    // already emitted; the whole string is unterminated.
                    unterminated.push(start);
                    return len;
                }
            }
            // Closing quote: it belongs to the trailing fragment.
            b'"' => {
                tokens.push((SyntaxKind::STRING_FRAGMENT, frag_start..i + 1));
                return i + 1;
            }
            _ => i += char_len(input, i),
        }
    }
    // Reached EOF without a closing quote: emit the trailing fragment and flag
    // the string as unterminated (the node still extends to EOF).
    if len > frag_start {
        tokens.push((SyntaxKind::STRING_FRAGMENT, frag_start..len));
    }
    unterminated.push(start);
    len
}

/// Lex a `${ … }` interpolation interior beginning at `expr_start` with the
/// top-level [`scan_token`], emitting its tokens and returning the byte offset
/// of the `}` that closes it at brace depth 0 (`None` if EOF is reached first —
/// an unterminated interpolation). The closing `}` is *not* emitted here; the
/// caller emits it as an `R_BRACE`.
///
/// Token counting is what makes the boundary robust: a backtick ident, a
/// `#/regex/` or a `// comment` is a single token, so a `}`/`"` inside it is
/// never a delimiter. A nested string literal is descended into via
/// [`expand_string`].
fn lex_interp(
    input: &str,
    expr_start: usize,
    tokens: &mut Vec<(SyntaxKind, Range<usize>)>,
    unterminated: &mut Vec<usize>,
) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut pos = expr_start;
    let mut depth: u32 = 0;
    while pos < bytes.len() {
        match bytes[pos] {
            b'}' if depth == 0 => return Some(pos),
            b'}' => {
                depth -= 1;
                tokens.push((SyntaxKind::R_BRACE, pos..pos + 1));
                pos += 1;
            }
            b'{' => {
                depth += 1;
                tokens.push((SyntaxKind::L_BRACE, pos..pos + 1));
                pos += 1;
            }
            // Nested string: descend so its `"`/`}` (and any further `${ … }`)
            // are handled by the string machinery.
            b'"' => pos = expand_string(input, pos, tokens, unterminated),
            _ => {
                let (kind, end) = scan_token(input, pos);
                tokens.push((kind, pos..end));
                pos = end;
            }
        }
    }
    None
}

// ── byte helpers ─────────────────────────────────────────────────

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Skip a backslash escape at `i` (the `\`), consuming the `\` and the escaped
/// char (whole UTF-8 char); clamps at EOF for a trailing `\`.
fn skip_escape(input: &str, i: usize) -> usize {
    let after_backslash = i + 1;
    if after_backslash < input.len() {
        after_backslash + char_len(input, after_backslash)
    } else {
        after_backslash
    }
}

/// Byte length of the UTF-8 char starting at `input[i]`.
fn char_len(input: &str, i: usize) -> usize {
    input[i..].chars().next().map_or(1, char::len_utf8)
}
