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

    let parsed = crate::compile(query)?;
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
