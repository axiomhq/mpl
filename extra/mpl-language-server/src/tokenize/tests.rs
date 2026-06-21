use super::{TokenType, collect_tokens};

// The chumsky-lexer-backed `collect_tokens` always returns a `Vec` (never
// `None`): highlighting must survive incomplete / mid-edit input. These tests
// pin both the happy-path classification and that resilience.

fn text<'a>(q: &'a str, t: &super::Token) -> &'a str {
    &q[t.span.from..t.span.to]
}

// ── Variable tokens ──────────────────────────────────────────────

#[test]
fn variable_plain_source() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query);
    assert_eq!(tokens[0].kind, TokenType::Variable);
    assert_eq!(text(query, &tokens[0]), "ds");
}

#[test]
fn variable_escaped_ident() {
    let query = r#"ds:metric | filter `my-tag` == "x""#;
    let tokens = collect_tokens(query);
    let tag = tokens
        .iter()
        .find(|t| text(query, t) == "`my-tag`")
        .expect("should have escaped tag");
    assert_eq!(tag.kind, TokenType::Variable);
}

// ── String tokens ────────────────────────────────────────────────

#[test]
fn string_token() {
    let query = r#"ds:metric | filter tag == "hello""#;
    let tokens = collect_tokens(query);
    let s = tokens
        .iter()
        .find(|t| t.kind == TokenType::String)
        .expect("should have string token");
    assert_eq!(text(query, s), r#""hello""#);
}

/// Interpolation highlighting descends into `${ … }`: the literal text
/// fragments (with the quotes merged in) stay `String`, and the embedded
/// expression is classified on its own (`$h` → `Variable`). So
/// `"host ${ $h } end"` sub-tokenizes into the 3-token model, matching the
/// parser's `StringFragment::{Text,Expr}` split — not one opaque `String`.
#[test]
fn interpolated_string_sub_tokenizes() {
    let query = r#"ds:metric | where tag == "host ${ $h } end""#;
    let tokens = collect_tokens(query);
    let i = tokens
        .iter()
        .position(|t| t.kind == TokenType::String && text(query, t) == r#""host "#)
        .expect("leading string fragment");
    assert_eq!(text(query, &tokens[i]), r#""host "#);
    assert_eq!(tokens[i + 1].kind, TokenType::Variable);
    assert_eq!(text(query, &tokens[i + 1]), "$h");
    assert_eq!(tokens[i + 2].kind, TokenType::String);
    assert_eq!(text(query, &tokens[i + 2]), r#" end""#);
    // The whole literal is no longer emitted as a single opaque String token.
    assert!(
        !tokens
            .iter()
            .any(|t| text(query, t) == r#""host ${ $h } end""#)
    );
}

// ── Number tokens ────────────────────────────────────────────────

#[test]
fn number_int() {
    let query = "ds:metric | map + 5";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && text(query, t) == "5")
    );
}

#[test]
fn number_float() {
    let query = "ds:metric | map + 3.14";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && text(query, t) == "3.14")
    );
}

#[test]
fn number_time_relative() {
    let query = "ds:metric | align to 1m using avg";
    let tokens = collect_tokens(query);
    let t = tokens
        .iter()
        .find(|t| text(query, t) == "1m")
        .expect("should have time token");
    assert_eq!(t.kind, TokenType::Number);
}

// ── Bool tokens ──────────────────────────────────────────────────

#[test]
fn bool_token() {
    let query = "ds:metric | filter tag == true";
    let tokens = collect_tokens(query);
    let b = tokens
        .iter()
        .find(|t| t.kind == TokenType::Bool)
        .expect("should have bool token");
    assert_eq!(text(query, b), "true");
}

// ── Regexp tokens ────────────────────────────────────────────────

#[test]
fn regexp_token() {
    let query = "ds:metric | filter tag == #/pattern/";
    let tokens = collect_tokens(query);
    let re = tokens
        .iter()
        .find(|t| t.kind == TokenType::Regexp)
        .expect("should have regexp token");
    assert_eq!(text(query, re), "#/pattern/");
}

#[test]
fn regexp_replace_token() {
    let query = "ds:metric | replace tag ~ #s/foo/bar/";
    let tokens = collect_tokens(query);
    let re = tokens
        .iter()
        .find(|t| t.kind == TokenType::Regexp)
        .expect("should have regexp token");
    assert_eq!(text(query, re), "#s/foo/bar/");
}

// ── Operator tokens ──────────────────────────────────────────────

#[test]
fn operator_cmp() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query);
    let op = tokens
        .iter()
        .find(|t| t.kind == TokenType::Operator)
        .expect("should have operator token");
    assert_eq!(text(query, op), "==");
}

