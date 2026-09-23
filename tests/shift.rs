use std::{collections::HashMap, convert::Infallible};

use mpl_lang::{
    Query,
    query::Aggregate,
    visitor::{QueryVisitor, QueryWalker, VisitRes},
};

fn round_trip(source: &str) -> Query {
    let (query, warnings) = mpl_lang::compile(source, HashMap::new())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    assert!(warnings.into_vec().is_empty(), "{source}");
    let formatted = query.to_string();
    let (reparsed, _) = mpl_lang::compile(&formatted, HashMap::new())
        .unwrap_or_else(|error| panic!("{formatted}: {error:?}"));
    assert_eq!(
        serde_json::to_value(&query).unwrap(),
        serde_json::to_value(reparsed).unwrap(),
        "{source}"
    );
    query
}

#[test]
fn signed_shift_literal_contract() {
    for (literal, seconds) in [
        ("-1h", -3600),
        ("+1h", 3600),
        ("1h", 3600),
        ("- 1h", -3600),
        ("+ 1h", 3600),
        ("0s", 0),
        ("-0ms", 0),
        ("+0y", 0),
        ("-2000ms", -2),
        ("1000ms", 1),
        ("-2s", -2),
        ("+2m", 120),
        ("-1d", -86400),
        ("1w", 604800),
        ("1M", 2592000),
        ("-1y", -31536000),
        ("9223372036854775807s", i64::MAX),
        ("-9223372036854775808s", i64::MIN),
        ("9223372036854775807000ms", i64::MAX),
        ("-9223372036854775808000ms", i64::MIN),
        ("153722867280912930m", 9223372036854775800),
        ("-153722867280912930m", -9223372036854775800),
    ] {
        let query = round_trip(&format!("test:cpu[120..240] | shift {literal}"));
        let Query::Simple { aggregates, .. } = query else {
            panic!("expected a simple query");
        };
        assert!(
            matches!(aggregates.as_slice(), [Aggregate::Shift { seconds: value }] if *value == seconds)
        );
        assert_eq!(aggregates[0].to_string(), format!("| shift {seconds}s"));
        assert_eq!(
            serde_json::to_value(&aggregates[0]).unwrap(),
            serde_json::json!({"Shift": {"seconds": seconds}})
        );
    }
}

#[test]
fn shift_preserves_pipeline_position_and_compute_structure() {
    for source in [
        "test:cpu as usage | where service == 1 | shift -1h | align to 60s using last | shift +1m | map abs | extend env = 1",
        "(test:cpu | shift -1h, test:cpu | shift +1h) | compute delta using - | shift -1d | align to 60s using last",
    ] {
        round_trip(source);
    }
    let Query::Simple { aggregates, .. } =
        round_trip("test:cpu | align to 60s using last | shift -1h | map abs | shift +1m")
    else {
        panic!("expected a simple query");
    };
    assert!(matches!(
        aggregates.as_slice(),
        [
            Aggregate::Align(_),
            Aggregate::Shift { seconds: -3600 },
            Aggregate::Map(_),
            Aggregate::Shift { seconds: 60 },
        ]
    ));
}

#[test]
fn shift_rejects_overflow_fractional_seconds_and_nonliteral_offsets() {
    for literal in [
        "9223372036854775808s",
        "-9223372036854775809s",
        "153722867280912931m",
        "-153722867280912931m",
        "9223372036854775808000ms",
        "-9223372036854775809000ms",
        "170141183460469231731687303715884105727y",
        "999999999999999999999999999999999999999999999999999s",
        "1ms",
        "-1ms",
        "1500ms",
        "-1500ms",
        "1.5s",
        "-0.5s",
        "1.5h",
        "1e3s",
        "1",
        "1ns",
        "$offset",
        "+-1h",
        "--1h",
        "",
    ] {
        let source = format!("test:cpu | shift {literal}");
        assert!(
            mpl_lang::compile(&source, HashMap::new()).is_err(),
            "accepted {source}"
        );
    }
    for source in [
        "param $offset: Duration; test:cpu | shift $offset",
        "test:cpu | shift -1h | where service == 1",
        "test:cpu | shift -1h | sample 0.5",
    ] {
        assert!(
            mpl_lang::compile(source, HashMap::new()).is_err(),
            "accepted {source}"
        );
    }
}

#[test]
fn shift_visitor_walk_and_stop_contract() {
    struct Visitor {
        stop: bool,
        seen: Vec<i64>,
        left: Vec<i64>,
    }
    impl QueryVisitor for Visitor {
        type Error = Infallible;

        fn visit_shift(&mut self, seconds: &mut i64) -> Result<VisitRes, Self::Error> {
            self.seen.push(*seconds);
            *seconds += 1;
            Ok(if self.stop {
                VisitRes::Stop
            } else {
                VisitRes::Walk
            })
        }

        fn leave_shift(&mut self, seconds: &mut i64) -> Result<(), Self::Error> {
            self.left.push(*seconds);
            Ok(())
        }
    }
    impl QueryWalker for Visitor {}

    for stop in [false, true] {
        let mut query = round_trip(
            "(test:cpu | shift -1h, test:cpu | shift +1h) | compute delta using - | shift 0s",
        );
        let mut visitor = Visitor {
            stop,
            seen: Vec::new(),
            left: Vec::new(),
        };
        visitor.walk(&mut query).unwrap();
        assert_eq!(visitor.seen, [-3600, 3600, 0]);
        assert_eq!(
            visitor.left,
            if stop { vec![] } else { vec![-3599, 3601, 1] }
        );
        let Query::Compute { aggregates, .. } = query else {
            panic!("expected a compute query");
        };
        assert!(matches!(
            aggregates.as_slice(),
            [Aggregate::Shift { seconds: 1 }]
        ));
    }
}
