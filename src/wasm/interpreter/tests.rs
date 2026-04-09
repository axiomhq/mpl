use super::*;
use crate::{
    enc_regex::EncodableRegex,
    linker::{AlignFunction, ComputeFunction, GroupFunction, MapFunction},
    query::{
        Aggregate, Align, As, BucketBy, Cmp, Filter, GroupBy, Mapping, MetricId, Source, TagType,
    },
    tags::TagValue,
    types::{BucketSpec, BucketType, ComputeType, Dataset, MapType, Metric},
    wasm::steps::{PipeStep, StepNode},
};
use miette::{SourceOffset, SourceSpan};

fn span() -> SourceSpan {
    SourceSpan::new(SourceOffset::from(0), 0)
}

fn step(node: StepNode) -> PipeStep {
    PipeStep {
        span: span(),
        canonical: String::new(),
        node,
    }
}

fn source_node(ds: &str, metric: &str) -> StepNode {
    StepNode::Source(Source {
        metric_id: MetricId {
            dataset: Parameterized::Concrete(Dataset::from(ds)),
            metric: Metric::new(metric).unwrap(),
        },
        time: None,
    })
}

fn ds(dataset: &str, metric: &str, series: Vec<Series>) -> Datasets {
    vec![DatasetEntry {
        name: dataset.into(),
        metrics: vec![MetricEntry {
            name: metric.into(),
            series,
        }],
    }]
}

fn s(tags: &[(&str, &str)], timestamps: Vec<f64>, values: Vec<f64>) -> Series {
    let tag_map: HashMap<String, String> = tags
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Series {
        name: series_name(&tag_map),
        tags: tag_map,
        timestamps,
        values,
    }
}

fn map_agg(t: MapType, arg: Option<f64>) -> StepNode {
    StepNode::Aggregate(Aggregate::Map(Mapping {
        function: MapFunction::Builtin(t),
        arg,
    }))
}

fn group_agg(func: TagsType, tags: Vec<String>) -> StepNode {
    StepNode::Aggregate(Aggregate::GroupBy(GroupBy {
        span: span(),
        function: GroupFunction::Builtin(func),
        tags,
    }))
}

fn align_agg(func: TimeType, secs: u64) -> StepNode {
    StepNode::Aggregate(Aggregate::Align(Align {
        function: AlignFunction::Builtin(func),
        time: Parameterized::Concrete(RelativeTime {
            value: secs,
            unit: TimeUnit::Second,
        }),
    }))
}

#[test]
fn source_loads_series() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(&[("h", "a")], vec![0.0, 60.0], vec![10.0, 20.0])],
    );
    let result = interpret(&[step(source_node("ds", "m"))], &datasets);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].as_ref().unwrap().len(), 1);
    assert_eq!(result[0].as_ref().unwrap()[0].values, vec![10.0, 20.0]);
    assert!(result[0].is_ok());
}

#[test]
fn source_parameterized_error() {
    let datasets: Datasets = vec![];
    let node = StepNode::Source(Source {
        metric_id: MetricId {
            dataset: Parameterized::Param {
                span: span(),
                param: crate::query::Param {
                    span: span(),
                    name: "ds".into(),
                    typ: crate::query::ParamType::Dataset,
                },
            },
            metric: Metric::new("m").unwrap(),
        },
        time: None,
    });
    let result = interpret(&[step(node)], &datasets);
    assert!(
        result[0]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Parameterized")
    );
}

#[test]
fn filter_eq() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("host", "a")], vec![0.0], vec![1.0]),
            s(&[("host", "b")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "host".into(),
        rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
            strumbra::SharedString::try_from("a").unwrap(),
        ))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![1.0]);
}

#[test]
fn filter_regex() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("path", "/api/v1")], vec![0.0], vec![1.0]),
            s(&[("path", "/health")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "path".into(),
        rhs: Cmp::RegEx(Parameterized::Concrete(
            EncodableRegex::new("^/api").unwrap(),
        )),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
}

#[test]
fn filter_unknown_tag() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![1.0])]);
    let filter = Filter::Cmp {
        field: "missing".into(),
        rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
            strumbra::SharedString::try_from("x").unwrap(),
        ))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert!(
        result[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Unknown tag")
    );
}

