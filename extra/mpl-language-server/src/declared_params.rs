//! Extraction of the parameters a query declares inline (`param $x: int;`).
//!
//! Distinct from the *system* params a host injects (see [`crate::system_params`]):
//! these are the `param` declarations written in the query source itself. Hosts
//! use this to discover which parameters a saved query expects so they can
//! prompt for or bind values before execution.
//!
//! Built on the same tolerant preamble scan that powers completions
//! ([`crate::completions::extract_declared_params`]), so partial / not-yet-valid
//! queries still report the params they have declared.

use serde::Serialize;

use crate::completions::{ParamItem, extract_declared_params};

/// A single parameter declared in a query, in a JS-friendly shape.
///
/// `name` has no leading `$`, and `type` uses the source-level spelling
/// (`Dataset`, `Metric`, `Duration`, `Regex`, `string`, `int`, `float`,
/// `bool`) — the same vocabulary users write in the query and that
/// [`crate::SystemParamSpec`] accepts. Optionals are flagged via `optional`
/// rather than wrapped in `Option<...>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclaredParam {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub optional: bool,
}

impl From<&ParamItem> for DeclaredParam {
    fn from(item: &ParamItem) -> DeclaredParam {
        DeclaredParam {
            name: item.label.trim_start_matches('$').to_string(),
            type_name: item.typ.spelling().to_string(),
            optional: item.optional,
        }
    }
}

/// Returns the parameters declared inline in `query`, with their types.
///
/// Scans the query preamble for `param $name: type;` declarations, tolerating
/// directives, comments, and an incomplete query body. Declarations with an
/// unrecognised type are skipped.
#[must_use]
pub fn declared_params(query: &str) -> Vec<DeclaredParam> {
    extract_declared_params(query)
        .iter()
        .map(DeclaredParam::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_declared_params_with_types() {
        let query = "\
param $dataset: Dataset;
param $age: int;
param $name: Option<string>;
param $hosts: array;
param $codes: Option<array>;
$dataset:metric";
        assert_eq!(
            declared_params(query),
            vec![
                DeclaredParam {
                    name: "dataset".to_string(),
                    type_name: "Dataset".to_string(),
                    optional: false,
                },
                DeclaredParam {
                    name: "age".to_string(),
                    type_name: "int".to_string(),
                    optional: false,
                },
                DeclaredParam {
                    name: "name".to_string(),
                    type_name: "string".to_string(),
                    optional: true,
                },
                DeclaredParam {
                    name: "hosts".to_string(),
                    type_name: "array".to_string(),
                    optional: false,
                },
                DeclaredParam {
                    name: "codes".to_string(),
                    type_name: "array".to_string(),
                    optional: true,
                },
            ]
        );
    }

    #[test]
    fn no_params_yields_empty_vec() {
        assert!(declared_params("my_dataset:my_metric").is_empty());
    }

    #[test]
    fn tolerates_incomplete_query_body() {
        // The body is unfinished, but the declared param is still reported —
        // matching the editor's need to surface params while typing.
        assert_eq!(
            declared_params("param $tag: Regex;\n$tag:metric | filter "),
            vec![DeclaredParam {
                name: "tag".to_string(),
                type_name: "Regex".to_string(),
                optional: false,
            }]
        );
    }
}
