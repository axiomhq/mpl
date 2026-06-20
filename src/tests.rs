use std::collections::HashMap;

use miette::NarratableReportHandler;

use crate::{
    CompileError, ParseError, TypeError,
    query::{Cmp, DirectiveValue, Expr, Filter, TagType, TerminalParamType},
};

fn render_diagnostic(err: CompileError, src: &str) -> String {
    let report = miette::Report::new(err).with_source_code(src.to_string());
    let mut out = String::new();
    NarratableReportHandler::new()
        .render_report(&mut out, report.as_ref())
        .expect("rendering a miette diagnostic should not fail");
    out
}

#[test]
fn parse_align_without_time() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[1h..]
| where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align using sum
| group by method, path, code using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn align_with_time_but_without_to_reports_missing_to() {
    // `align <time> using <fn>` is invalid: a time after `align` must be
    // introduced by `to` (grammar: `align ("to" time)? ("over" time)? using fn`).
    // The hygienic error should name the real problem -- a missing `to` after
    // `align` -- rather than the generic pest fallback that points at `align`
    // itself and suggests "Did you mean align?".
    let s = "dataset:metric | align 1m using avg";
    let err = super::compile(s, HashMap::new())
        .expect_err("`align 1m using avg` is missing `to` and must fail");
    let rendered = render_diagnostic(err, s);
    assert!(
        rendered.contains("to") && rendered.contains("align"),
        "expected the diagnostic to point out that `to` is missing after `align`, got:\n{rendered}"
    );
}

#[test]
fn parse_bucket_without_time() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_duration[1h..]
| where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| bucket by method, path, code using histogram(max)
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn parse_group_by() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[1h..]
| where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn parse_group_ts() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[1747077736092..]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn parse_group_rfc() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[2025-03-01T13:00:00Z..+1h]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn parse_group_rate() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[2025-03-01T13:00:00Z..+1h]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn parse_re_escape() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[2025-03-01T13:00:00Z..+1h]
| filter path == #/\.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn parse_logic_0() -> Result<(), Box<dyn std::error::Error>> {
    let s = r#"
dataset:metric
| filter a == "snot"
    "#;
    let (res, _) = super::compile(s, HashMap::new())?;
    let expected = Filter::Cmp {
        field: "a".into(),
        rhs: Cmp::Eq(Expr::Const("snot".try_into()?)),
    };
    match res {
        crate::Query::Simple { filters, .. } => {
            assert_eq!(1, filters.len());
            assert_eq!(&expected, filters[0].filter());
        }
        crate::Query::Compute { .. } => panic!("not a simple query"),
    }

    Ok(())
}

#[test]
fn parse_logic_1() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
dataset:metric
| filter a == 7.0 and not b == 8
    ";
    let (res, _) = super::compile(s, HashMap::new())?;
    let expected = Filter::And(vec![
        Filter::Cmp {
            field: "a".into(),
            rhs: Cmp::Eq(Expr::Const(7.0.into())),
        },
        Filter::Not(Box::new(Filter::Cmp {
            field: "b".into(),
            rhs: Cmp::Eq(Expr::Const(8.into())),
        })),
    ]);
    match res {
        crate::Query::Simple { filters, .. } => {
            assert_eq!(1, filters.len());
            assert_eq!(&expected, filters[0].filter());
        }
        crate::Query::Compute { .. } => panic!("not a simple query"),
    }

    Ok(())
}

#[test]
fn parse_logic_2() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
dataset:metric
| filter a == 7 and b == 8 or c == 9 and ( d == 10 or e == 11 )
    ";
    let (res, _) = super::compile(s, HashMap::new())?;
    let expected = Filter::Or(vec![
        Filter::And(vec![
            Filter::Cmp {
                field: "a".into(),
                rhs: Cmp::Eq(Expr::Const(7.into())),
            },
            Filter::Cmp {
                field: "b".into(),
                rhs: Cmp::Eq(Expr::Const(8.into())),
            },
        ]),
        Filter::And(vec![
            Filter::Cmp {
                field: "c".into(),
                rhs: Cmp::Eq(Expr::Const(9.into())),
            },
            Filter::Or(vec![
                Filter::Cmp {
                    field: "d".into(),
                    rhs: Cmp::Eq(Expr::Const(10.into())),
                },
                Filter::Cmp {
                    field: "e".into(),
                    rhs: Cmp::Eq(Expr::Const(11.into())),
                },
            ]),
        ]),
    ]);
    match res {
        crate::Query::Simple { filters, .. } => {
            assert_eq!(1, filters.len());
            assert_eq!(&expected, filters[0].filter());
        }
        crate::Query::Compute { .. } => panic!("not a simple query"),
    }

    Ok(())
}

