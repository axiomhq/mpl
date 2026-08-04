use super::{TokenType, collect_tokens};

// ── Variable tokens ──────────────────────────────────────────────

#[test]
fn variable_plain_source() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query);
    assert_eq!(tokens[0].kind, TokenType::Variable);
    assert_eq!(&query[tokens[0].span.from..tokens[0].span.to], "ds");
}

#[test]
fn variable_escaped_ident() {
    let query = r#"ds:metric | filter `my-tag` == "x""#;
    let tokens = collect_tokens(query);
    let tag = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "`my-tag`")
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
    assert_eq!(&query[s.span.from..s.span.to], r#""hello""#);
}

/// Tokens must be sorted by `from` and non-overlapping (CodeMirror requires
/// this). Asserted across the interpolation tests since the new string
/// handling emits multiple tokens per literal.
fn assert_sorted_non_overlapping(tokens: &[super::Token]) {
    for w in tokens.windows(2) {
        assert!(
            w[0].span.to <= w[1].span.from,
            "tokens overlap or unsorted: {:?} then {:?}",
            w[0].span,
            w[1].span
        );
    }
}

#[test]
fn string_interpolation_highlights_inner_param() {
    let query = r#"ds:metric | where tag == "host ${ $h } end""#;
    let tokens = collect_tokens(query);
    assert_sorted_non_overlapping(&tokens);

    // The interpolated param is highlighted as a Variable, not swallowed.
    let var = tokens
        .iter()
        .find(|t| t.kind == TokenType::Variable && &query[t.span.from..t.span.to] == "$h")
        .expect("interpolated param should be a Variable token");
    // It sits between the opening and closing String tokens of the literal.
    let first_string = tokens
        .iter()
        .find(|t| t.kind == TokenType::String)
        .expect("opening quote string token");
    let last_string = tokens
        .iter()
        .rev()
        .find(|t| t.kind == TokenType::String)
        .expect("closing quote string token");
    assert!(first_string.span.to <= var.span.from);
    assert!(var.span.to <= last_string.span.from);

    // The literal text segments are String tokens. A segment carries its own
    // `${` delimiter, so this asserts coverage of the literal text rather than
    // an exact token text.
    let host = query.find("host ").expect("literal text is in the query");
    assert!(
        tokens.iter().any(|t| t.kind == TokenType::String
            && t.span.from <= host
            && t.span.to >= host + "host ".len()),
        "literal text inside an interpolated string should be covered by a String token"
    );
}

#[test]
fn string_interpolation_highlights_number() {
    let query = r#"ds:metric | extend url = "port ${ 8080 }""#;
    let tokens = collect_tokens(query);
    assert_sorted_non_overlapping(&tokens);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && &query[t.span.from..t.span.to] == "8080")
    );
}

#[test]
fn string_interpolation_nested() {
    let query = r#"ds:metric | where tag == "a ${ "b ${ 42 }" } c""#;
    let tokens = collect_tokens(query);
    assert_sorted_non_overlapping(&tokens);
    // The deeply nested number is highlighted.
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && &query[t.span.from..t.span.to] == "42")
    );
}

#[test]
fn string_escaped_dollar_is_not_interpolation() {
    // `\$` is a literal dollar, so the whole thing stays one String token.
    let query = r#"ds:metric | where tag == "price \${ 5 }""#;
    let tokens = collect_tokens(query);
    assert_sorted_non_overlapping(&tokens);
    let s = tokens
        .iter()
        .find(|t| t.kind == TokenType::String)
        .expect("should have string token");
    assert_eq!(&query[s.span.from..s.span.to], r#""price \${ 5 }""#);
    // No Number token, because the braces are literal text.
    assert!(!tokens.iter().any(|t| t.kind == TokenType::Number));
}

// ── Number tokens ────────────────────────────────────────────────

#[test]
fn number_int() {
    let query = "ds:metric | map + 5";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && &query[t.span.from..t.span.to] == "5")
    );
}

#[test]
fn number_float() {
    let query = "ds:metric | map + 3.14";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && &query[t.span.from..t.span.to] == "3.14")
    );
}

