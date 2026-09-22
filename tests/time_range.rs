use std::collections::HashMap;

use serde_json::{Value, json};

/// inline ranges survive compilation and formatting, including when an alias or an operator follows the source.
#[test]
fn source_time_range_contract() {
    for (range, expected) in [
        ("", Value::Null),
        (
            "[0..2]",
            json!({"start": {"Timestamp": 0}, "end": {"Timestamp": 2}}),
        ),
        ("[120..]", json!({"start": {"Timestamp": 120}, "end": null})),
        (
            "[2h..5m]",
            json!({"start": {"Relative": {"value": 7200, "unit": "Second"}}, "end": {"Relative": {"value": 300, "unit": "Second"}}}),
        ),
    ] {
        for suffix in [
            "",
            " as usage",
            " | align to 1s using last",
            " as usage | align to 1s using last",
        ] {
            let source = format!("test:cpu{range}{suffix}");
            let (query, _) = mpl_lang::compile(&source, HashMap::new())
                .unwrap_or_else(|error| panic!("{source}: {error:?}"));
            assert_eq!(
                serde_json::to_value(query.time_range()).unwrap(),
                expected,
                "{source}"
            );
            let formatted = query.to_string();
            let (reparsed, _) = mpl_lang::compile(&formatted, HashMap::new())
                .unwrap_or_else(|error| panic!("{formatted}: {error:?}"));
            assert_eq!(
                serde_json::to_value(query).unwrap(),
                serde_json::to_value(reparsed).unwrap(),
                "{source}"
            );
        }
    }
}

/// source ranges either resolve or return an error at the responsible stage; invalid input must not unwind.
#[cfg(feature = "clock")]
#[test]
fn source_time_resolution_contract() {
    for (range, expected) in [
        ("[0..2]", Ok(())),
        ("[2h..5m]", Ok(())),
        ("[120..]", Ok(())),
        ("[9223372036854775807y..]", Err("parse")),
        ("[9223372036854775807s..]", Err("time")),
        ("[1000000y..]", Err("time")),
        ("[0..9223372036854775807]", Err("time")),
    ] {
        let source = format!("test:cpu{range} | align to 1s using last");
        let actual = mpl_lang::compile(&source, HashMap::new())
            .map_err(|_| "parse")
            .and_then(|(query, _)| {
                query
                    .time_range()
                    .expect("source range")
                    .to_start_end()
                    .map(|_| ())
                    .map_err(|_| "time")
            });
        assert_eq!(actual, expected, "{source}");
    }
}

/// each public time unit preserves its scale and rejects values beyond the duration representation.
#[test]
fn relative_time_conversion_contract() {
    use mpl_lang::query::{RelativeTime, TimeUnit};

    for (unit, millis) in [
        (TimeUnit::Millisecond, 1),
        (TimeUnit::Second, 1000),
        (TimeUnit::Minute, 60_000),
        (TimeUnit::Hour, 3_600_000),
        (TimeUnit::Day, 86_400_000),
        (TimeUnit::Week, 604_800_000),
        (TimeUnit::Month, 2_592_000_000),
        (TimeUnit::Year, 31_536_000_000),
    ] {
        let mut time = RelativeTime { value: 1, unit };
        assert_eq!(
            time.to_duration()
                .expect("one time unit")
                .num_milliseconds(),
            millis
        );
        time.value = i64::MAX as u64;
        assert_eq!(
            time.to_duration().is_ok(),
            time.unit == TimeUnit::Millisecond
        );
        time.value = u64::MAX;
        assert!(time.to_duration().is_err());
    }
}
