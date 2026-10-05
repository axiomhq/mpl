use std::collections::HashMap;

use itertools::iproduct;
use mpl_lang::{
    CompileError, Query, compile,
    parser::ParseError,
    query::Aggregate,
    time::{Offset, Timerange, Timestamp},
    types::Parameterized,
};
use test_case::test_case;

const OFFSETS: &[i64] = &[-i64::MAX, -3600, -1, 0, 1, 3600, i64::MAX];
const TIMESTAMPS: &[u64] = &[0, 1, 3600, (1 << 63) - 1, 1 << 63, u64::MAX - 1, u64::MAX];

fn source_offsets(query: &Query) -> Vec<Option<i64>> {
    match query {
        Query::Simple { source, .. } => vec![source.offset.map(Offset::as_secs)],
        Query::Compute { left, right, .. } => {
            let mut offsets = source_offsets(left);
            offsets.extend(source_offsets(right));
            offsets
        }
    }
}

fn assert_round_trip(query: &Query) {
    let text = query.to_string();
    let (reparsed, _) =
        compile(&text, HashMap::new()).unwrap_or_else(|error| panic!("{text}: {error:?}"));
    assert_eq!(
        serde_json::to_value(query).unwrap(),
        serde_json::to_value(reparsed).unwrap(),
        "{text}"
    );
}

fn assert_bad_rule(text: &str, rule: &str) {
    let Err(CompileError::Parser(errors)) = compile(text, HashMap::new()) else {
        panic!("expected a rule error for {text}");
    };
    let [ParseError::RuleNotSupportedHere { span }] = errors.as_slice() else {
        panic!("{text}: {errors:?}");
    };
    assert_eq!(text[span.offset()..span.offset() + span.len()].trim(), rule);
}

#[test]
fn an_offset_moves_both_ends() {
    for (&a, &b, offset) in iproduct!(
        TIMESTAMPS,
        TIMESTAMPS,
        OFFSETS.iter().copied().chain([i64::MIN])
    ) {
        let (start, end) = (a.min(b), a.max(b));
        let range = Timerange::new(Timestamp(start), Timestamp(end)).unwrap();
        let offseted = range.offset(Offset::secs(offset));
        let expected_start = i128::from(start) + i128::from(offset);
        let expected_end = i128::from(end) + i128::from(offset);
        assert_eq!(
            offseted.is_ok(),
            expected_start >= 0 && expected_end <= i128::from(u64::MAX),
            "{range:?}, offset {offset}"
        );
        if let Ok(offseted) = offseted {
            assert_eq!(i128::from(offseted.start().as_secs()), expected_start);
            assert_eq!(i128::from(offseted.end().as_secs()), expected_end);
            assert_eq!(offseted.duration(), range.duration());
        }
    }
}

#[test_case(""; "unsigned")]
#[test_case("+"; "plus")]
#[test_case("-"; "minus")]
#[test_case("--"; "repeated minus")]
#[test_case("-+"; "minus plus")]
#[test_case("+-"; "plus minus")]
#[test_case("++"; "repeated plus")]
fn offset_uses_the_existing_duration_rules(sign: &str) {
    for (value, unit) in iproduct!(
        [
            "0",
            "1",
            "999",
            "1000",
            "1001",
            "3600",
            "1.5",
            "\"1\"",
            "9223372036854775808"
        ],
        ["ms", "s", "m", "h", "d", "w", "M", "y", "", "ns", "h1m"]
    )
    // Minute values just below and above i64::MAX seconds.
    .chain([("153722867280912930", "m"), ("153722867280912931", "m")])
    {
        let duration = format!("{value}{unit}");
        let offseted = compile(
            &format!("test:cpu | offset {sign}{duration}"),
            HashMap::new(),
        );
        let aligned = compile(
            &format!("test:cpu | align to {duration} using last"),
            HashMap::new(),
        );
        if let Ok((aligned, align_warnings)) = aligned {
            let Query::Simple { aggregates, .. } = aligned else {
                unreachable!()
            };
            let Aggregate::Align(align) = &aggregates[0] else {
                unreachable!()
            };
            let Some(Parameterized::Concrete(time)) = &align.time else {
                unreachable!()
            };
            let seconds = i64::try_from(time.value);
            assert_eq!(
                offseted.is_ok(),
                matches!(sign, "" | "+" | "-") && seconds.is_ok(),
                "{sign}{duration}"
            );
            if let Ok((offseted, offset_warnings)) = offseted {
                assert_eq!(
                    source_offsets(&offseted),
                    vec![Some(if sign == "-" {
                        -seconds.unwrap()
                    } else {
                        seconds.unwrap()
                    })],
                    "{duration}"
                );
                assert_eq!(
                    offset_warnings.into_vec().len(),
                    align_warnings.into_vec().len()
                );
                assert_round_trip(&offseted);
            }
        } else {
            assert!(offseted.is_err(), "{sign}{duration}");
        }
    }
}

#[test]
fn offset_comes_first_and_only_once() {
    for (offset_, rule) in iproduct!(
        OFFSETS,
        [
            "sample 0.5",
            "where service == 1",
            "map abs",
            "align using last",
            "group using sum",
            "bucket using histogram(0.5)",
            "extend env = 1",
            "as cpu_alias",
        ]
    ) {
        let offset = format!("offset {offset_}s");
        let source = format!("test:cpu | {offset}");
        let (query, _) = compile(&format!("{source} | {rule}"), HashMap::new()).unwrap();
        assert_eq!(source_offsets(&query), vec![Some(*offset_)]);
        assert_bad_rule(&format!("test:cpu | {rule} | {offset}"), &offset);
        assert_bad_rule(&format!("{source} | {offset}"), &offset);
    }
}

#[test]
fn each_source_keeps_its_own_offset() {
    let sources: Vec<_> = std::iter::once(None)
        .chain(OFFSETS.iter().copied().map(Some))
        .map(|offset| {
            (
                offset.map_or_else(
                    || "test:cpu".to_owned(),
                    |s| format!("test:cpu | offset {s}s"),
                ),
                offset,
            )
        })
        .collect();
    for ((left, a), (right, b)) in iproduct!(&sources, &sources) {
        let pair = format!("({left}, {right}) | compute delta using -");
        for (text, expected) in [
            (left.clone(), vec![*a]),
            (pair.clone(), vec![*a, *b]),
            (
                format!("({pair}, {left}) | compute delta using -"),
                vec![*a, *b, *a],
            ),
            (
                format!("({right}, {pair}) | compute delta using -"),
                vec![*b, *a, *b],
            ),
        ] {
            let (query, _) = compile(&text, HashMap::new()).unwrap();
            assert_eq!(source_offsets(&query), expected, "{text}");
            assert_round_trip(&query);
            if matches!(query, Query::Compute { .. }) {
                assert_bad_rule(&format!("{text} | offset -1s"), "offset -1s");
            }
        }
    }
}