#[test]
fn parse_idents() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
dataset:metric
| where tag == infra
| where tag2 == trueish
| where tag3 == falseish
| where tag4 == inf|where tag5==true|where tag6==false";
    let (res, _) = super::compile(s, HashMap::new())?;
    let expected = [
        Filter::Cmp {
            field: "tag".into(),
            rhs: Cmp::Eq(Expr::Tag("infra".to_string())),
        },
        Filter::Cmp {
            field: "tag2".into(),
            rhs: Cmp::Eq(Expr::Tag("trueish".to_string())),
        },
        Filter::Cmp {
            field: "tag3".into(),
            rhs: Cmp::Eq(Expr::Tag("falseish".to_string())),
        },
        Filter::Cmp {
            field: "tag4".into(),
            rhs: Cmp::Eq(Expr::Const(f64::INFINITY.into())),
        },
        Filter::Cmp {
            field: "tag5".into(),
            rhs: Cmp::Eq(Expr::Const(true.into())),
        },
        Filter::Cmp {
            field: "tag6".into(),
            rhs: Cmp::Eq(Expr::Const(false.into())),
        },
    ];
    match res {
        crate::Query::Simple { filters, .. } => {
            assert_eq!(6, filters.len());
            for (i, filter) in filters.iter().enumerate() {
                assert_eq!(&expected[i], filter.filter());
            }
        }
        crate::Query::Compute { .. } => panic!("not a simple query"),
    }

    Ok(())
}

#[test]
fn parse_params() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
param $dataset: Dataset;
param $resolution: Duration;
param $name: string;
param $age: int;
param $height: float;
param $is_cool: bool;
param $re: Regex;

$dataset:metric
| filter name == $name
| filter age > $age
| filter height > $height
| filter is_cool == $is_cool
| filter matches == $re
| align to $resolution using avg
";
    let (res, _) = super::compile(s, HashMap::new())?;
    match res {
        crate::Query::Simple { source, .. } => {
            assert!(source.metric_id.dataset.is_param());
        }
        crate::Query::Compute { .. } => panic!("not a simple query"),
    }

    Ok(())
}

#[test]
fn parse_params_multi_define() {
    let s = r"
param $dataset: Dataset;
param $dataset: Duration;

$dataset:metric
";

    match super::compile(s, HashMap::new()) {
        Err(CompileError::Parse(ParseError::ParamDefinedMultipleTimes { span: _, param })) => {
            assert_eq!("dataset", param);
        }
        res => panic!("Expected param defined multiple times error, got {res:?}"),
    }
}

#[test]
fn parse_params_undefined() {
    let s = "$dataset:metric";

    match super::compile(s, HashMap::new()) {
        Err(CompileError::Parse(ParseError::UndefinedParam { span: _, param })) => {
            assert_eq!("dataset", param);
        }
        res => panic!("Expected undefined param error, got {res:?}"),
    }
}

#[test]
fn parse_params_mismatched_type() {
    let s = r"
param $dataset: Duration;

$dataset:metric
";

    match super::compile(s, HashMap::new()) {
        Err(CompileError::Type(TypeError::TypeMismatch {
            use_span,
            declaration_span,
            param_name,
            expected,
            actual,
        })) => {
            assert_eq!("dataset", param_name);
            assert_eq!(&[TerminalParamType::Dataset], expected.as_slice());
            assert_eq!(TerminalParamType::Duration, actual);
            assert_eq!(28, use_span.offset());
            assert_eq!(8, use_span.len());
            assert_eq!(7, declaration_span.offset());
            assert_eq!(8, declaration_span.len());
        }
        res => panic!("Expected mismatched param type error, got {res:?}"),
    }
}

