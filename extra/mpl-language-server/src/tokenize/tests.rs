use super::{Token, TokenType, collect_tokens};

fn kind_of(query: &str, tokens: &[Token], text: &str) -> Option<TokenType> {
    tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == text)
        .map(|t| t.kind)
}

/// Tokens must be sorted and non-overlapping (CodeMirror requires this).
fn assert_sorted_non_overlapping(tokens: &[Token]) {
    for w in tokens.windows(2) {
        assert!(
            w[0].span.to <= w[1].span.from,
            "tokens overlap or unsorted: {:?} then {:?}",
            w[0].span,
            w[1].span
        );
    }
}

// ── classification ───────────────────────────────────────────────

#[test]
fn full_filter_sequence() {
    let query = r#"ds:metric | filter tag == "hello""#;
    let tokens = collect_tokens(query);
    assert_sorted_non_overlapping(&tokens);
    let seq: Vec<(&TokenType, &str)> = tokens
        .iter()
        .map(|t| (&t.kind, &query[t.span.from..t.span.to]))
        .collect();
    assert_eq!(
        seq,
        vec![
            (&TokenType::Variable, "ds"),
            (&TokenType::Punctuation, ":"),
            (&TokenType::Variable, "metric"),
            (&TokenType::Punctuation, "|"),
            (&TokenType::Keyword, "filter"),
            (&TokenType::Variable, "tag"),
            (&TokenType::Operator, "=="),
            (&TokenType::String, "\"hello\""),
        ]
    );
}

#[test]
fn escaped_ident_is_variable() {
    let query = r#"ds:metric | filter `my-tag` == "x""#;
    let tokens = collect_tokens(query);
    assert_eq!(
        kind_of(query, &tokens, "`my-tag`"),
        Some(TokenType::Variable)
    );
}

#[test]
fn param_ident_is_variable() {
    let query = "param $dur: Duration;\nds:metric";
    let tokens = collect_tokens(query);
    assert_eq!(kind_of(query, &tokens, "$dur"), Some(TokenType::Variable));
    assert_eq!(kind_of(query, &tokens, "param"), Some(TokenType::Keyword));
    assert_eq!(kind_of(query, &tokens, "Duration"), Some(TokenType::Type));
}

#[test]
fn numbers_and_durations() {
    let query = "ds:metric | align to 1m using avg";
    let tokens = collect_tokens(query);
    assert_eq!(kind_of(query, &tokens, "1m"), Some(TokenType::Number));
    assert_eq!(kind_of(query, &tokens, "align"), Some(TokenType::Keyword));
    assert_eq!(kind_of(query, &tokens, "using"), Some(TokenType::Keyword));
}

#[test]
fn bool_and_regex_and_types() {
    let query = "ds:metric | filter a == true and b == #/x.*/ and c is string";
    let tokens = collect_tokens(query);
    assert_eq!(kind_of(query, &tokens, "true"), Some(TokenType::Bool));
    assert_eq!(kind_of(query, &tokens, "#/x.*/"), Some(TokenType::Regexp));
    assert_eq!(kind_of(query, &tokens, "is"), Some(TokenType::Keyword));
    assert_eq!(kind_of(query, &tokens, "string"), Some(TokenType::Type));
}

#[test]
fn comment_is_a_token() {
    let query = "// header line\nds:metric";
    let tokens = collect_tokens(query);
    assert_eq!(
        kind_of(query, &tokens, "// header line"),
        Some(TokenType::Comment)
    );
}

#[test]
fn option_type_inner_highlighted() {
    let query = "param $f: Option<string>;\nds:metric";
    let tokens = collect_tokens(query);
    assert_eq!(kind_of(query, &tokens, "Option"), Some(TokenType::Type));
    assert_eq!(kind_of(query, &tokens, "string"), Some(TokenType::Type));
}

#[test]
fn string_interpolation_splits_into_subtokens() {
    // The literal is no longer opaque: the highlight lexer descends into
    // `${ … }`, so the embedded `$h` is its own Variable token and the literal
    // text becomes String fragments either side. The `${`/`}` delimiters carry
    // no colour (trivia) and are dropped from the highlight stream, leaving the
    // 3-token String / Variable / String model.
    let query = r#"ds:metric | filter tag == "host ${ $h } end""#;
    let tokens = collect_tokens(query);
    assert_sorted_non_overlapping(&tokens);
    let seq: Vec<(&TokenType, &str)> = tokens
        .iter()
        .map(|t| (&t.kind, &query[t.span.from..t.span.to]))
        .collect();
    assert_eq!(
        seq,
        vec![
            (&TokenType::Variable, "ds"),
            (&TokenType::Punctuation, ":"),
            (&TokenType::Variable, "metric"),
            (&TokenType::Punctuation, "|"),
            (&TokenType::Keyword, "filter"),
            (&TokenType::Variable, "tag"),
            (&TokenType::Operator, "=="),
            (&TokenType::String, "\"host "),
            (&TokenType::Variable, "$h"),
            (&TokenType::String, " end\""),
        ]
    );
}

// ── the headline: tokens on incomplete / invalid input ───────────

#[test]
fn incomplete_filter_rhs_still_tokenizes() {
    // Previously this returned `None` (no highlighting). Now it tokenises.
    let query = "metric:cpu | filter region == ";
    let tokens = collect_tokens(query);
    assert!(!tokens.is_empty());
    assert_eq!(kind_of(query, &tokens, "filter"), Some(TokenType::Keyword));
    assert_eq!(kind_of(query, &tokens, "=="), Some(TokenType::Operator));
}

#[test]
fn incomplete_align_using_still_tokenizes() {
    let query = "metric:cpu | align using ";
    let tokens = collect_tokens(query);
    assert_eq!(kind_of(query, &tokens, "align"), Some(TokenType::Keyword));
    assert_eq!(kind_of(query, &tokens, "using"), Some(TokenType::Keyword));
}

#[test]
fn previously_none_inputs_now_tokenize() {
    // Every one of these made the old pest-based tokenizer return `None`.
    for query in ["ds:", "ds", "ds | filter tag == \"x\"", "`my-dataset`:"] {
        let tokens = collect_tokens(query);
        assert!(!tokens.is_empty(), "expected tokens for {query:?}");
        assert_sorted_non_overlapping(&tokens);
    }
}

#[test]
fn pathological_input_never_panics() {
    for query in ["", "{{{}}}", "|||", "@@@", "#", "$"] {
        let _ = collect_tokens(query);
    }
}
