//! Decoding for the `system_params` argument shared by `diagnostics()` and
//! `completions()`.
//!
//! Hosts (e.g. a query service injecting `$__interval`) tell the language
//! server about externally-supplied parameters so the editor stops flagging
//! them as undeclared and starts suggesting them.
//!
//! The wire format mirrors the source-level param syntax users already see in
//! their queries (`Dataset`, `Duration`, `Regex`, `string`, `int`, `float`,
//! `bool`) rather than the lowercase completion-internal representation —
//! a host registering `{ name: "__interval", type: "Duration" }` matches what
//! they would have written had they declared the param inline.
use std::collections::HashMap;

use serde::Deserialize;
use wasm_bindgen::JsValue;

use crate::query::{ParamType, TagType, TerminalParamType};

use super::completions::{ParamItem, ParamType as CompletionParamType};

/// Wire-format entry for a single system-supplied parameter. The `type` field
/// uses source-level spellings so registrations read like the language they
/// shadow.
#[derive(Debug, Deserialize)]
pub(super) struct SystemParamSpec {
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) type_name: String,
    #[serde(default)]
    pub(super) optional: bool,
}

impl SystemParamSpec {
    /// Maps the source-level type string to a query-level `ParamType`,
    /// or returns `None` for unknown spellings (silently dropped — invalid
    /// host registrations must not break the editor).
    fn to_query_param_type(&self) -> Option<ParamType> {
        let terminal = parse_terminal(&self.type_name)?;
        Some(if self.optional {
            ParamType::Optional(terminal)
        } else {
            ParamType::Terminal(terminal)
        })
    }

    /// Maps the spec to the completion-side `ParamItem` used to render
    /// the `$param` autocomplete list.
    fn to_completion_item(&self) -> Option<ParamItem> {
        let typ = parse_completion_type(&self.type_name)?;
        Some(ParamItem {
            label: ensure_dollar_prefix(&self.name),
            typ,
            optional: self.optional,
        })
    }
}

fn parse_terminal(s: &str) -> Option<TerminalParamType> {
    match s {
        "Dataset" => Some(TerminalParamType::Dataset),
        // `duration` is the legacy lowercase form; accept it for symmetry
        // with the in-source param syntax even though it triggers an
        // OldDuration warning when written in a query.
        "Duration" | "duration" => Some(TerminalParamType::Duration),
        "Regex" => Some(TerminalParamType::Regex),
        "string" => Some(TerminalParamType::Tag(TagType::String)),
        "int" => Some(TerminalParamType::Tag(TagType::Int)),
        "float" => Some(TerminalParamType::Tag(TagType::Float)),
        "bool" => Some(TerminalParamType::Tag(TagType::Bool)),
        _ => None,
    }
}

fn parse_completion_type(s: &str) -> Option<CompletionParamType> {
    match s {
        "Dataset" => Some(CompletionParamType::Dataset),
        "Duration" | "duration" => Some(CompletionParamType::Duration),
        "Regex" => Some(CompletionParamType::Regex),
        "string" => Some(CompletionParamType::String),
        "int" => Some(CompletionParamType::Int),
        "float" => Some(CompletionParamType::Float),
        "bool" => Some(CompletionParamType::Bool),
        _ => None,
    }
}

/// Param labels in completion results are dollar-prefixed (`$__interval`);
/// hosts may pass names with or without the leading `$`, so normalise here.
fn ensure_dollar_prefix(name: &str) -> String {
    if name.starts_with('$') {
        name.to_string()
    } else {
        format!("${name}")
    }
}

/// Decodes a `JsValue` (expected: array of `SystemParamSpec`, or `null`/
/// `undefined`/missing) into a `Vec`. Bad shapes degrade to empty rather than
/// throwing — diagnostics must never disappear because the host shipped a
/// malformed registration.
pub(super) fn decode(value: JsValue) -> Vec<SystemParamSpec> {
    if value.is_null() || value.is_undefined() {
        return Vec::new();
    }
    serde_wasm_bindgen::from_value::<Vec<SystemParamSpec>>(value).unwrap_or_default()
}

/// Builds the `HashMap` passed to `compile()`. Entries with unknown types are
/// dropped. Name-prefix validation (`__`) is left to the parser, which surfaces
/// `SystemParamMissingPrefix` as a diagnostic the host can act on.
pub(super) fn to_compile_params(specs: &[SystemParamSpec]) -> HashMap<String, ParamType> {
    specs
        .iter()
        .filter_map(|s| s.to_query_param_type().map(|t| (s.name.clone(), t)))
        .collect()
}