#[test]
fn number_time_relative() {
    let query = "ds:metric | align to 1m using avg";
    let tokens = collect_tokens(query);
    let t = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "1m")
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
    assert_eq!(&query[b.span.from..b.span.to], "true");
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
    assert_eq!(&query[re.span.from..re.span.to], "#/pattern/");
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
    assert_eq!(&query[op.span.from..op.span.to], "==");
}

#[test]
fn operator_map_calc() {
    let query = "ds:metric | map + 5";
    let tokens = collect_tokens(query);
    let op = tokens
        .iter()
        .find(|t| t.kind == TokenType::Operator)
        .expect("should have operator token");
    assert_eq!(&query[op.span.from..op.span.to], "+");
}

#[test]
fn operator_compute() {
    let query = "( ds1:m1 , ds2:m2 ) | compute result using /";
    let tokens = collect_tokens(query);
    let op = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "/")
        .expect("should have / operator");
    assert_eq!(op.kind, TokenType::Operator);
}

// `in` shares `Rule::cmp` with the symbolic operators but is word-shaped, so
// it is styled as a keyword (matching `is`), not an operator.
#[test]
fn keyword_in_cmp() {
    let query = r#"ds:metric | where tag in ["a", 2, true]"#;
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "in")
        .expect("should have in token");
    assert_eq!(kw.kind, TokenType::Keyword);
}

// Array literals carry no token themselves (brackets and commas stay gaps);
// each element is tokenized on its own.
#[test]
fn array_elements_tokenized_individually() {
    let query = r#"ds:metric | where tag in ["a", 2, true]"#;
    let tokens = collect_tokens(query);
    let text = |t: &super::Token| query[t.span.from..t.span.to].to_string();
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::String && text(t) == "\"a\""),
        "string element should have a String token"
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && text(t) == "2"),
        "int element should have a Number token"
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Bool && text(t) == "true"),
        "bool element should have a Bool token"
    );
    assert_sorted_non_overlapping(&tokens);
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
    assert_eq!(&query[pipe.span.from..pipe.span.to], "|");
}

// ── Keyword tokens ───────────────────────────────────────────────

#[test]
fn keyword_filter() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "filter")
        .expect("should have filter keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_where() {
    let query = r#"ds:metric | where tag == "x""#;
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "where")
        .expect("should have where keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_not() {
    let query = r#"ds:metric | filter not tag == "x""#;
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "not")
        .expect("should have not keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_ifdef() {
    let query = "param $f: Option<string>;\nds:metric | ifdef($f) { where tag == $f }";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "ifdef")
        .expect("should have ifdef keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_else() {
    // The `else` clause was added alongside `ifdef`; without an explicit
    // `Rule::kw_else` arm in `token_type`, the highlighter would emit no
    // token for `else` and the editor would render it as plain text.
    let query = "param $f: Option<string>;\nds:metric | ifdef($f) { where tag == $f } else { where tag == \"x\" }";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "else")
        .expect("should have else keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_bucket_fn() {
    let query = "ds:metric | bucket to 1m using histogram(count)";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "histogram")
        .expect("should have histogram keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_bucket_conversion() {
    let query = "ds:metric | bucket to 1m using interpolate_cumulative_histogram(rate, count)";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "rate")
        .expect("should have rate keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_bucket_with_conversion_fn() {
    let query = "ds:metric | bucket to 1m using interpolate_cumulative_histogram(rate, count)";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "interpolate_cumulative_histogram")
        .expect("should have keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

// ── sample keyword ───────────────────────────────────────────────

#[test]
fn keyword_sample() {
    let query = "ds:metric | sample 10";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "sample")
        .expect("should have sample keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn sample_number_highlighted() {
    let query = "ds:metric | sample 10";
    let tokens = collect_tokens(query);
    let num = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "10")
        .expect("should have number token");
    assert_eq!(num.kind, TokenType::Number);
}

// ── extend keyword ──────────────────────────────────

#[test]
fn keyword_extend() {
    let query = "ds:metric | extend env = \"prod\"";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "extend")
        .expect("should have extend keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn extend_value_literal_highlighted() {
    // The string literal on the RHS of extend should be tokenised as a
    // string, not blurred into the surrounding ident/keyword tokens.
    let query = "ds:metric | extend env = \"prod\"";
    let tokens = collect_tokens(query);
    let s = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "\"prod\"")
        .expect("should have string token");
    assert_eq!(s.kind, TokenType::String);
}

