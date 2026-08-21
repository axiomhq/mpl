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
//!
//! `Timestamp` extends that vocabulary to the query window a runtime hands the
//! engine as `$__start` and `$__end`. The runtime binds those itself, so they
//! arrive as registrations rather than as a type a `param` line spells, and a
//! query reads them back through string interpolation.
use std::collections::HashMap;

use serde::Deserialize;

use mpl_lang::query::{ParamType, TagType, TerminalParamType};

use crate::completions::{ParamItem, ParamType as CompletionParamType};

/// Wire-format entry for a single system-supplied parameter. The `type` field
/// uses source-level spellings so registrations read like the language they
/// shadow.
#[derive(Debug, Deserialize)]
pub struct SystemParamSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub optional: bool,
}

impl SystemParamSpec {
    /// Maps the source-level type string to a query-level `ParamType`,
    /// or returns `None` for unknown spellings (silently dropped — invalid
    /// host registrations must not break the editor).
    fn to_query_param_type(&self) -> Option<ParamType> {
        let terminal = registered_type(&self.type_name)?.into();
        Some(if self.optional {
            ParamType::Optional(terminal)
        } else {
            ParamType::Terminal(terminal)
        })
    }

    /// Maps the spec to the completion-side `ParamItem` used to render
    /// the `$param` autocomplete list.
    fn to_completion_item(&self) -> Option<ParamItem> {
        let typ = registered_type(&self.type_name)?;
        Some(ParamItem {
            label: ensure_dollar_prefix(&self.name),
            typ,
            optional: self.optional,
        })
    }
}

/// The spelling a `Timestamp` registration arrives with, pinned to the query
/// type's own `Display` by `the_timestamp_spelling_matches_the_query_type`.
const TIMESTAMP_SPELLING: &str = "Timestamp";

/// The type a registration's `type` string names.
///
/// Declarable spellings resolve through the editor's type table, the same one
/// completion offers inside a `param` declaration. `Timestamp` is the runtime's
/// to bind, so it resolves against its own spelling here.
fn registered_type(spelling: &str) -> Option<CompletionParamType> {
    if spelling == TIMESTAMP_SPELLING {
        return Some(CompletionParamType::Timestamp);
    }
    CompletionParamType::from_spelling(spelling)
}

/// The editor keeps its own param type so completion results can travel as the
/// flat `{ type, optional }` pair the TypeScript side reads. This is where that
/// type meets the query-level one, and the mapping is total: every type the
/// editor offers names a type the parser accepts.
impl From<CompletionParamType> for TerminalParamType {
    fn from(typ: CompletionParamType) -> TerminalParamType {
        match typ {
            CompletionParamType::Dataset => TerminalParamType::Dataset,
            CompletionParamType::Duration => TerminalParamType::Duration,
            CompletionParamType::Regex => TerminalParamType::Regex,
            CompletionParamType::String => TerminalParamType::Tag(TagType::String),
            CompletionParamType::Int => TerminalParamType::Tag(TagType::Int),
            CompletionParamType::Float => TerminalParamType::Tag(TagType::Float),
            CompletionParamType::Bool => TerminalParamType::Tag(TagType::Bool),
            CompletionParamType::Array => TerminalParamType::Tag(TagType::Array),
            CompletionParamType::Timestamp => TerminalParamType::Timestamp,
        }
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

/// Builds the `HashMap` passed to `compile()`. Entries with unknown types are
/// dropped. Name-prefix validation (`__`) is left to the parser, which surfaces
/// `SystemParamMissingPrefix` as a diagnostic the host can act on.
pub fn to_compile_params(specs: &[SystemParamSpec]) -> HashMap<String, ParamType> {
    specs
        .iter()
        .filter_map(|s| s.to_query_param_type().map(|t| (s.name.clone(), t)))
        .collect()
}

/// Builds the `ParamItem` list spliced into `compute_completions`'s declared-
/// param set. Same drop-unknown semantics as `to_compile_params`.
pub fn to_completion_items(specs: &[SystemParamSpec]) -> Vec<ParamItem> {
    specs
        .iter()
        .filter_map(SystemParamSpec::to_completion_item)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The editor's spelling and the query type's `Display` are two renderings
    /// of one vocabulary. A host registration is written in the first and
    /// resolved through the second, so the two have to agree character for
    /// character or a registration silently fails to bind.
    #[test]
    fn spellings_agree_with_the_query_types() {
        for typ in crate::completions::PARAM_TYPES {
            assert_eq!(
                typ.spelling(),
                TerminalParamType::from(typ).to_string(),
                "{typ:?}"
            );
        }
    }

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

    /// The wire spelling and the query type's `Display` are one vocabulary, so
    /// a registration written as `Timestamp` has to name the type the compiler
    /// resolves it to. `spellings_agree_with_the_query_types` pins the same
    /// property for every type the editor offers.
    #[test]
    fn the_timestamp_spelling_matches_the_query_type() {
        assert_eq!(
            TIMESTAMP_SPELLING,
            TerminalParamType::Timestamp.to_string(),
            "the registration spelling names the query type"
        );
    }

    /// The window a runtime supplies arrives as two `Timestamp` registrations.
    /// Both have to reach `compile`, or the editor reports `$__start` and
    /// `$__end` as undeclared in a query the runtime executes happily.
    #[test]
    fn timestamp_registrations_reach_the_compiler() {
        let specs = [
            spec("__start", "Timestamp", false),
            spec("__end", "Timestamp", false),
        ];
        let map = to_compile_params(&specs);
        assert_eq!(map.len(), 2);
        assert!(matches!(
            map["__start"],
            ParamType::Terminal(TerminalParamType::Timestamp)
        ));
        assert!(matches!(
            map["__end"],
            ParamType::Terminal(TerminalParamType::Timestamp)
        ));
    }

    /// A runtime binds `Timestamp`, so the type table completion offers inside
    /// a `param` declaration stays the set of types a `param` line may spell.
    /// The registration still reaches the `$param` list, so a query can read
    /// the window back where an expression is accepted.
    #[test]
    fn a_timestamp_registration_completes_without_being_declarable() {
        assert_eq!(
            CompletionParamType::from_spelling(TIMESTAMP_SPELLING),
            None,
            "the declarable-type table names the types a `param` line spells"
        );

        let items = to_completion_items(&[spec("__start", "Timestamp", false)]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "$__start");
        assert_eq!(items[0].typ, CompletionParamType::Timestamp);
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
    }
}