#[test]
fn operator_map_calc() {
    let query = "ds:metric | map + 5";
    let tokens = collect_tokens(query);
    let op = tokens
        .iter()
        .find(|t| t.kind == TokenType::Operator)
        .expect("should have operator token");
    assert_eq!(text(query, op), "+");
}

#[test]
fn operator_compute() {
    let query = "( ds1:m1 , ds2:m2 ) | compute result using /";
    let tokens = collect_tokens(query);
    let op = tokens
        .iter()
        .find(|t| text(query, t) == "/")
        .expect("should have / operator");
    assert_eq!(op.kind, TokenType::Operator);
}

// ── Punctuation tokens ───────────────────────────────────────────

#[test]
fn punctuation_pipe() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query);
    let pipe = tokens
        .iter()
        .find(|t| t.kind == TokenType::Punctuation)
        .expect("should have pipe token");
    assert_eq!(text(query, pipe), "|");
}

// ── Keyword tokens ───────────────────────────────────────────────

#[test]
fn keyword_filter() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| text(query, t) == "filter")
        .expect("filter");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_where() {
    let query = r#"ds:metric | where tag == "x""#;
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| text(query, t) == "where")
        .expect("where");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_not() {
    let query = r#"ds:metric | filter not tag == "x""#;
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| text(query, t) == "not")
        .expect("not");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_is() {
    let query = "ds:metric | where tag is string";
    let tokens = collect_tokens(query);
    let kw = tokens.iter().find(|t| text(query, t) == "is").expect("is");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_ifdef_else_and_bucket_words() {
    // Words outside the slice are still keyword-highlighted lexically (the
    // lexer carries the full MPL keyword set so the editor keeps working
    // across the whole language).
    for (query, word) in [
        ("ds:m | ifdef($f) { where t == $f }", "ifdef"),
        ("ds:m | ifdef($f) {} else {}", "else"),
        (
            "ds:metric | bucket to 1m using histogram(count)",
            "histogram",
        ),
        (
            "ds:metric | bucket to 1m using interpolate_cumulative_histogram(rate, count)",
            "rate",
        ),
        ("ds:metric | sample 10", "sample"),
        ("ds:metric | extend env = \"prod\"", "extend"),
    ] {
        let tokens = collect_tokens(query);
        let kw = tokens
            .iter()
            .find(|t| text(query, t) == word)
            .unwrap_or_else(|| panic!("missing `{word}`"));
        assert_eq!(kw.kind, TokenType::Keyword, "`{word}` should be a keyword");
    }
}

// ── full sequence verification ───────────────────────────────────

#[test]
fn full_query_sequence() {
    let query = r#"ds:metric | filter tag == "hello""#;
    let tokens = collect_tokens(query);
    assert_eq!(tokens.len(), 7);
    assert_eq!(tokens[0].kind, TokenType::Variable);
    assert_eq!(text(query, &tokens[0]), "ds");
    assert_eq!(tokens[1].kind, TokenType::Variable);
    assert_eq!(text(query, &tokens[1]), "metric");
    assert_eq!(tokens[2].kind, TokenType::Punctuation);
    assert_eq!(text(query, &tokens[2]), "|");
    assert_eq!(tokens[3].kind, TokenType::Keyword);
    assert_eq!(text(query, &tokens[3]), "filter");
    assert_eq!(tokens[4].kind, TokenType::Variable);
    assert_eq!(text(query, &tokens[4]), "tag");
    assert_eq!(tokens[5].kind, TokenType::Operator);
    assert_eq!(text(query, &tokens[5]), "==");
    assert_eq!(tokens[6].kind, TokenType::String);
    assert_eq!(text(query, &tokens[6]), r#""hello""#);
}

#[test]
fn is_filter_full_sequence() {
    let query = "ds:metric | where tag is string";
    let tokens = collect_tokens(query);
    assert_eq!(tokens.len(), 7);
    assert_eq!(tokens[3].kind, TokenType::Keyword);
    assert_eq!(text(query, &tokens[3]), "where");
    assert_eq!(tokens[5].kind, TokenType::Keyword);
    assert_eq!(text(query, &tokens[5]), "is");
    assert_eq!(tokens[6].kind, TokenType::Type);
    assert_eq!(text(query, &tokens[6]), "string");
}

// ── param declaration tokens ─────────────────────────────────────

#[test]
fn param_keyword_ident_and_type() {
    let query = "param $dur: duration;\nds:metric";
    let tokens = collect_tokens(query);
    assert_eq!(
        tokens
            .iter()
            .find(|t| text(query, t) == "param")
            .expect("param")
            .kind,
        TokenType::Keyword
    );
    assert_eq!(
        tokens
            .iter()
            .find(|t| text(query, t) == "$dur")
            .expect("$dur")
            .kind,
        TokenType::Variable
    );
    assert_eq!(
        tokens
            .iter()
            .find(|t| text(query, t) == "duration")
            .expect("duration")
            .kind,
        TokenType::Type
    );
}

#[test]
fn param_type_all_variants_highlighted() {
    let types = [
        "Dataset", "Duration", "string", "int", "float", "bool", "Regex",
    ];
    for typ_name in types {
        let query = format!("param $x: {typ_name};\nds:metric");
        let tokens = collect_tokens(&query);
        let typ = tokens
            .iter()
            .find(|t| text(&query, t) == typ_name)
            .unwrap_or_else(|| panic!("missing {typ_name}"));
        assert_eq!(typ.kind, TokenType::Type, "{typ_name} should be Type");
    }
}

#[test]
fn optional_type_option_and_inner_are_types() {
    let query = "param $f: Option<string>;\nds:metric";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| text(query, t) == "Option" && t.kind == TokenType::Type)
    );
    assert!(
        tokens
            .iter()
            .any(|t| text(query, t) == "string" && t.kind == TokenType::Type)
    );
}

