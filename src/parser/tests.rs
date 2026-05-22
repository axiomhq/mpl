use std::collections::HashMap;

use ordered_float::OrderedFloat;
use pest::Parser;

use super::*;

#[test]
fn test_relative_time() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut parse = MPLParser::parse(Rule::time, "1h")?;
    assert_eq!(parse.len(), 1);
    assert_eq!(parse.as_str(), "1h");
    let next = parse.next().expect("EOF");
    assert_eq!(next.as_rule(), Rule::time_relative);
    let mut inner = next.into_inner();
    assert_eq!(inner.len(), 2);
    assert_eq!(inner.as_str(), "1h");
    let next = inner.next().expect("EOF");
    assert_eq!(next.as_rule(), Rule::time_unit_digits);
    assert_eq!(next.as_str(), "1");
    let next = inner.next().expect("EOF");
    assert_eq!(next.as_rule(), Rule::time_unit_hour);
    assert_eq!(next.as_str(), "h");
    Ok(())
}
#[test]
fn test_timestamp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut parse = MPLParser::parse(Rule::time, "11233145")?;
    assert_eq!(parse.len(), 1);
    assert_eq!(parse.as_str(), "11233145");
    let next = parse.next().expect("EOF");
    assert_eq!(next.as_rule(), Rule::time_timestamp);
    assert_eq!(next.as_str(), "11233145");

    Ok(())
}

#[test]
fn test_number() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut parse = MPLParser::parse(Rule::number, "123")?;
    match parse_number(parse.next().expect("EOF"))? {
        Number::Int(i) => assert_eq!(i, 123),
        Number::Float(_) => panic!("Expected integer"),
    }
    Ok(())
}

#[test]
fn test_number_float() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut parse = MPLParser::parse(Rule::number, "123.456")?;
    match parse_number(parse.next().expect("EOF"))? {
        Number::Float(f) => assert_eq!(OrderedFloat(f), OrderedFloat(123.456)),
        Number::Int(_) => panic!("Expected float"),
    }
    Ok(())
}

#[test]
fn test_compute_query_post_compute_aggregates()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let query = "
    (
        test:metric_a[30m..]
        | align to 1m using sum,
        test:metric_b[30m..]
        | align to 1m using sum,
    )
    | compute result using /
    | map * 100
    | align to 5m using last
    ";

    let (parsed, _) = crate::compile(query, HashMap::new())?;
    let Query::Compute { aggregates, .. } = &parsed else {
        panic!("expected Query::Compute, got {parsed:?}");
    };

    assert!(
        matches!(&aggregates[0], Aggregate::Map(_)),
        "first aggregate should be Map, got {:?}",
        aggregates[0]
    );
    assert!(
        matches!(&aggregates[1], Aggregate::Align(_)),
        "second aggregate should be Align, got {:?}",
        aggregates[1]
    );

    Ok(())
}
#[test]
fn optional_ok() {
    let query = "
        param $t: Option<string>;
        dataset:metric
        | ifdef($t) { where tag == $t }
    ";

    assert!(crate::compile(query, HashMap::new()).is_ok());
}

#[test]
fn optional_use_without_ifdef() {
    let query = "
        param $t: Option<string>;
        dataset:metric
        | where tag == $t
    ";
    assert!(crate::compile(query, HashMap::new()).is_err());
}

#[test]
fn optional_ok_with_else_branch() {
    // Regression: `parse_where_part` used to `assert_empty()` after the
    // if-branch, which rejected any trailing `kw_else` tokens even though
    // the grammar accepted them. This test pins the round-trip: parsed
    // query must compile, and the canonical Display must include the
    // `else { ... }` clause.
    let query = "
        param $t: Option<string>;
        dataset:metric
        | ifdef($t) { where tag == $t } else { where tag == \"default\" }
    ";
    let (q, _) = crate::compile(query, HashMap::new()).expect("ifdef with else should compile");
    let rendered = q.to_string();
    assert!(
        rendered.contains("} else { where "),
        "canonical form should preserve the else branch, got:\n{rendered}"
    );
}

#[test]
fn optional_ifdef_else_without_param_reference_in_either_branch_errors() {
    // Symmetric to `optional_ok`: when the gating param is referenced in
    // *neither* branch, `OptionalNotUsed` still fires. The else branch is
    // walked by the visitor too, so this proves the walker reaches it.
    let query = "
        param $t: Option<string>;
        dataset:metric
        | ifdef($t) { where tag == \"a\" } else { where tag == \"b\" }
    ";
    assert!(crate::compile(query, HashMap::new()).is_err());
}

#[test]
fn optional_ifdef_else_with_param_reference_in_else_branch_only_ok() {
    // The visitor's `seen_param` is set if *either* branch references the
    // gating param. An else-only reference is enough to satisfy the
    // `OptionalNotUsed` check.
    let query = "
        param $t: Option<string>;
        dataset:metric
        | ifdef($t) { where tag == \"a\" } else { where tag == $t }
    ";
    assert!(crate::compile(query, HashMap::new()).is_ok());
}

#[test]
fn optional_ifdef_without_optional() {
    let query = "
        param $t: string;
        dataset:metric
        | ifdef($t) { where tag == $t }
    ";

    assert!(crate::compile(query, HashMap::new()).is_err());
}