#[test]
fn map_mul() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![10.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::Mul, Some(2.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![20.0]);
}

#[test]
fn map_div_zero() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![10.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::Div, Some(0.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert!(
        result[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Division by zero")
    );
}

#[test]
fn map_rate() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(&[], vec![0.0, 60.0, 120.0], vec![0.0, 60.0, 180.0])],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::Rate, None)),
    ];
    let result = interpret(&steps, &datasets);
    let v = &result[1].as_ref().unwrap()[0].values;
    assert!(v[0].is_nan());
    assert!((v[1] - 1.0).abs() < 1e-10);
    assert!((v[2] - 2.0).abs() < 1e-10);
}

#[test]
fn map_fill_prev_no_prev() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![f64::NAN])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::FillPrev, None)),
    ];
    let result = interpret(&steps, &datasets);
    assert!(
        result[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("no previous value")
    );
}

#[test]
fn map_requires_arg() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![1.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::Add, None)),
    ];
    let result = interpret(&steps, &datasets);
    assert!(
        result[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("requires an argument")
    );
}

#[test]
fn group_using_sum() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("r", "us")], vec![0.0, 60.0], vec![1.0, 2.0]),
            s(&[("r", "us")], vec![0.0, 60.0], vec![3.0, 4.0]),
        ],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(group_agg(TagsType::Sum, vec!["r".into()])),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![4.0, 6.0]);
}

#[test]
fn group_all() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("h", "a")], vec![0.0], vec![1.0]),
            s(&[("h", "b")], vec![0.0], vec![3.0]),
        ],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(group_agg(TagsType::Sum, vec![])),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![4.0]);
}

#[test]
fn group_empty_error() {
    let datasets = ds("ds", "m", vec![]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(group_agg(TagsType::Sum, vec![])),
    ];
    let result = interpret(&steps, &datasets);
    assert!(
        result[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Cannot group empty")
    );
}

#[test]
fn align_sum() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(
            &[],
            vec![0.0, 30.0, 60.0, 90.0],
            vec![1.0, 2.0, 3.0, 4.0],
        )],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(align_agg(TimeType::Sum, 60)),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].timestamps, vec![0.0, 60.0]);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![3.0, 7.0]);
}

#[test]
fn align_month_error() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![1.0])]);
    let node = StepNode::Aggregate(Aggregate::Align(Align {
        function: AlignFunction::Builtin(TimeType::Sum),
        time: Parameterized::Concrete(RelativeTime {
            value: 1,
            unit: TimeUnit::Month,
        }),
    }));
    let result = interpret(&[step(source_node("ds", "m")), step(node)], &datasets);
    assert!(
        result[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Month")
    );
}

#[test]
fn as_rename() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![1.0])]);
    let node = StepNode::Aggregate(Aggregate::As(As {
        name: Metric::new("new_name").unwrap(),
    }));
    let steps = vec![step(source_node("ds", "m")), step(node)];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].name, "new_name");
}

#[test]
fn compute_not_supported() {
    let datasets: Datasets = vec![];
    let node = StepNode::Compute {
        left: vec![],
        right: vec![],
        name: "x".into(),
        op: ComputeFunction::Builtin(ComputeType::Div),
    };
    let result = interpret(&[step(node)], &datasets);
    assert!(
        result[0]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Compute")
    );
}

#[test]
fn error_step_carries_forward() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![1.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(StepNode::Error("test error".into())),
        step(map_agg(MapType::Mul, Some(2.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert!(result[1].is_err());
    assert!(
        result[1]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("test error")
    );
    assert_eq!(result[2].as_ref().unwrap()[0].values, vec![2.0]);
}

#[test]
fn interpolate_linear() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(
            &[],
            vec![0.0, 60.0, 120.0, 180.0, 240.0],
            vec![10.0, f64::NAN, f64::NAN, 40.0, 50.0],
        )],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::InterpolateLinear, None)),
    ];
    let result = interpret(&steps, &datasets);
    let v = &result[1].as_ref().unwrap()[0].values;
    assert_eq!(v[0], 10.0);
    assert!((v[1] - 20.0).abs() < 1e-10);
    assert!((v[2] - 30.0).abs() < 1e-10);
    assert_eq!(v[3], 40.0);
    assert_eq!(v[4], 50.0);
}

