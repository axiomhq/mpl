use super::*;

#[test]
fn simple_query() {
    let steps = parse_steps("test:metric").unwrap();
    assert_eq!(steps.len(), 1);
    assert!(matches!(steps[0].node, StepNode::Source(_)));
    assert_eq!(steps[0].canonical, "`test`:`metric`");
}

#[test]
fn filter_and_aggregate() {
    let steps = parse_steps("test:metric\n| where code >= 500\n| group using sum").unwrap();
    assert_eq!(steps.len(), 3);
    assert!(matches!(steps[0].node, StepNode::Source(_)));
    assert!(matches!(steps[1].node, StepNode::Filter(_)));
    assert!(matches!(steps[2].node, StepNode::Aggregate(_)));
}

#[test]
fn unknown_function_produces_error_step() {
    let steps =
        parse_steps("test:metric\n| align to 5m using unknown_fn\n| group using sum").unwrap();
    assert_eq!(steps.len(), 3);
    assert!(matches!(steps[0].node, StepNode::Source(_)));
    assert!(matches!(steps[1].node, StepNode::Error(_)));
    assert!(matches!(steps[2].node, StepNode::Aggregate(_)));
}

#[test]
fn unsupported_replace_produces_error_step() {
    let steps =
        parse_steps("test:metric\n| replace x = y ~ #s/foo/bar/\n| group using sum").unwrap();
    assert!(
        steps
            .iter()
            .any(|s| matches!(&s.node, StepNode::Error(msg) if msg.contains("not supported")))
    );
    assert!(
        steps
            .iter()
            .any(|s| matches!(s.node, StepNode::Aggregate(_)))
    );
}

#[test]
fn unsupported_join_produces_error_step() {
    let steps =
        parse_steps("test:metric\n| join x from test:other by y\n| group using sum").unwrap();
    assert!(
        steps
            .iter()
            .any(|s| matches!(&s.node, StepNode::Error(msg) if msg.contains("not supported")))
    );
}

#[test]
fn syntax_error_throws() {
    assert!(parse_steps("test:metric\n| blahblah").is_err());
}

#[test]
fn compute_query() {
    let steps = parse_steps("(\n  test:a,\n  test:b\n)\n| compute ratio using /").unwrap();
    assert_eq!(steps.len(), 1);
    assert!(matches!(steps[0].node, StepNode::Compute { .. }));
}

#[test]
fn compute_with_post_pipes() {
    let steps =
        parse_steps("(\n  test:a,\n  test:b\n)\n| compute ratio using /\n| group using sum")
            .unwrap();
    assert_eq!(steps.len(), 2);
    assert!(matches!(steps[0].node, StepNode::Compute { .. }));
    assert!(matches!(steps[1].node, StepNode::Aggregate(_)));
}

#[test]
fn sample_step() {
    let steps = parse_steps("test:metric\n| sample 0.5\n| group using sum").unwrap();
    assert_eq!(steps.len(), 3);
    assert!(matches!(steps[1].node, StepNode::Sample(v) if (v - 0.5).abs() < f64::EPSILON));
}

#[test]
fn directives_and_params() {
    let steps = parse_steps(
        "param $ds: dataset;\nparam $dur: duration;\n$ds:metric\n| align to $dur using avg",
    )
    .unwrap();
    assert_eq!(steps.len(), 2);
    assert!(matches!(steps[0].node, StepNode::Source(_)));
}

#[test]
fn canonical_has_no_comments() {
    let steps = parse_steps("// comment\ntest:metric\n// another\n| group using sum").unwrap();
    assert!(!steps[0].canonical.contains("//"));
    assert!(!steps[1].canonical.contains("//"));
}

#[test]
fn source_with_as() {
    let steps = parse_steps("`com.app.test`:ingest_pressure as cake").unwrap();
    assert_eq!(steps.len(), 2);
    assert!(matches!(steps[0].node, StepNode::Source(_)));
    assert!(matches!(
        steps[1].node,
        StepNode::Aggregate(Aggregate::As(_))
    ));
}

#[test]
fn map_operations() {
    let s = parse_steps("test:metric\n| map rate").unwrap();
    assert!(matches!(s[1].node, StepNode::Aggregate(_)));

    let s = parse_steps("test:metric\n| map * 5").unwrap();
    assert!(matches!(s[1].node, StepNode::Aggregate(_)));

    let s = parse_steps("test:metric\n| map is::lt(100)").unwrap();
    assert!(matches!(s[1].node, StepNode::Aggregate(_)));
}

