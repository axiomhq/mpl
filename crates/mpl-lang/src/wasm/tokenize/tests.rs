use super::{TokenType, collect_tokens};

// ── Variable tokens ──────────────────────────────────────────────

#[test]
fn variable_plain_source() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query).expect("should tokenize");
    assert_eq!(tokens[0].kind, TokenType::Variable);
    assert_eq!(&query[tokens[0].span.from..tokens[0].span.to], "ds");
}

#[test]
fn variable_escaped_ident() {
    let query = r#"ds:metric | filter `my-tag` == "x""#;
    let tokens = collect_tokens(query).expect("should tokenize");
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
    let tokens = collect_tokens(query).expect("should tokenize");
    let s = tokens
        .iter()
        .find(|t| t.kind == TokenType::String)
        .expect("should have string token");
    assert_eq!(&query[s.span.from..s.span.to], r#""hello""#);
}

// ── Number tokens ────────────────────────────────────────────────

#[test]
fn number_int() {
    let query = "ds:metric | map + 5";
    let tokens = collect_tokens(query).expect("should tokenize");
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && &query[t.span.from..t.span.to] == "5")
    );
}

#[test]
fn number_float() {
    let query = "ds:metric | map + 3.14";
    let tokens = collect_tokens(query).expect("should tokenize");
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenType::Number && &query[t.span.from..t.span.to] == "3.14")
    );
}

#[test]
fn number_time_relative() {
    let query = "ds:metric | align to 1m using avg";
    let tokens = collect_tokens(query).expect("should tokenize");
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
    let tokens = collect_tokens(query).expect("should tokenize");
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
    let tokens = collect_tokens(query).expect("should tokenize");
    let re = tokens
        .iter()
        .find(|t| t.kind == TokenType::Regexp)
        .expect("should have regexp token");
    assert_eq!(&query[re.span.from..re.span.to], "#/pattern/");
}

#[test]
fn regexp_replace_token() {
    let query = "ds:metric | replace tag ~ #s/foo/bar/";
    let tokens = collect_tokens(query).expect("should tokenize");
    let re = tokens
        .iter()
        .find(|t| t.kind == TokenType::Regexp)
        .expect("should have regexp token");
    assert_eq!(&query[re.span.from..re.span.to], "#s/foo/bar/");
}

// ── Operator tokens ──────────────────────────────────────────────

#[test]
fn operator_cmp() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query).expect("should tokenize");
    let op = tokens
        .iter()
        .find(|t| t.kind == TokenType::Operator)
        .expect("should have operator token");
    assert_eq!(&query[op.span.from..op.span.to], "==");
}

#[test]
fn operator_map_calc() {
    let query = "ds:metric | map + 5";
    let tokens = collect_tokens(query).expect("should tokenize");
    let op = tokens
        .iter()
        .find(|t| t.kind == TokenType::Operator)
        .expect("should have operator token");
    assert_eq!(&query[op.span.from..op.span.to], "+");
}

#[test]
fn operator_compute() {
    let query = "( ds1:m1 , ds2:m2 ) | compute result using /";
    let tokens = collect_tokens(query).expect("should tokenize");
    let op = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "/")
        .expect("should have / operator");
    assert_eq!(op.kind, TokenType::Operator);
}

// ── Punctuation tokens ───────────────────────────────────────────

#[test]
fn punctuation_pipe() {
    let query = r#"ds:metric | filter tag == "x""#;
    let tokens = collect_tokens(query).expect("should tokenize");
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
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "filter")
        .expect("should have filter keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_where() {
    let query = r#"ds:metric | where tag == "x""#;
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "where")
        .expect("should have where keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_not() {
    let query = r#"ds:metric | filter not tag == "x""#;
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "not")
        .expect("should have not keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_bucket_fn() {
    let query = "ds:metric | bucket to 1m using histogram(count)";
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "histogram")
        .expect("should have histogram keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_bucket_conversion() {
    let query = "ds:metric | bucket to 1m using interpolate_cumulative_histogram(rate, count)";
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "rate")
        .expect("should have rate keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn keyword_bucket_with_conversion_fn() {
    let query = "ds:metric | bucket to 1m using interpolate_cumulative_histogram(rate, count)";
    let tokens = collect_tokens(query).expect("should tokenize");
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
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "sample")
        .expect("should have sample keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn sample_number_highlighted() {
    let query = "ds:metric | sample 10";
    let tokens = collect_tokens(query).expect("should tokenize");
    let num = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "10")
        .expect("should have number token");
    assert_eq!(num.kind, TokenType::Number);
}

// ── full sequence verification ───────────────────────────────────

#[test]
fn full_query_sequence() {
    let query = r#"ds:metric | filter tag == "hello""#;
    let tokens = collect_tokens(query).expect("should tokenize");
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
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "param")
        .expect("should have param keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn param_type_duration_highlighted() {
    let query = "param $dur: duration;\nds:metric";
    let tokens = collect_tokens(query).expect("should tokenize");
    let typ = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "duration")
        .expect("should have duration type token");
    assert_eq!(typ.kind, TokenType::Type);
}

#[test]
fn param_ident_highlighted_as_variable() {
    let query = "param $dur: duration;\nds:metric";
    let tokens = collect_tokens(query).expect("should tokenize");
    let var = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "$dur")
        .expect("should have param ident variable");
    assert_eq!(var.kind, TokenType::Variable);
}

#[test]
fn param_type_all_variants_highlighted() {
    let types = [
        "dataset", "duration", "string", "int", "float", "bool", "regex",
    ];
    for typ_name in types {
        let query = format!("param $x: {typ_name};\nds:metric");
        let tokens =
            collect_tokens(&query).expect(&format!("should tokenize with type {typ_name}"));
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
fn param_multiple_declarations() {
    let query = "param $ds: dataset;\nparam $d: duration;\nds:metric";
    let tokens = collect_tokens(query).expect("should tokenize");
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
    let tokens = collect_tokens(query).expect("should tokenize");
    let kw = tokens
        .iter()
        .find(|t| &query[t.span.from..t.span.to] == "is")
        .expect("should have is keyword");
    assert_eq!(kw.kind, TokenType::Keyword);
}

#[test]
fn is_filter_tag_type_highlighted() {
    let query = "ds:metric | where tag is string";
    let tokens = collect_tokens(query).expect("should tokenize");
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
        let tokens =
            collect_tokens(&query).expect(&format!("should tokenize with type {typ_name}"));
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
    let tokens = collect_tokens(query).expect("should tokenize");
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

// ── edge cases ───────────────────────────────────────────────────

#[test]
fn invalid_query_returns_none() {
    assert!(collect_tokens("{{{}}}").is_none());
}

// ── dataset given, no metric ─────────────────────────────────────

#[test]
fn dataset_colon_no_metric_returns_none() {
    assert!(collect_tokens("ds:").is_none());
}

#[test]
fn backtick_dataset_colon_no_metric_returns_none() {
    assert!(collect_tokens("`my-dataset`:").is_none());
}

#[test]
fn dataset_no_colon_returns_none() {
    assert!(collect_tokens("ds").is_none());
}

#[test]
fn dataset_no_metric_with_filter_returns_none() {
    assert!(collect_tokens("ds: | filter tag == \"x\"").is_none());
}

#[test]
fn dataset_no_colon_with_filter_returns_none() {
    assert!(collect_tokens("ds | filter tag == \"x\"").is_none());
}

#[test]
fn backtick_dataset_no_colon_with_where_returns_none() {
    assert!(collect_tokens("`my-dataset` | where tag == \"x\"").is_none());
}