#[test]
fn filter_is_types() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("x", "")], vec![0.0], vec![1.0]),
            s(&[("x", "true")], vec![0.0], vec![2.0]),
            s(&[("x", "42")], vec![0.0], vec![3.0]),
            s(&[("x", "3.14")], vec![0.0], vec![4.0]),
            s(&[("x", "hello")], vec![0.0], vec![5.0]),
        ],
    );

    let check = |tag_type: TagType, expected_count: usize| {
        let filter = Filter::Cmp {
            field: "x".into(),
            rhs: Cmp::Is(tag_type),
        };
        let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
        let result = interpret(&steps, &datasets);
        assert_eq!(result[1].as_ref().unwrap().len(), expected_count);
    };

    check(TagType::None, 1);
    check(TagType::Bool, 1);
    check(TagType::Int, 1);
    check(TagType::Float, 2); // "42" and "3.14" both parse as f64
    check(TagType::String, 5);
}

#[test]
fn map_abs() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![-5.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::Abs, None)),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![5.0]);
}

#[test]
fn map_increase() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(&[], vec![0.0, 60.0, 120.0], vec![10.0, 20.0, 15.0])],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::Increase, None)),
    ];
    let result = interpret(&steps, &datasets);
    let v = &result[1].as_ref().unwrap()[0].values;
    assert!(v[0].is_nan());
    assert_eq!(v[1], 10.0);
    assert_eq!(v[2], 0.0);
}

#[test]
fn sample_filters_series() {
    let datasets = ds(
        "ds",
        "m",
        (0..20)
            .map(|i| s(&[("i", &i.to_string())], vec![0.0], vec![1.0]))
            .collect(),
    );
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Sample(0.5))];
    let result = interpret(&steps, &datasets);
    assert!(result[1].as_ref().unwrap().len() < 20);
    assert!(result[1].is_ok());
}

#[test]
fn raw_tag_variants() {
    assert_eq!(raw_tag(&TagValue::None), "");
    assert_eq!(raw_tag(&TagValue::Bool(true)), "true");
    assert_eq!(raw_tag(&TagValue::Int(42)), "42");
    assert_eq!(raw_tag(&TagValue::Float(3.14)), "3.14");
    assert_eq!(
        raw_tag(&TagValue::String(
            strumbra::SharedString::try_from("hello").unwrap()
        )),
        "hello"
    );
}

#[test]
fn time_to_seconds_all_units() {
    let rt = |unit: TimeUnit| RelativeTime { value: 1, unit };
    assert!((time_to_seconds(&rt(TimeUnit::Millisecond)).unwrap() - 0.001).abs() < 1e-10);
    assert_eq!(time_to_seconds(&rt(TimeUnit::Second)).unwrap(), 1.0);
    assert_eq!(time_to_seconds(&rt(TimeUnit::Minute)).unwrap(), 60.0);
    assert_eq!(time_to_seconds(&rt(TimeUnit::Hour)).unwrap(), 3600.0);
    assert_eq!(time_to_seconds(&rt(TimeUnit::Day)).unwrap(), 86400.0);
    assert_eq!(time_to_seconds(&rt(TimeUnit::Week)).unwrap(), 604800.0);
    assert!(time_to_seconds(&rt(TimeUnit::Month)).is_err());
    assert!(time_to_seconds(&rt(TimeUnit::Year)).is_err());
}

