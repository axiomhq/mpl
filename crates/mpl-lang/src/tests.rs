use crate::{
    CompileError, ParseError, TypeError,
    query::{Cmp, Filter, ParamType, TagType},
    types::Parameterized,
};

#[test]
fn parse_group_by() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
`dev.metrics`:http_requests_total[1h..]
| where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 3m using prom::rate
| group by method, path, code using sum
    ";
    super::compile(s)?;
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
    super::compile(s)?;
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
    super::compile(s)?;
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
    super::compile(s)?;
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
    super::compile(s)?;
    Ok(())
}

#[test]
fn parse_logic_0() -> Result<(), Box<dyn std::error::Error>> {
    let s = r#"
dataset:metric
| filter a == "snot"
    "#;
    let res = super::compile(s)?;
    let expected = Filter::Cmp {
        field: "a".into(),
        rhs: Cmp::Eq(Parameterized::Concrete("snot".try_into()?)),
    };
    match res {
        crate::Query::Simple { filters, .. } => {
            assert_eq!(1, filters.len());
            assert_eq!(expected, filters[0]);
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
    let res = super::compile(s)?;
    let expected = Filter::And(vec![
        Filter::Cmp {
            field: "a".into(),
            rhs: Cmp::Eq(Parameterized::Concrete(7.0.into())),
        },
        Filter::Not(Box::new(Filter::Cmp {
            field: "b".into(),
            rhs: Cmp::Eq(Parameterized::Concrete(8.into())),
        })),
    ]);
    match res {
        crate::Query::Simple { filters, .. } => {
            assert_eq!(1, filters.len());
            assert_eq!(expected, filters[0]);
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
    let res = super::compile(s)?;
    let expected = Filter::Or(vec![
        Filter::And(vec![
            Filter::Cmp {
                field: "a".into(),
                rhs: Cmp::Eq(Parameterized::Concrete(7.into())),
            },
            Filter::Cmp {
                field: "b".into(),
                rhs: Cmp::Eq(Parameterized::Concrete(8.into())),
            },
        ]),
        Filter::And(vec![
            Filter::Cmp {
                field: "c".into(),
                rhs: Cmp::Eq(Parameterized::Concrete(9.into())),
            },
            Filter::Or(vec![
                Filter::Cmp {
                    field: "d".into(),
                    rhs: Cmp::Eq(Parameterized::Concrete(10.into())),
                },
                Filter::Cmp {
                    field: "e".into(),
                    rhs: Cmp::Eq(Parameterized::Concrete(11.into())),
                },
            ]),
        ]),
    ]);
    match res {
        crate::Query::Simple { filters, .. } => {
            assert_eq!(1, filters.len());
            assert_eq!(expected, filters[0]);
        }
        crate::Query::Compute { .. } => panic!("not a simple query"),
    }

    Ok(())
}

#[test]
fn parse_params() -> Result<(), Box<dyn std::error::Error>> {
    let s = r"
param $dataset: dataset;
param $resolution: duration;
param $name: string;
param $age: int;
param $height: float;
param $is_cool: bool;
param $re: regex;

$dataset:metric
| filter name == $name
| filter age > $age
| filter height > $height
| filter is_cool == $is_cool
| filter matches == $re
| align to $resolution using avg
";
    let res = super::compile(s)?;
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
param $dataset: dataset;
param $dataset: duration;

$dataset:metric
";

    match super::compile(s) {
        Err(CompileError::Parse(ParseError::ParamDefinedMultipleTimes { span: _, param })) => {
            assert_eq!("dataset", param);
        }
        res => panic!("Expected param defined multiple times error, got {res:?}"),
    }
}

#[test]
fn parse_params_undefined() {
    let s = "$dataset:metric";

    match super::compile(s) {
        Err(CompileError::Parse(ParseError::UndefinedParam { span: _, param })) => {
            assert_eq!("dataset", param);
        }
        res => panic!("Expected undefined param error, got {res:?}"),
    }
}

#[test]
fn parse_params_mismatched_type() {
    let s = r"
param $dataset: duration;

$dataset:metric
";

    match super::compile(s) {
        Err(CompileError::Type(TypeError::TypeMismatch {
            use_span,
            declaration_span,
            param_name,
            expected,
            actual,
        })) => {
            assert_eq!("dataset", param_name);
            assert_eq!(&[ParamType::Dataset], expected.as_slice());
            assert_eq!(ParamType::Duration, actual);
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
param $value: dataset;

dataset:metric
| where key == $value
";

    match super::compile(s) {
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
                    ParamType::Tag(TagType::String),
                    ParamType::Tag(TagType::Int),
                    ParamType::Tag(TagType::Float),
                    ParamType::Tag(TagType::Bool)
                ],
                expected.as_slice()
            );
            assert_eq!(ParamType::Dataset, actual);
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
param $duration: dataset;

dataset:metric
| align to $duration using avg
";

    match super::compile(s) {
        Err(CompileError::Type(TypeError::TypeMismatch {
            use_span,
            declaration_span,
            param_name,
            expected,
            actual,
        })) => {
            assert_eq!("duration", param_name);
            assert_eq!(&[ParamType::Duration], expected.as_slice());
            assert_eq!(ParamType::Dataset, actual);
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
    super::compile(s)?;
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
    super::compile(s)?;
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
    assert!(super::compile(s).is_err());
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
    super::compile(s)?;
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
    assert!(super::compile(s).is_err());
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
    super::compile(s)?;
    Ok(())
}
