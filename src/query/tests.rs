use miette::SourceSpan;

use crate::{
    enc_regex::EncodableRegex,
    query::{
        Param, ParamType, ParamValue, ParseProvidedParamsError, ProvidedParam, ProvidedParams,
        RelativeTime, TagType, TimeUnit,
    },
    types::Dataset,
};

#[test]
fn provided_params_parse() {
    let mpl_params = vec![
        Param {
            span: SourceSpan::from(0..0),
            name: "dataset".to_string(),
            typ: ParamType::Dataset,
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "duration".to_string(),
            typ: ParamType::Duration,
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "string".to_string(),
            typ: ParamType::Tag(TagType::String),
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "int".to_string(),
            typ: ParamType::Tag(TagType::Int),
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "float".to_string(),
            typ: ParamType::Tag(TagType::Float),
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "bool".to_string(),
            typ: ParamType::Tag(TagType::Bool),
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "regex".to_string(),
            typ: ParamType::Regex,
        },
    ];

    let query_params = [
        ("lol", "whatever, this should be ignored"),
        ("param__dataset", "`my-metrics`"),
        ("param__duration", "42s"),
        ("param__string", "\"bar\""),
        ("param__int", "42"),
        ("param__float", "42.0"),
        ("param__bool", "true"),
        ("param__regex", "#/[a-z]+/"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect::<Vec<(String, String)>>();

    let (provided_params, _) = ProvidedParams::parse_and_validate(&mpl_params, &query_params)
        .expect("failed to parse provided params");

    assert!(provided_params.as_slice().contains(&ProvidedParam {
        name: "dataset".to_string(),
        value: ParamValue::Dataset(Dataset::new("my-metrics".to_string()))
    }));
    assert!(provided_params.as_slice().contains(&ProvidedParam {
        name: "duration".to_string(),
        value: ParamValue::Duration(RelativeTime {
            value: 42,
            unit: TimeUnit::Second
        })
    }));
    assert!(provided_params.as_slice().contains(&ProvidedParam {
        name: "string".to_string(),
        value: ParamValue::String("bar".to_string())
    }));
    assert!(provided_params.as_slice().contains(&ProvidedParam {
        name: "int".to_string(),
        value: ParamValue::Int(42)
    }));
    assert!(provided_params.as_slice().contains(&ProvidedParam {
        name: "float".to_string(),
        value: ParamValue::Float(42.0)
    }));
    assert!(provided_params.as_slice().contains(&ProvidedParam {
        name: "bool".to_string(),
        value: ParamValue::Bool(true)
    }));
    assert!(provided_params.as_slice().contains(&ProvidedParam {
        name: "regex".to_string(),
        value: ParamValue::Regex(EncodableRegex::new("[a-z]+").expect("invalid regex"))
    }));
    assert_eq!(7, provided_params.as_slice().len());
}

#[test]
fn provided_params_duplicates() {
    let mpl_params = vec![
        Param {
            span: SourceSpan::from(0..0),
            name: "dataset".to_string(),
            typ: ParamType::Dataset,
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "__interval".to_string(),
            typ: ParamType::Duration,
        },
    ];

    let query_params = [
        ("lol", "whatever, this should be ignored"),
        ("param__dataset", "my-metrics"),
        ("param__dataset", "my-metrics-2"),
        ("param____interval", "1m"),
        ("param____interval", "5m"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect::<Vec<(String, String)>>();

    match ProvidedParams::parse_and_validate(&mpl_params, &query_params) {
        Err(ParseProvidedParamsError::ParamsProvidedMoreThanOnce(list)) => {
            assert_eq!(2, list.as_slice().len());
            assert!(list.as_slice().contains(&"dataset".to_string()));
            assert!(list.as_slice().contains(&"__interval".to_string()));
        }
        res => panic!("expected duplicate params error, got {res:?}"),
    }
}

#[test]
fn provided_params_declared_but_not_provided() {
    let mpl_params = vec![Param {
        span: SourceSpan::from(0..0),
        name: "dataset".to_string(),
        typ: ParamType::Dataset,
    }];

    let query_params = [("lol", "whatever, this should be ignored")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<Vec<(String, String)>>();

    match ProvidedParams::parse_and_validate(&mpl_params, &query_params) {
        Err(ParseProvidedParamsError::ParamsDeclaredButNotProvided(list)) => {
            assert_eq!(1, list.as_slice().len());
            assert!(list.as_slice().contains(&"dataset".to_string()));
        }
        res => panic!("expected params declared but not provided error, got {res:?}"),
    }
}

#[test]
fn provided_params_not_declared() {
    let mpl_params = vec![];

    let query_params = [("param__dataset", "yo")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<Vec<(String, String)>>();

    let (_, warnings) =
        ProvidedParams::parse_and_validate(&mpl_params, &query_params).expect("expected ok");
    assert_eq!(1, warnings.as_slice().len());
    let warning = warnings
        .as_slice()
        .first()
        .expect("expected a warning, i just checked?");
    assert!(warning.contains("provided but not declared: $dataset"));
}

#[test]
fn provided_params_parse_failure() {
    let mpl_params = vec![
        Param {
            span: SourceSpan::from(0..0),
            name: "dataset".to_string(),
            typ: ParamType::Dataset,
        },
        Param {
            span: SourceSpan::from(0..0),
            name: "duration".to_string(),
            typ: ParamType::Duration,
        },
    ];

    let query_params = [
        ("param__dataset", "`my-dataset`"),
        ("param__duration", "invalid-duration"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect::<Vec<(String, String)>>();

    match ProvidedParams::parse_and_validate(&mpl_params, &query_params) {
        Err(ParseProvidedParamsError::ParseParam {
            param_name,
            expected_type,
            err: _,
        }) => {
            assert_eq!("duration", param_name);
            assert_eq!(ParamType::Duration, expected_type);
        }
        res => panic!("expected parse param error, got {res:?}"),
    }
}

#[test]
fn too_many_provided_params() {
    let mpl_params = vec![];

    let query_params = (0..129)
        .map(|i| (format!("param__dataset_{i}"), format!("value_{i}")))
        .collect::<Vec<(String, String)>>();

    match ProvidedParams::parse_and_validate(&mpl_params, &query_params) {
        Err(ParseProvidedParamsError::TooManyParamsProvided(_)) => {
            // ok
        }
        res => panic!("expected too many params error, got {res:?}"),
    }
}