#[test]
fn filter_ne() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("h", "a")], vec![0.0], vec![1.0]),
            s(&[("h", "b")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "h".into(),
        rhs: Cmp::Ne(Parameterized::Concrete(TagValue::String(
            strumbra::SharedString::try_from("a").unwrap(),
        ))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![2.0]);
}

#[test]
fn filter_gt() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("code", "200")], vec![0.0], vec![1.0]),
            s(&[("code", "500")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "code".into(),
        rhs: Cmp::Gt(Parameterized::Concrete(TagValue::Int(300))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![2.0]);
}

#[test]
fn filter_ge() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("code", "300")], vec![0.0], vec![1.0]),
            s(&[("code", "500")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "code".into(),
        rhs: Cmp::Ge(Parameterized::Concrete(TagValue::Int(300))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 2);
}

#[test]
fn filter_lt() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("code", "200")], vec![0.0], vec![1.0]),
            s(&[("code", "500")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "code".into(),
        rhs: Cmp::Lt(Parameterized::Concrete(TagValue::Int(300))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![1.0]);
}

#[test]
fn filter_le() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("code", "300")], vec![0.0], vec![1.0]),
            s(&[("code", "500")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "code".into(),
        rhs: Cmp::Le(Parameterized::Concrete(TagValue::Int(300))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
}

#[test]
fn filter_regex_not() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("p", "/api")], vec![0.0], vec![1.0]),
            s(&[("p", "/health")], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "p".into(),
        rhs: Cmp::RegExNot(Parameterized::Concrete(
            EncodableRegex::new("^/api").unwrap(),
        )),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![2.0]);
}

#[test]
fn filter_and_or_not() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("a", "1"), ("b", "2")], vec![0.0], vec![10.0]),
            s(&[("a", "1"), ("b", "3")], vec![0.0], vec![20.0]),
            s(&[("a", "2"), ("b", "2")], vec![0.0], vec![30.0]),
        ],
    );
    // (a == "1" AND b == "2") => first series only
    let f_and = Filter::And(vec![
        Filter::Cmp {
            field: "a".into(),
            rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
                strumbra::SharedString::try_from("1").unwrap(),
            ))),
        },
        Filter::Cmp {
            field: "b".into(),
            rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
                strumbra::SharedString::try_from("2").unwrap(),
            ))),
        },
    ]);
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(f_and))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![10.0]);

    // a == "1" OR a == "2" => all 3
    let f_or = Filter::Or(vec![
        Filter::Cmp {
            field: "a".into(),
            rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
                strumbra::SharedString::try_from("1").unwrap(),
            ))),
        },
        Filter::Cmp {
            field: "a".into(),
            rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
                strumbra::SharedString::try_from("2").unwrap(),
            ))),
        },
    ]);
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(f_or))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 3);

    // NOT a == "1" => third series
    let f_not = Filter::Not(Box::new(Filter::Cmp {
        field: "a".into(),
        rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
            strumbra::SharedString::try_from("1").unwrap(),
        ))),
    }));
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(f_not))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![30.0]);
}

#[test]
fn filter_missing_tag_returns_false() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("a", "1")], vec![0.0], vec![1.0]),
            s(&[], vec![0.0], vec![2.0]),
        ],
    );
    let filter = Filter::Cmp {
        field: "a".into(),
        rhs: Cmp::Eq(Parameterized::Concrete(TagValue::String(
            strumbra::SharedString::try_from("1").unwrap(),
        ))),
    };
    let steps = vec![step(source_node("ds", "m")), step(StepNode::Filter(filter))];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
}

#[test]
fn map_is_lt() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![10.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::IsLt, Some(20.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![1.0, 0.0]);
}

#[test]
fn map_is_gt() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![10.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::IsGt, Some(20.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![0.0, 1.0]);
}

#[test]
fn map_is_eq() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![20.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::IsEq, Some(20.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![1.0, 0.0]);
}

#[test]
fn map_is_ne() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![20.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::IsNe, Some(20.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![0.0, 1.0]);
}

#[test]
fn map_is_ge() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![20.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::IsGe, Some(20.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![1.0, 1.0]);
}

#[test]
fn map_is_le() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![20.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::IsLe, Some(20.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![1.0, 0.0]);
}