// ── full sequence verification ───────────────────────────────────

#[test]
fn full_query_sequence() {
    let query = r#"ds:metric | filter tag == "hello""#;
    let tokens = collect_tokens(query);
    assert_eq!(tokens.len(), 7);

    assert_eq!(tokens[0].kind, TokenType::Variable);
    assert_eq!(&query[tokens[0].span.from..tokens[0].span.to], "ds");

    assert_eq!(tokens[1].kind, TokenType::Variable);
    assert_eq!(&query[tokens[1].span.from..tokens[1].span.to], "metric");

    assert_eq!(tokens[2].kind, TokenType::Punctuation);
    assert_eq!(&query[tokens[2].span.from..tokens[2].span.to], "|");

    assert_eq!(tokens[3].kind, TokenType::Keyword);
    assert_eq!(&query[tokens[3].span.from..tokens[3].span.to], "filter");

    assert_eq!(tokens[4].kind, TokenType::Variable);
    assert_eq!(&query[tokens[4].span.from..tokens[4].span.to], "tag");

    assert_eq!(tokens[5].kind, TokenType::Operator);
    assert_eq!(&query[tokens[5].span.from..tokens[5].span.to], "==");

    assert_eq!(tokens[6].kind, TokenType::String);
    assert_eq!(&query[tokens[6].span.from..tokens[6].span.to], r#""hello""#);
}

// ── param declaration tokens ─────────────────────────────────────

#[test]
fn param_keyword_highlighted() {
    let query = "param $dur: duration;\nds:metric";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "param")
        .expect("should have param keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn param_type_duration_highlighted() {
    let query = "param $dur: duration;\nds:metric";
    let tokens = collect_tokens(query);
    let typ = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "duration")
        .expect("should have duration type token");
    assert_eq!(typ.kind, TokenType::Type);
}

#[test]
fn param_ident_highlighted_as_variable() {
    let query = "param $dur: duration;\nds:metric";
    let tokens = collect_tokens(query);
    let var = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "$dur")
        .expect("should have param ident variable");
    assert_eq!(var.kind, TokenType::Variable);
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
            .find(|t| &query[t.span.from..t.span.to] == typ_name)
            .unwrap_or_else(|| panic!("should have {typ_name} type token"));
        assert_eq!(
            typ.kind,
            TokenType::Type,
            "param type '{typ_name}' should be TokenType::Type"
        );
    }
}

#[test]
fn optional_type_option_keyword_is_type() {
    let query = "param $f: Option<string>;\nds:metric";
    let tokens = collect_tokens(query);
    let opt = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "Option")
        .expect("should have Option type token");
    assert_eq!(opt.kind, TokenType::Type);
}

#[test]
fn optional_type_inner_is_separately_tokenized() {
    let query = "param $f: Option<string>;\nds:metric";
    let tokens = collect_tokens(query);
    let inner = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "string")
        .expect("inner type should be tokenized separately");
    assert_eq!(inner.kind, TokenType::Type);
}

#[test]
fn optional_type_inner_param_native_type() {
    // Tokenization is intentionally more lenient than parsing/completions: the
    // editor should keep syntax highlighting useful while users are mid-edit,
    // and diagnostics remain responsible for reporting invalid `Option` inners.
    let query = "param $d: Option<Duration>;\nds:metric";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| &query[t.span.from..t.span.to] == "Option" && t.kind == TokenType::Type),
        "should have Option type token"
    );
    assert!(
        tokens
            .iter()
            .any(|t| &query[t.span.from..t.span.to] == "Duration" && t.kind == TokenType::Type),
        "should have inner Duration type token"
    );
}

#[test]
fn optional_type_all_inner_variants_highlighted() {
    // Keep highlighting lenient for optional inners that diagnostics later
    // reject; this avoids flickering while users edit `Option<...>` types.
    // `Metric` is not an accepted inner type per the grammar — exclude it.
    let inners = [
        "Dataset", "Duration", "Regex", "string", "int", "float", "bool",
    ];
    for inner in inners {
        let query = format!("param $x: Option<{inner}>;\nds:metric");
        let tokens = collect_tokens(&query);
        assert!(
            tokens
                .iter()
                .any(|t| &query[t.span.from..t.span.to] == "Option" && t.kind == TokenType::Type),
            "Option not tokenized as Type for inner={inner}"
        );
        assert!(
            tokens
                .iter()
                .any(|t| &query[t.span.from..t.span.to] == inner && t.kind == TokenType::Type),
            "{inner} not tokenized as Type inside Option<>"
        );
    }
}

