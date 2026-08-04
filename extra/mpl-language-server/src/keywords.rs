//! Documentation for MPL's keywords, shared by completion and hover.

use serde::Serialize;

/// What a keyword means, and how it is written.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct KeywordInfo {
    pub label: &'static str,
    pub description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax: Option<&'static str>,
}

const fn kw(label: &'static str, description: &'static str) -> KeywordInfo {
    KeywordInfo {
        label,
        description,
        syntax: None,
    }
}

const fn kw_syntax(
    label: &'static str,
    description: &'static str,
    syntax: &'static str,
) -> KeywordInfo {
    KeywordInfo {
        label,
        description,
        syntax: Some(syntax),
    }
}

const KEYWORDS: &[KeywordInfo] = &[
    kw_syntax(
        "where",
        "Filter time series by tag values",
        "| where <tag> == <value>",
    ),
    // `where` is the canonical spelling; the lint offers to rewrite `filter`.
    kw_syntax(
        "filter",
        "Filter time series by tag values (deprecated alias for `where`)",
        "| filter <tag> == <value>",
    ),
    kw_syntax(
        "sample",
        "Sample time series at a numeric rate",
        "| sample <rate>",
    ),
    kw_syntax(
        "map",
        "Apply a function to each data point",
        "| map <function>",
    ),
    kw_syntax(
        "group",
        "Group time series by tags and aggregate",
        "| group by <tags> using <function>",
    ),
    kw_syntax(
        "align",
        "Align time series to a regular time grid",
        "| align to <interval> using <function>",
    ),
    kw_syntax(
        "bucket",
        "Bucket time series into histogram buckets",
        "| bucket by <tags> to <interval> using <function>(<specs>)",
    ),
    kw_syntax(
        "compute",
        "Compute a new metric from two sources",
        "| compute <metric> using <function>",
    ),
    kw_syntax("as", "Rename the output metric", "| as <name>"),
    kw_syntax(
        "extend",
        "Add new constant-valued tags to every series after aggregation. Each tag must be \
         net-new for the query — a series that already carries the tag causes the query to \
         fail. Only constant values (strings, numbers, booleans, or scalar params) are \
         supported.",
        "| extend <tag> = <value>, ...",
    ),
    kw_syntax(
        "ifdef",
        "Conditionally apply a filter when an optional param is supplied. The body is dropped \
         when the param is omitted; an optional `else` branch applies a different filter in \
         that case.",
        "| ifdef($param) { where <filter-expr> } [else { where <else-filter-expr> }]",
    ),
    kw_syntax(
        "else",
        "Optional companion to `ifdef`: applies a filter when the gating optional param is \
         *not* supplied. Only valid immediately after an `ifdef(...) { ... }` block.",
        "| ifdef($param) { ... } else { where <filter-expr> }",
    ),
    kw_syntax(
        "set",
        "Set query directives (time range, resolution)",
        "set <directive> = <value>;",
    ),
    kw_syntax(
        "param",
        "Declare a query parameter",
        "param $<name>: <type>;",
    ),
    kw_syntax(
        "Option",
        "Wraps a param type to mark it optional. Optional params can only be referenced \
         inside an ifdef gating on them.",
        "param $name: Option<T>;",
    ),
    kw("by", "Specify tags for grouping or bucketing"),
    kw("using", "Specify the function to apply"),
    kw("to", "Specify target time interval for align or bucket"),
    kw("and", "Logical AND in filter expressions"),
    kw("or", "Logical OR in filter expressions"),
    kw("not", "Logical NOT in filter expressions"),
    kw_syntax("is", "Check a tag's type", "<tag> is <type>"),
    kw_syntax(
        "in",
        "Membership in an array of values",
        "<tag> in [<value>, ...]",
    ),
];

/// The documentation for `label`, or `None` when it names no keyword.
#[must_use]
pub fn keyword_info(label: &str) -> Option<KeywordInfo> {
    KEYWORDS.iter().copied().find(|k| k.label == label)
}

/// The description for `label`, for building a completion item.
///
/// Panics in debug builds on an undocumented keyword; `every_offered_keyword_is_documented`
/// in the completion tests is what keeps that from happening in release.
#[must_use]
pub(crate) fn describe(label: &str) -> &'static str {
    debug_assert!(
        keyword_info(label).is_some(),
        "keyword `{label}` has no documentation"
    );
    keyword_info(label).map_or("", |k| k.description)
}

#[cfg(test)]
mod tests {
    use super::{KEYWORDS, keyword_info};

    #[test]
    fn labels_are_unique() {
        let mut seen: Vec<&str> = KEYWORDS.iter().map(|k| k.label).collect();
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), count, "duplicate keyword entry");
    }

    #[test]
    fn every_entry_is_reachable_by_its_label() {
        for entry in KEYWORDS {
            assert_eq!(
                keyword_info(entry.label).map(|k| k.label),
                Some(entry.label)
            );
        }
        assert!(keyword_info("nonsense").is_none());
    }

    #[test]
    fn descriptions_are_present() {
        for entry in KEYWORDS {
            assert!(
                !entry.description.trim().is_empty(),
                "{} has an empty description",
                entry.label
            );
        }
    }
}