#[test]
fn map_filter_gt() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![10.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::FilterGt, Some(15.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert!(result[1].as_ref().unwrap()[0].values[0].is_nan());
    assert_eq!(result[1].as_ref().unwrap()[0].values[1], 30.0);
}

#[test]
fn map_filter_lt() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![10.0, 30.0])]);
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::FilterLt, Some(25.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values[0], 10.0);
    assert!(result[1].as_ref().unwrap()[0].values[1].is_nan());
}

#[test]
fn map_filter_eq_ne_ge_le() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0, 60.0], vec![20.0, 30.0])]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::FilterEq, Some(20.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values[0], 20.0);
    assert!(result[1].as_ref().unwrap()[0].values[1].is_nan());

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::FilterNe, Some(20.0))),
        ],
        &datasets,
    );
    assert!(result[1].as_ref().unwrap()[0].values[0].is_nan());
    assert_eq!(result[1].as_ref().unwrap()[0].values[1], 30.0);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::FilterGe, Some(20.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values[0], 20.0);
    assert_eq!(result[1].as_ref().unwrap()[0].values[1], 30.0);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::FilterLe, Some(20.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values[0], 20.0);
    assert!(result[1].as_ref().unwrap()[0].values[1].is_nan());
}

#[test]
fn map_add_sub_div_min_max() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![10.0])]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::Add, Some(5.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![15.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::Sub, Some(3.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![7.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::Div, Some(2.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![5.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::Min, Some(5.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![5.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(map_agg(MapType::Max, Some(25.0))),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![25.0]);
}

#[test]
fn map_fill_const() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(&[], vec![0.0, 60.0], vec![f64::NAN, 5.0])],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::FillConst, Some(99.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![99.0, 5.0]);
}

#[test]
fn map_fill_prev() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(
            &[],
            vec![0.0, 60.0, 120.0],
            vec![5.0, f64::NAN, f64::NAN],
        )],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(map_agg(MapType::FillPrev, None)),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![5.0, 5.0, 5.0]);
}

#[test]
fn align_avg() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(
            &[],
            vec![0.0, 30.0, 60.0, 90.0],
            vec![10.0, 20.0, 30.0, 40.0],
        )],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(align_agg(TimeType::Avg, 60)),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].timestamps, vec![0.0, 60.0]);
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![15.0, 35.0]);
}

#[test]
fn align_rate() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(
            &[],
            vec![0.0, 10.0, 20.0, 60.0, 70.0, 80.0],
            vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0],
        )],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(align_agg(TimeType::Rate, 60)),
    ];
    let result = interpret(&steps, &datasets);
    assert!(!result[1].as_ref().unwrap()[0].values[0].is_nan());
}

#[test]
fn align_min_max_count_last() {
    let datasets = ds(
        "ds",
        "m",
        vec![s(
            &[],
            vec![0.0, 30.0, 60.0, 90.0],
            vec![5.0, 15.0, 25.0, 35.0],
        )],
    );

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(align_agg(TimeType::Min, 60)),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![5.0, 25.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(align_agg(TimeType::Max, 60)),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![15.0, 35.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(align_agg(TimeType::Count, 60)),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![2.0, 2.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(align_agg(TimeType::Last, 60)),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![15.0, 35.0]);
}

#[test]
fn group_by_tag_avg() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("r", "us")], vec![0.0], vec![10.0]),
            s(&[("r", "eu")], vec![0.0], vec![20.0]),
            s(&[("r", "us")], vec![0.0], vec![30.0]),
        ],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(group_agg(TagsType::Avg, vec!["r".into()])),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 2);
}

#[test]
fn group_count_min_max() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("h", "a")], vec![0.0], vec![10.0]),
            s(&[("h", "b")], vec![0.0], vec![20.0]),
        ],
    );

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(group_agg(TagsType::Count, vec![])),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![2.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(group_agg(TagsType::Min, vec![])),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![10.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(group_agg(TagsType::Max, vec![])),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![20.0]);

    let result = interpret(
        &[
            step(source_node("ds", "m")),
            step(group_agg(TagsType::Avg, vec![])),
        ],
        &datasets,
    );
    assert_eq!(result[1].as_ref().unwrap()[0].values, vec![15.0]);
}