#[test]
fn param_multiple_declarations() {
    let query = "param $ds: Dataset;\nparam $d: duration;\nds:metric";
    let tokens = collect_tokens(query);
    let param_keywords: Vec<_> = tokens
        .iter()
        .filter(|t| &query[t.span.from..t.span.to] == "param")
        .collect();
    assert_eq!(param_keywords.len(), 2, "should have two param keywords");
    for kw in &param_keywords {
        assert_eq!(kw.kind, TokenType::Keyword);
    }

    let type_tokens: Vec<_> = tokens
        .iter()
        .filter(|t| t.kind == TokenType::Type)
        .collect();
    assert_eq!(type_tokens.len(), 2, "should have two type tokens");
}

// ── is_filter tokens ─────────────────────────────────────────────

#[test]
fn keyword_is() {
    let query = "ds:metric | where tag is string";
    let tokens = collect_tokens(query);
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "is")
        .expect("should have is keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn is_filter_tag_type_highlighted() {
    let query = "ds:metric | where tag is string";
    let tokens = collect_tokens(query);
    let typ = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "string")
        .expect("should have string type token");
    assert_eq!(typ.kind, TokenType::Type);
}

#[test]
fn is_filter_all_tag_types() {
    let types = ["string", "int", "float", "bool"];
    for typ_name in types {
        let query = format!("ds:metric | where tag is {typ_name}");
        let tokens = collect_tokens(&query);
        let typ = tokens
            .iter()
            .find(|t| &query[t.span.from..t.span.to] == typ_name)
            .unwrap_or_else(|| panic!("should have {typ_name} type token"));
        assert_eq!(
            typ.kind,
            TokenType::Type,
            "tag type '{typ_name}' should be TokenType::Type"
        );
    }
}

#[test]
fn is_filter_full_sequence() {
    let query = "ds:metric | where tag is string";
    let tokens = collect_tokens(query);
    assert_eq!(tokens.len(), 7);

    assert_eq!(tokens[0].kind, TokenType::Variable);
    assert_eq!(&query[tokens[0].span.from..tokens[0].span.to], "ds");

    assert_eq!(tokens[1].kind, TokenType::Variable);
    assert_eq!(&query[tokens[1].span.from..tokens[1].span.to], "metric");

    assert_eq!(tokens[2].kind, TokenType::Punctuation);
    assert_eq!(&query[tokens[2].span.from..tokens[2].span.to], "|");

    assert_eq!(tokens[3].kind, TokenType::Keyword);
    assert_eq!(&query[tokens[3].span.from..tokens[3].span.to], "where");

    assert_eq!(tokens[4].kind, TokenType::Variable);
    assert_eq!(&query[tokens[4].span.from..tokens[4].span.to], "tag");

    assert_eq!(tokens[5].kind, TokenType::Keyword);
    assert_eq!(&query[tokens[5].span.from..tokens[5].span.to], "is");

    assert_eq!(tokens[6].kind, TokenType::Type);
    assert_eq!(&query[tokens[6].span.from..tokens[6].span.to], "string");
}

// ── incomplete queries ───────────────────────────────────────────
//
// Every input below is a query mid-edit. The parser recovers, so the parts that
// are well-formed keep their colours while the user finishes typing.

/// Tokens must be sorted and non-overlapping even when the parser had to
/// recover: CodeMirror's `RangeSetBuilder` rejects anything else, so a
/// recovery path that emitted a token twice would break the whole document's
/// highlighting rather than just the broken span.
fn assert_tokenizes_to(query: &str, expected: &[(TokenType, &str)]) {
    let tokens = collect_tokens(query);
    assert_sorted_non_overlapping(&tokens);
    let actual: Vec<(TokenType, &str)> = tokens
        .into_iter()
        .map(|t| (t.kind, &query[t.span.from..t.span.to]))
        .collect();
    assert_eq!(actual, expected, "for {query:?}");
}

