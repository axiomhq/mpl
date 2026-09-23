use std::collections::HashMap;

use mpl_lang::query::SpotlightReducer;

#[test]
fn spotlight_compiles_and_round_trips() {
    for fields in [
        "*",
        "service, `cloud.region`",
        "`using`",
        "`true`, `false`, `null`, `inf`",
    ] {
        for reducer in ["sum", "avg"] {
            let source = format!(
                "test:cpu | align to 60s using last | extend env = 1 | spotlight [120..240] against [0..120] by {fields} using {reducer} limit 20"
            );
            let (query, _) = mpl_lang::compile(&source, HashMap::new()).unwrap();
            let spotlight = query.spotlight().unwrap();
            assert_eq!(spotlight.comparison.start().as_secs(), 120);
            assert_eq!(spotlight.baseline.end().as_secs(), 120);
            assert_eq!(spotlight.limit, 20);
            assert_eq!(spotlight.fields.is_none(), fields == "*");
            assert_eq!(
                spotlight.reducer,
                if reducer == "sum" {
                    SpotlightReducer::Sum
                } else {
                    SpotlightReducer::Avg
                }
            );
            let formatted = query.to_string();
            let (reparsed, _) = mpl_lang::compile(&formatted, HashMap::new()).unwrap();
            assert_eq!(
                serde_json::to_value(query).unwrap(),
                serde_json::to_value(reparsed).unwrap()
            );
        }
    }
}

#[test]
fn spotlight_defaults_and_duplicate_fields() {
    let (query, warnings) = mpl_lang::compile(
        "test:cpu | spotlight [120..240] against [0..120] by service, service using avg",
        HashMap::new(),
    )
    .unwrap();
    let spotlight = query.spotlight().unwrap();
    assert_eq!(spotlight.limit, 10);
    assert_eq!(
        spotlight.fields.as_deref(),
        Some(["service".to_string()].as_slice())
    );
    assert!(!warnings.into_vec().is_empty());
}

#[test]
fn shift_can_precede_but_not_follow_spotlight() {
    let spotlight = "spotlight [120..240] against [0..120] by * using sum";
    let (query, _) = mpl_lang::compile(
        &format!("test:cpu | shift -1h | {spotlight}"),
        HashMap::new(),
    )
    .unwrap();
    assert!(query.spotlight().is_some());
    let (reparsed, _) = mpl_lang::compile(&query.to_string(), HashMap::new()).unwrap();
    assert_eq!(
        serde_json::to_value(query).unwrap(),
        serde_json::to_value(reparsed).unwrap()
    );
    assert!(
        mpl_lang::compile(
            &format!("test:cpu | {spotlight} | shift -1h"),
            HashMap::new(),
        )
        .is_err()
    );
}

#[test]
fn spotlight_rejects_invalid_queries() {
    for operation in [
        "spotlight [120..120] against [0..120] by * using sum",
        "spotlight [120..240] against [60..180] by * using sum",
        "spotlight [240..120] against [0..120] by * using sum",
        "spotlight [120..240] against [0..120] by * using max",
        "spotlight [120..240] against [0..120] by * using sum limit 0",
        "spotlight [120..240] against [0..120] by * using sum limit 201",
        "spotlight [120..240] against [0..120] by * using sum | map abs",
        "spotlight [120..240] against [0..120] by * using sum | extend a = 1",
        "spotlight [120..240] against [0..120] by * using sum | as other",
        "spotlight [120..240] against [0..120] by * using sum | spotlight [120..240] against [0..120] by * using avg",
        "spotlight [120..] against [0..120] by * using sum",
        "spotlight [-120..240] against [0..120] by * using sum",
        "spotlight [120..240] against [0..120] by using sum",
        "spotlight [120..240] against [0..120] by * using sum(1)",
    ] {
        assert!(
            mpl_lang::compile(&format!("test:cpu | {operation}"), HashMap::new()).is_err(),
            "accepted {operation}"
        );
    }
}

#[test]
fn spotlight_is_a_result_not_a_compute_operand() {
    let operand = "test:cpu | spotlight [120..240] against [0..120] by * using sum";
    assert!(
        mpl_lang::compile(
            &format!("({operand}, test:cpu) | compute ratio using /"),
            HashMap::new()
        )
        .is_err()
    );
    let query = "(test:cpu | align to 60s using last, test:cpu | align to 60s using last) | compute ratio using / | spotlight [120..240] against [0..120] by * using avg";
    assert!(
        mpl_lang::compile(query, HashMap::new())
            .unwrap()
            .0
            .spotlight()
            .is_some()
    );
}