#[test]
fn as_rename_series() {
    let datasets = ds("ds", "m", vec![s(&[], vec![0.0], vec![1.0])]);
    let node = StepNode::Aggregate(Aggregate::As(As {
        name: Metric::new("renamed").unwrap(),
    }));
    let steps = vec![step(source_node("ds", "m")), step(node)];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap()[0].name, "renamed");
}

fn bucket_agg(tags: Vec<String>, secs: u64, spec: Vec<BucketSpec>) -> StepNode {
    StepNode::Aggregate(Aggregate::Bucket(BucketBy {
        span: span(),
        function: BucketType::Histogram,
        time: Parameterized::Concrete(RelativeTime {
            value: secs,
            unit: TimeUnit::Second,
        }),
        tags,
        spec,
    }))
}

#[test]
fn bucket_count() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(
                &[("handler", "ingest"), ("le", "0.5")],
                vec![0.0, 60.0],
                vec![1.0, 2.0],
            ),
            s(
                &[("handler", "ingest"), ("le", "1.0")],
                vec![0.0, 60.0],
                vec![3.0, 4.0],
            ),
        ],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(bucket_agg(
            vec!["handler".into()],
            120,
            vec![BucketSpec::Count],
        )),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert!(result[1].is_ok());
}

#[test]
fn bucket_percentile() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(
                &[("handler", "ingest"), ("le", "0.5")],
                vec![0.0, 60.0],
                vec![10.0, 20.0],
            ),
            s(
                &[("handler", "ingest"), ("le", "1.0")],
                vec![0.0, 60.0],
                vec![50.0, 100.0],
            ),
        ],
    );
    let steps = vec![
        step(source_node("ds", "m")),
        step(bucket_agg(
            vec!["handler".into()],
            120,
            vec![BucketSpec::Percentile(0.9)],
        )),
    ];
    let result = interpret(&steps, &datasets);
    assert_eq!(result[1].as_ref().unwrap().len(), 1);
    assert!(result[1].is_ok());
}

#[test]
fn bucket_sum_avg_min_max() {
    let datasets = ds(
        "ds",
        "m",
        vec![
            s(&[("h", "a"), ("le", "1.0")], vec![0.0], vec![10.0]),
            s(&[("h", "a"), ("le", "2.0")], vec![0.0], vec![20.0]),
        ],
    );
    for spec in [
        BucketSpec::Sum,
        BucketSpec::Avg,
        BucketSpec::Min,
        BucketSpec::Max,
    ] {
        let steps = vec![
            step(source_node("ds", "m")),
            step(bucket_agg(vec!["h".into()], 120, vec![spec])),
        ];
        let result = interpret(&steps, &datasets);
        assert!(result[1].is_ok(), "failed for {spec:?}");
    }
}

#[test]
fn source_unknown_metric_returns_empty() {
    let datasets: Datasets = vec![];
    let result = interpret(&[step(source_node("ds", "m"))], &datasets);
    assert_eq!(result[0].as_ref().unwrap().len(), 0);
}

#[test]
fn source_error_carries_forward() {
    let datasets = ds("ds", "m", vec![]);
    let steps = vec![
        step(source_node("ds", "missing")),
        step(map_agg(MapType::Mul, Some(2.0))),
    ];
    let result = interpret(&steps, &datasets);
    assert!(result[0].is_ok());
    assert_eq!(result[1].as_ref().unwrap().len(), 0);
}

#[test]
fn spec_name_variants() {
    assert_eq!(spec_name(&BucketSpec::Count), "Count");
    assert_eq!(spec_name(&BucketSpec::Avg), "Avg");
    assert_eq!(spec_name(&BucketSpec::Sum), "Sum");
    assert_eq!(spec_name(&BucketSpec::Min), "Min");
    assert_eq!(spec_name(&BucketSpec::Max), "Max");
    assert_eq!(spec_name(&BucketSpec::Percentile(0.99)), "p99");
}