#[test]
fn braces_only_yield_no_tokens() {
    // Nothing here is a colourable construct, so an empty result is right —
    // but it is now "no tokens", not "tokenization failed".
    assert_tokenizes_to("{{{}}}", &[]);
}

#[test]
fn dataset_colon_no_metric_still_highlights_the_dataset() {
    assert_tokenizes_to("ds:", &[(TokenType::Variable, "ds")]);
}

#[test]
fn backtick_dataset_colon_no_metric_still_highlights_the_dataset() {
    assert_tokenizes_to("`my-dataset`:", &[(TokenType::Variable, "`my-dataset`")]);
}

#[test]
fn dataset_no_colon_still_highlights_the_dataset() {
    assert_tokenizes_to("ds", &[(TokenType::Variable, "ds")]);
}

#[test]
fn missing_metric_still_highlights_the_filter_that_follows() {
    // The error is at the source; everything downstream of it is well-formed
    // and keeps its colours.
    assert_tokenizes_to(
        "ds: | filter tag == \"x\"",
        &[
            (TokenType::Variable, "ds"),
            (TokenType::Punctuation, "|"),
            (TokenType::Keyword, "filter"),
            (TokenType::Variable, "tag"),
            (TokenType::Operator, "=="),
            (TokenType::String, "\"x\""),
        ],
    );
}

/// With the `:` missing, the parser reports the `|` and then takes the next
/// identifier as the metric name, so `filter` is read as a metric here and
/// coloured as one. Everything after it is unreachable and falls back to the
/// word itself.
#[test]
fn missing_colon_reads_the_rule_keyword_as_the_metric_name() {
    assert_tokenizes_to(
        "ds | filter tag == \"x\"",
        &[
            (TokenType::Variable, "ds"),
            (TokenType::Punctuation, "|"),
            (TokenType::Variable, "filter"),
            (TokenType::Variable, "tag"),
            (TokenType::Operator, "=="),
            (TokenType::String, "\"x\""),
        ],
    );
}

#[test]
fn backtick_dataset_no_colon_reads_the_rule_keyword_as_the_metric_name() {
    assert_tokenizes_to(
        "`my-dataset` | where tag == \"x\"",
        &[
            (TokenType::Variable, "`my-dataset`"),
            (TokenType::Punctuation, "|"),
            (TokenType::Variable, "where"),
            (TokenType::Variable, "tag"),
            (TokenType::Operator, "=="),
            (TokenType::String, "\"x\""),
        ],
    );
}

#[test]
fn unterminated_string_still_highlights_what_precedes_it() {
    assert_tokenizes_to(
        "ds:metric | where tag == \"oops",
        &[
            (TokenType::Variable, "ds"),
            (TokenType::Variable, "metric"),
            (TokenType::Punctuation, "|"),
            (TokenType::Keyword, "where"),
            (TokenType::Variable, "tag"),
            (TokenType::Operator, "=="),
        ],
    );
}

// ── comments ─────────────────────────────────────────────────────

/// Comments are kept as trivia in the tree, so they are tokens like any other
/// and the editor colours them from the same stream.
#[test]
fn comment_is_a_token() {
    let query = "// a note\nds:metric";
    let tokens = collect_tokens(query);
    let c = tokens
        .iter()
        .find(|t| t.kind == TokenType::Comment)
        .expect("should have a comment token");
    assert_eq!(&query[c.span.from..c.span.to], "// a note");
}

#[test]
fn trailing_comment_is_a_token() {
    let query = "ds:metric // why\n";
    let tokens = collect_tokens(query);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Comment && &query[t.span.from..t.span.to] == "// why")
    );
}

// ── token_at ─────────────────────────────────────────────────────
//
// The point query behind hover: what is the token under this offset.

use super::token_at;

/// `(kind, text)` at `offset`, with the text sliced back out of the query the
/// way a caller does.
fn at(query: &str, offset: usize) -> Option<(TokenType, &str)> {
    token_at(query, offset).map(|t| (t.kind, &query[t.span.from..t.span.to]))
}

#[test]
fn token_at_a_param_reference() {
    // Offsets: `$` at 13, name through 22.
    let q = "where tag == $container and";
    let dollar = q.find('$').expect("param in query");
    // On the `$` itself, mid-name, and on the last name character.
    for offset in [dollar, dollar + 3, dollar + "$container".len() - 1] {
        assert_eq!(
            at(q, offset),
            Some((TokenType::Variable, "$container")),
            "offset {offset}"
        );
    }
}