/// Builds the `ParamItem` list spliced into `compute_completions`'s declared-
/// param set. Same drop-unknown semantics as `to_compile_params`.
pub(super) fn to_completion_items(specs: &[SystemParamSpec]) -> Vec<ParamItem> {
    specs
        .iter()
        .filter_map(SystemParamSpec::to_completion_item)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Specs without going through the JS bridge — exercises only the
    // type-string decode and the dollar-prefix normalisation.
    fn spec(name: &str, type_name: &str, optional: bool) -> SystemParamSpec {
        SystemParamSpec {
            name: name.to_string(),
            type_name: type_name.to_string(),
            optional,
        }
    }

    #[test]
    fn to_compile_params_maps_all_terminal_types() {
        let specs = [
            spec("__a", "Dataset", false),
            spec("__b", "Duration", false),
            spec("__c", "Regex", false),
            spec("__d", "string", false),
            spec("__e", "int", false),
            spec("__f", "float", false),
            spec("__g", "bool", false),
        ];
        let map = to_compile_params(&specs);
        assert_eq!(map.len(), 7);
        assert!(matches!(
            map["__a"],
            ParamType::Terminal(TerminalParamType::Dataset)
        ));
        assert!(matches!(
            map["__d"],
            ParamType::Terminal(TerminalParamType::Tag(TagType::String))
        ));
    }

    #[test]
    fn to_compile_params_wraps_optional() {
        let specs = [spec("__a", "string", true)];
        let map = to_compile_params(&specs);
        assert!(matches!(
            map["__a"],
            ParamType::Optional(TerminalParamType::Tag(TagType::String))
        ));
    }

    #[test]
    fn unknown_type_strings_are_dropped() {
        // Bad type spelling must not poison the rest of the registration —
        // valid entries still flow through.
        let specs = [spec("__a", "Bogus", false), spec("__b", "Duration", false)];
        let map = to_compile_params(&specs);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("__b"));

        // The completion-side decode must drop the same unknown entry,
        // so a misspelt type doesn't leak into the autocomplete dropdown.
        let items = to_completion_items(&specs);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "$__b");
    }

    #[test]
    fn legacy_lowercase_duration_accepted() {
        // Source-level `duration` is a legacy spelling; the system_params
        // API accepts it for symmetry, mapping to TerminalParamType::Duration.
        let specs = [spec("__t", "duration", false)];
        let map = to_compile_params(&specs);
        assert!(matches!(
            map["__t"],
            ParamType::Terminal(TerminalParamType::Duration)
        ));
    }

    #[test]
    fn completion_items_normalise_dollar_prefix() {
        // Some hosts will pass names with `$`, others without. Completion
        // labels must always carry the prefix for the autocomplete UI.
        let specs = [
            spec("__a", "Duration", false),
            spec("$__b", "Duration", false),
        ];
        let items = to_completion_items(&specs);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"$__a"));
        assert!(labels.contains(&"$__b"));
    }

    #[test]
    fn completion_items_carry_optional_flag() {
        let specs = [spec("__x", "string", true)];
        let items = to_completion_items(&specs);
        assert_eq!(items.len(), 1);
        assert!(items[0].optional);
    }

    #[test]
    fn completion_items_cover_every_supported_type() {
        // Pin the completion-side type mapping for every accepted spelling.
        // Catches drift between the query-level `parse_terminal` and the
        // completion-side `parse_completion_type` — they must stay aligned
        // so a registration produces matching diagnostics + completions.
        let specs = [
            spec("__a", "Dataset", false),
            spec("__b", "Duration", false),
            spec("__c", "Regex", false),
            spec("__d", "string", false),
            spec("__e", "int", false),
            spec("__f", "float", false),
            spec("__g", "bool", false),
            spec("__h", "duration", false),
        ];
        let items = to_completion_items(&specs);
        assert_eq!(
            items.len(),
            specs.len(),
            "every supported type must produce a completion item"
        );
        let by_label: std::collections::HashMap<&str, &super::ParamItem> =
            items.iter().map(|i| (i.label.as_str(), i)).collect();
        assert_eq!(by_label["$__a"].typ, CompletionParamType::Dataset);
        assert_eq!(by_label["$__b"].typ, CompletionParamType::Duration);
        assert_eq!(by_label["$__c"].typ, CompletionParamType::Regex);
        assert_eq!(by_label["$__d"].typ, CompletionParamType::String);
        assert_eq!(by_label["$__e"].typ, CompletionParamType::Int);
        assert_eq!(by_label["$__f"].typ, CompletionParamType::Float);
        assert_eq!(by_label["$__g"].typ, CompletionParamType::Bool);
        assert_eq!(
            by_label["$__h"].typ,
            CompletionParamType::Duration,
            "legacy lowercase `duration` must map to Duration on the completion side too"
        );
    }
}