#[test]
fn align_prom_rate() {
    let steps = parse_steps("test:metric\n| align to 5m using prom::rate").unwrap();
    assert!(matches!(steps[1].node, StepNode::Aggregate(_)));
}

#[test]
fn bucket_histogram() {
    let steps = parse_steps(
        "test:metric\n| bucket by method, path to 5m using interpolate_delta_histogram(0.90, max, 0.99)",
    )
    .unwrap();
    assert!(matches!(steps[1].node, StepNode::Aggregate(_)));
}

#[test]
fn set_directives() {
    let steps = parse_steps("set strict;\nset x = 42;\ntest:metric").unwrap();
    assert_eq!(steps.len(), 1);
    assert!(matches!(steps[0].node, StepNode::Source(_)));
}

#[test]
fn spans_are_correct() {
    let input = "test:metric\n| where code >= 500\n| group using sum";
    let steps = parse_steps(input).unwrap();
    for step in steps.iter() {
        let end = step.span.offset() + step.span.len();
        assert!(end <= input.len());
    }
}

#[test]
fn display_source() {
    let steps = parse_steps("test:metric").unwrap();
    assert_eq!(format!("{}", steps[0].node), "`test`:`metric`");
}

#[test]
fn display_filter() {
    let steps = parse_steps("test:metric\n| where code >= 500").unwrap();
    let s = format!("{}", steps[1].node);
    assert!(s.starts_with("| where"));
}

#[test]
fn display_aggregate() {
    let steps = parse_steps("test:metric\n| group using sum").unwrap();
    let s = format!("{}", steps[1].node);
    assert!(s.contains("group"));
}

#[test]
fn display_sample() {
    let node = StepNode::Sample(0.5);
    assert_eq!(format!("{node}"), "| sample 0.5");
}

#[test]
fn display_error() {
    let node = StepNode::Error("bad".into());
    assert_eq!(format!("{node}"), "/* error: bad */");
}

#[test]
fn display_compute() {
    let steps = parse_steps("(\n  test:a,\n  test:b\n)\n| compute ratio using /").unwrap();
    let s = format!("{}", steps[0].node);
    assert!(s.contains("compute"));
    assert!(s.contains("ratio"));
}

#[test]
fn source_with_unresolved_param_produces_error() {
    let steps = parse_steps("$missing:metric").unwrap();
    assert!(steps.iter().any(|s| matches!(&s.node, StepNode::Error(_))));
}

#[test]
fn filter_with_unresolved_param_produces_error() {
    let steps = parse_steps("test:metric\n| where x == $missing").unwrap();
    assert!(steps.iter().any(|s| matches!(&s.node, StepNode::Error(_))));
}

#[test]
fn nested_compute_left() {
    let input = "(\n  (\n    test:a,\n    test:b\n  )\n  | compute inner using /,\n  test:c\n)\n| compute outer using *";
    let steps = parse_steps(input).unwrap();
    assert_eq!(steps.len(), 1);
    if let StepNode::Compute { left, .. } = &steps[0].node {
        assert_eq!(left.len(), 1);
        assert!(matches!(left[0].node, StepNode::Compute { .. }));
    } else {
        panic!("expected compute");
    }
}

#[test]
fn nested_compute_right() {
    let input = "(\n  test:a,\n  (\n    test:b,\n    test:c\n  )\n  | compute inner using /\n)\n| compute outer using *";
    let steps = parse_steps(input).unwrap();
    assert_eq!(steps.len(), 1);
    if let StepNode::Compute { right, .. } = &steps[0].node {
        assert_eq!(right.len(), 1);
        assert!(matches!(right[0].node, StepNode::Compute { .. }));
    } else {
        panic!("expected compute");
    }
}

#[test]
fn compute_with_post_pipe_error() {
    let steps = parse_steps(
        "(\n  test:a,\n  test:b\n)\n| compute ratio using /\n| align to 5m using unknown_fn",
    )
    .unwrap();
    assert_eq!(steps.len(), 2);
    assert!(matches!(steps[0].node, StepNode::Compute { .. }));
    assert!(matches!(steps[1].node, StepNode::Error(_)));
}