#[test]
fn parse_params_mismatched_type_value() {
    let s = r"
param $value: Dataset;

dataset:metric
| where key == $value
";

    match super::compile(s, HashMap::new()) {
        Err(CompileError::Type(TypeError::TypeMismatch {
            use_span,
            declaration_span,
            param_name,
            expected,
            actual,
        })) => {
            assert_eq!("value", param_name);
            assert_eq!(
                &[
                    TerminalParamType::Tag(TagType::String),
                    TerminalParamType::Tag(TagType::Int),
                    TerminalParamType::Tag(TagType::Float),
                    TerminalParamType::Tag(TagType::Bool)
                ],
                expected.as_slice()
            );
            assert_eq!(TerminalParamType::Dataset, actual);
            assert_eq!(55, use_span.offset());
            assert_eq!(6, use_span.len());
            assert_eq!(7, declaration_span.offset());
            assert_eq!(6, declaration_span.len());
        }
        res => panic!("Expected mismatched param type error, got {res:?}"),
    }
}

#[test]
fn parse_params_mismatched_type_duration() {
    let s = r"
param $duration: Dataset;

dataset:metric
| align to $duration using avg
";

    match super::compile(s, HashMap::new()) {
        Err(CompileError::Type(TypeError::TypeMismatch {
            use_span,
            declaration_span,
            param_name,
            expected,
            actual,
        })) => {
            assert_eq!("duration", param_name);
            assert_eq!(&[TerminalParamType::Duration], expected.as_slice());
            assert_eq!(TerminalParamType::Dataset, actual);
            assert_eq!(54, use_span.offset());
            assert_eq!(9, use_span.len());
            assert_eq!(7, declaration_span.offset());
            assert_eq!(9, declaration_span.len());
        }
        res => panic!("Expected mismatched param type error, got {res:?}"),
    }
}

#[test]
fn group_by_two() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[1747077736092..]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
| group by method, path using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn group_by_two_same() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[1747077736092..]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
| group by method, path, code using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn group_by_two_error() {
    let s = r"
`dev.metrics`:http_requests_total[1747077736092..]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path using sum
| group by method, path, code using sum
    ";
    assert!(super::compile(s, HashMap::new()).is_err());
}

#[test]
fn bucket_group_by() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[1747077736092..]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| bucket by method, path, code to 5m using histogram(max)
| group by method, path using sum
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn bucket_group_by_error() {
    let s = r"
`dev.metrics`:http_requests_total[1747077736092..]
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path using sum
| bucket by method, path, code to 5m using histogram(max)
    ";
    assert!(super::compile(s, HashMap::new()).is_err());
}
#[test]
fn group_by_compute() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
(
  `ds`:m1[1h..]
  | group by method, code using sum,
  `ds`:m2[1h..]
  | group by method, path using sum
)
| compute test using +
    ";
    super::compile(s, HashMap::new())?;
    Ok(())
}

#[test]
fn directive_string() -> Result<(), Box<dyn std::error::Error>> {
    let (query, _warnings) = super::compile("set foo = \"bar\";\ndataset:metric", HashMap::new())?;
    assert_eq!(
        query.directives().get("foo"),
        Some(&DirectiveValue::String("bar".to_string()))
    );
    Ok(())
}

#[test]
fn interp_display() -> Result<(), Box<dyn std::error::Error>> {
    let src = r#"param $host: string;
dataset:metric
| extend url = "http://${ $host }:${ 8080 }""#;
    let (query, _warnings) = super::compile(src, HashMap::new())?;
    let printed = query.to_string();
    super::compile(&printed, HashMap::new())
        .unwrap_or_else(|e| panic!("printed query did not re-parse: {e}\nprinted:\n{printed}"));
    Ok(())
}

#[test]
fn tag_expr_display() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        "dataset:metric\n| where foo == bar",
        "dataset:metric\n| extend x = some_tag",
        "dataset:metric\n| extend x = `weird tag`",
        "dataset:metric\n| where foo == `weird-tag`",
        "dataset:metric\n| extend url = \"http://${ id }\"",
    ];
    for src in cases {
        let (query, _warnings) = super::compile(src, HashMap::new())?;
        let printed = query.to_string();
        let (reparsed, _warnings) = super::compile(&printed, HashMap::new())?;
        assert_eq!(
            printed,
            reparsed.to_string(),
            "round-trip unstable for {src:?} vs {printed:?}"
        );
    }
    Ok(())
}
