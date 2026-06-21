use super::highlight;
use crate::wparser::{HlKind, HlToken};

/// All non-trivia tokens, paired with their source text, for easy assertions.
fn meaningful<'s>(src: &'s str, tokens: &[HlToken]) -> Vec<(HlKind, &'s str)> {
    tokens
        .iter()
        .filter(|t| !matches!(t.kind, HlKind::Whitespace))
        .map(|t| (t.kind, &src[t.start..t.end]))
        .collect()
}

fn kind_of(src: &str, tokens: &[HlToken], text: &str) -> Option<HlKind> {
    tokens
        .iter()
        .find(|t| &src[t.start..t.end] == text)
        .map(|t| t.kind)
}

/// Property: the lexer is *total* — its tokens cover the input exactly, with no
/// gaps or overlaps, sorted ascending. This is what makes it safe for an editor
/// to consume on every keystroke.
fn assert_covers(src: &str, tokens: &[HlToken]) {
    let mut pos = 0;
    for t in tokens {
        assert_eq!(t.start, pos, "gap/overlap before {t:?} in {src:?}");
        assert!(t.end > t.start, "empty token {t:?}");
        pos = t.end;
    }
    assert_eq!(pos, src.len(), "tokens do not reach end of {src:?}");
}

#[test]
fn classifies_a_full_filter_query() {
    let src = r#"ds:metric | filter region == "us""#;
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(
        meaningful(src, &tokens),
        vec![
            (HlKind::Variable, "ds"),
            (HlKind::Punctuation, ":"),
            (HlKind::Variable, "metric"),
            (HlKind::Punctuation, "|"),
            (HlKind::Keyword, "filter"),
            (HlKind::Variable, "region"),
            (HlKind::Operator, "=="),
            (HlKind::String, "\"us\""),
        ]
    );
}

#[test]
fn classifies_align_with_duration_and_type() {
    let src = "ds:metric | align to 5m using avg";
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "align"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "to"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "5m"), Some(HlKind::Number));
    assert_eq!(kind_of(src, &tokens, "using"), Some(HlKind::Keyword));
}

#[test]
fn classifies_param_declaration() {
    let src = "param $dur: Duration;\nds:metric";
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "param"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "$dur"), Some(HlKind::Variable));
    assert_eq!(kind_of(src, &tokens, "Duration"), Some(HlKind::Type));
}

#[test]
fn classifies_optional_type() {
    let src = "param $f: Option<string>;\nds:metric";
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "Option"), Some(HlKind::Type));
    assert_eq!(kind_of(src, &tokens, "string"), Some(HlKind::Type));
}

#[test]
fn comments_are_tokens() {
    let src = "// a header\nds:metric";
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "// a header"), Some(HlKind::Comment));
}

#[test]
fn regex_and_bool_and_is_keyword() {
    let src = "ds:metric | filter tag == #/ab|c/ and ok is bool or v == true";
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "#/ab|c/"), Some(HlKind::Regexp));
    assert_eq!(kind_of(src, &tokens, "is"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "and"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "or"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "bool"), Some(HlKind::Type));
    assert_eq!(kind_of(src, &tokens, "true"), Some(HlKind::Bool));
}

// ── the headline: highlighting still works on incomplete / mid-edit input ──

#[test]
fn incomplete_filter_rhs_still_tokenizes() {
    // No RHS yet, no closing — the structural parser would reject this.
    let src = "metric:cpu | filter region == ";
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "filter"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "region"), Some(HlKind::Variable));
    assert_eq!(kind_of(src, &tokens, "=="), Some(HlKind::Operator));
}

#[test]
fn incomplete_align_using_still_tokenizes() {
    let src = "metric:cpu | align using ";
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "align"), Some(HlKind::Keyword));
    assert_eq!(kind_of(src, &tokens, "using"), Some(HlKind::Keyword));
}

#[test]
fn unterminated_string_still_tokenizes() {
    let src = r#"ds:metric | filter tag == "half"#;
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    // The unterminated literal is still a single String token to EOF.
    assert_eq!(kind_of(src, &tokens, "\"half"), Some(HlKind::String));
}

#[test]
fn string_interpolation_descends_into_braces() {
    // `"host ${ $h } end"` splits into String / Variable / String: the embedded
    // `$h` is its own token; the `${`/`}` delimiters are trivia (filtered out of
    // the meaningful stream).
    let src = r#""host ${ $h } end""#;
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(
        meaningful(src, &tokens),
        vec![
            (HlKind::String, "\"host "),
            (HlKind::Variable, "$h"),
            (HlKind::String, " end\""),
        ]
    );
}

#[test]
fn string_interpolation_embeds_number() {
    // A numeric expression inside `${ … }` is classified as a Number, proving
    // the embedded expr is re-lexed with the top-level classifier.
    let src = r#""n=${ 42 }""#;
    let tokens = highlight(src);
    assert_covers(src, &tokens);
    assert_eq!(kind_of(src, &tokens, "42"), Some(HlKind::Number));
    assert_eq!(kind_of(src, &tokens, "\"n="), Some(HlKind::String));
}

#[test]
fn unterminated_interpolation_never_panics() {
    // Mid-edit input: open literal, open interpolation, no closers.
    for src in [r#""a ${ $b"#, r#""a ${"#, r#""${x}${y}"#, r#""${}""#] {
        let tokens = highlight(src);
        assert_covers(src, &tokens);
    }
}

#[test]
fn garbage_never_panics_and_covers_input() {
    for src in ["", "{{{}}}", "ds:", "|||", "@@@@", "#", "$", "param $"] {
        let tokens = highlight(src);
        assert_covers(src, &tokens);
    }
}