#[test]
fn token_at_a_param_at_the_start_of_the_query() {
    assert_eq!(at("$ds:metric", 0), Some((TokenType::Variable, "$ds")));
}

#[test]
fn token_at_whitespace_is_nothing() {
    let q = "where tag == $container and";
    let space = q.find('$').expect("param in query") - 1;
    assert_eq!(at(q, space), None);
}

#[test]
fn token_at_a_bare_dollar_is_nothing() {
    // `$` with no name is an invalid token, not a variable.
    let q = "ds:m | where tag == $";
    assert_eq!(at(q, q.len() - 1), None);
}

#[test]
fn token_at_past_the_end_is_nothing() {
    assert_eq!(at("$x", 99), None);
    assert_eq!(at("", 0), None);
}

#[test]
fn token_at_a_tag_is_a_variable_not_a_param() {
    // The hover path tells params from tags by the leading `$`; both are
    // variables here.
    let q = "ds:m | where tag == 1";
    assert_eq!(
        at(q, q.find("tag").expect("tag in query")),
        Some((TokenType::Variable, "tag"))
    );
}

#[test]
fn token_at_a_qualified_function_reports_the_whole_path() {
    // Hovering either segment must yield the name the stdlib is keyed by.
    let q = "ds:m | map prom::rate";
    let prom = q.find("prom").expect("path in query");
    assert_eq!(at(q, prom), Some((TokenType::Variable, "prom::rate")));
    assert_eq!(at(q, prom + 6), Some((TokenType::Variable, "prom::rate")));
}

#[test]
fn token_at_an_unqualified_function_reports_the_name() {
    let q = "ds:m | group using sum";
    assert_eq!(
        at(q, q.find("sum").expect("function in query")),
        Some((TokenType::Variable, "sum"))
    );
}

#[test]
fn token_at_a_rule_keyword() {
    let q = "ds:m | where tag == 1";
    assert_eq!(
        at(q, q.find("where").expect("keyword in query")),
        Some((TokenType::Keyword, "where"))
    );
}

#[test]
fn token_at_a_param_type() {
    // `Option` and its inner type are both types, which is what lets hover
    // document the wrapper separately from what it wraps.
    let q = "param $x: Option<string>;\nds:m";
    assert_eq!(
        at(q, q.find("Option").expect("wrapper in query")),
        Some((TokenType::Type, "Option"))
    );
    assert_eq!(
        at(q, q.find("string").expect("inner type in query")),
        Some((TokenType::Type, "string"))
    );
}

#[test]
fn token_at_never_panics_across_a_whole_query() {
    // Every offset of a query using most of the grammar, so a span that fell
    // outside the text or split a character would surface here.
    let q = "// note\nparam $p: Option<int>;\n`d s`:m[5m..] as x | where é == \"h ${ $p }\" \
             | bucket by a to 1m using histogram(count) | map prom::rate";
    for offset in 0..=q.len() {
        if !q.is_char_boundary(offset) {
            continue;
        }
        if let Some(t) = token_at(q, offset) {
            assert!(
                t.span.from <= t.span.to && t.span.to <= q.len(),
                "bad span {:?} at {offset}",
                t.span
            );
            assert!(
                q.is_char_boundary(t.span.from) && q.is_char_boundary(t.span.to),
                "span splits a character at {offset}"
            );
        }
    }
}

#[test]
fn token_at_a_duration_reports_the_whole_duration() {
    // `1m` is a digit plus a unit to the lexer. `collect_tokens` paints it as
    // one number and `token_at` agrees, so a caller using both sees one span.
    let q = "ds:m | align to 1m using avg";
    let one_m = q.find("1m").expect("duration in query");
    for offset in [one_m, one_m + 1] {
        assert_eq!(at(q, offset), Some((TokenType::Number, "1m")), "@{offset}");
    }

    let painted = collect_tokens(q)
        .into_iter()
        .find(|t| &q[t.span.from..t.span.to] == "1m")
        .expect("duration is painted");
    let pointed = token_at(q, one_m).expect("duration is reported");
    assert_eq!(painted.span, pointed.span);
}