// ── comments now highlighted from Rust ───────────────────────────

#[test]
fn comment_token() {
    let query = "// a header\nds:metric";
    let tokens = collect_tokens(query);
    let c = tokens
        .iter()
        .find(|t| t.kind == TokenType::Comment)
        .expect("should have comment token");
    assert_eq!(text(query, c), "// a header");
}

// ── resilience on incomplete / invalid input ─────────────────────

#[test]
fn incomplete_input_still_tokenizes() {
    // Previously these returned `None` and the editor fell back to JS regexes.
    for query in [
        "metric:cpu | filter region == ",
        "metric:cpu | align using ",
        "ds:",
        "ds",
        r#"ds:metric | filter x == "unterminated"#,
    ] {
        let tokens = collect_tokens(query);
        // `filter`/`align`/datasets still classify; never a panic, never empty
        // for these (each has at least an identifier).
        assert!(!tokens.is_empty(), "no tokens for `{query}`");
    }
}

#[test]
fn keyword_survives_incomplete_filter() {
    let query = "metric:cpu | filter region == ";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Keyword && text(query, t) == "filter")
    );
}

#[test]
fn pure_punctuation_does_not_panic() {
    // `{{{}}}` is highlightable-but-empty (braces aren't slice tokens); the key
    // guarantee is no panic and a well-formed (possibly empty) result.
    let _ = collect_tokens("{{{}}}");
}
