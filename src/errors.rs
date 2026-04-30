//! Error types and diagnostics for `MPL` parsing.
#![allow(unused_assignments)] // We need this for the parse error

use std::fmt::{self, Write as _};

use miette::{Diagnostic, SourceSpan};
use pest::{
    error::{Error as PestError, ErrorVariant, InputLocation, LineColLocation},
    iterators::Pair,
};
use strsim::jaro;

use crate::parser::Rule;

/// `MPL` parsing error
#[derive(thiserror::Error, Debug, Diagnostic)]
pub enum ParseError {
    /// Syntax error with source location.
    #[error("MPL syntax error: {message}")]
    #[diagnostic(code(mpl_lang::syntax_error))]
    SyntaxError {
        /// The source location of the error with detailed message
        #[label("{label}")]
        span: SourceSpan,
        /// Short label for the inline source annotation
        label: String,
        /// The detailed error message
        message: String,
        /// Optional suggestion for fixing the error
        #[help]
        suggestion: Option<Suggestion>,
    },

    #[error("This feature is not supported at the moment: {rule:?}")]
    /// Rule for a unsupported feature
    #[diagnostic(
        code(mpl_lang::not_supported),
        help("This feature may be added in a future version")
    )]
    NotSupported {
        /// The source location of the unsupported feature
        #[label("unsupported: {rule:?}")]
        span: SourceSpan,
        /// The rule that is not supported
        rule: Rule,
    },

    /// Unexpected rule
    #[error("Unexpected rule: {rule:?} expected one of {expected:?}")]
    #[diagnostic(code(mpl_lang::unexpected_rule))]
    Unexpected {
        /// The source location of the unexpected rule
        #[label("unexpected {rule:?}")]
        span: SourceSpan,
        /// The rule that was unexpected
        rule: Rule,
        /// Expected rules
        expected: Vec<Rule>,
    },

    /// Unexpected Token
    #[error("Found unexpected tokens: {rules:?}")]
    #[diagnostic(code(mpl_lang::unexpected_tokens))]
    UnexpectedTokens {
        /// The source location of the unexpected tokens
        #[label("unexpected tokens")]
        span: SourceSpan,
        /// The unexpected rules
        rules: Vec<Rule>,
    },

    /// Unexpected EOF
    #[error("Unexpected end of input")]
    #[diagnostic(
        code(mpl_lang::unexpected_eof),
        help("The query appears to be incomplete")
    )]
    EOF {
        /// The source location where more input was expected
        #[label("expected more input here")]
        span: SourceSpan,
    },

    /// Invalid Floating point number
    #[error("Invalid float: {0}")]
    #[diagnostic(code(mpl_lang::invalid_float))]
    InvalidFloat(#[from] std::num::ParseFloatError),

    /// Invalid Integer
    #[error("Invalid integer: {0}")]
    #[diagnostic(code(mpl_lang::invalid_integer))]
    InvalidInteger(#[from] std::num::ParseIntError),

    /// Invalid bool
    #[error("Invalid bool: {0}")]
    #[diagnostic(code(mpl_lang::invalid_bool))]
    InvalidBool(#[from] std::str::ParseBoolError),

    /// Invalid date
    #[error("Invalid date: {0}")]
    #[diagnostic(code(mpl_lang::invalid_date))]
    InvalidDate(#[from] chrono::ParseError),

    /// Invalid Regex
    #[error("Invalid Regex: {0}")]
    #[diagnostic(code(mpl_lang::invalid_regex))]
    InvalidRegex(#[from] regex::Error),

    /// Unsupported align function
    #[error("Unsupported align function: {name}")]
    #[diagnostic(
        code(mpl_lang::unsupported_align_function),
        help("Check the documentation for available align functions")
    )]
    UnsupportedAlignFunction {
        /// The source location of the unsupported function
        #[label("unknown function")]
        span: SourceSpan,
        /// The name of the unsupported function
        name: String,
    },

    /// Unsupported group function
    #[error("Unsupported group function: {name}")]
    #[diagnostic(
        code(mpl_lang::unsupported_group_function),
        help("Check the documentation for available group functions")
    )]
    UnsupportedGroupFunction {
        /// The source location of the unsupported function
        #[label("unknown function")]
        span: SourceSpan,
        /// The name of the unsupported function
        name: String,
    },

    /// Unsupported compute function
    #[error("Unsupported compute function: {name}")]
    #[diagnostic(
        code(mpl_lang::unsupported_compute_function),
        help("Check the documentation for available compute functions")
    )]
    UnsupportedComputeFunction {
        /// The source location of the unsupported function
        #[label("unknown function")]
        span: SourceSpan,
        /// The name of the unsupported function
        name: String,
    },

    /// Unsupported bucketing function
    #[error("Unsupported bucket function: {name}")]
    #[diagnostic(
        code(mpl_lang::unsupported_bucket_function),
        help(
            "Available functions: histogram, interpolate_delta_histogram, interpolate_cumulative_histogram"
        )
    )]
    UnsupportedBucketFunction {
        /// The source location of the unsupported function
        #[label("unknown function")]
        span: SourceSpan,
        /// The name of the unsupported function
        name: String,
    },

    /// Unsupported map evaluation
    #[error("Unsupported map evaluation: {name}")]
    #[diagnostic(
        code(mpl_lang::unsupported_map_evaluation),
        help("Check the documentation for available map operations")
    )]
    UnsupportedMapEvaluation {
        /// The source location of the unsupported operation
        #[label("unknown operation")]
        span: SourceSpan,
        /// The name of the unsupported operation
        name: String,
    },

    /// Unsupported map function
    #[error("Unsupported map function: {name}")]
    #[diagnostic(
        code(mpl_lang::unsupported_map_function),
        help("Check the documentation for available map functions")
    )]
    UnsupportedMapFunction {
        /// The source location of the unsupported function
        #[label("unknown function")]
        span: SourceSpan,
        /// The name of the unsupported function
        name: String,
    },

    /// Unsupported regexp comparison
    #[error("Unsupported regexp comparison: {op}")]
    #[diagnostic(
        code(mpl_lang::unsupported_regexp_comparison),
        help("Use '==' or '!=' for regex comparisons")
    )]
    UnsupportedRegexpComparison {
        /// The source location of the unsupported operator
        #[label("invalid operator")]
        span: SourceSpan,
        /// The unsupported operator
        op: String,
    },

    /// Unsupported comparison against tag value
    #[error("Unsupported tag comparison: {op}")]
    #[diagnostic(
        code(mpl_lang::unsupported_tag_comparison),
        help("Supported operators: ==, !=, >, >=, <, <=")
    )]
    UnsupportedTagComparison {
        /// The source location of the unsupported operator
        #[label("invalid operator")]
        span: SourceSpan,
        /// The unsupported operator
        op: String,
    },

    /// The feature is not implemented yet
    #[error("Not implemented: {0}")]
    #[diagnostic(
        code(mpl_lang::not_implemented),
        help("This feature is planned but not yet implemented")
    )]
    NotImplemented(&'static str),

    /// Strumbra error
    #[error("String construction error: {0}")]
    #[diagnostic(code(mpl_lang::strumbra_error))]
    StrumbraError(#[from] strumbra::Error),

    /// Unreachable error
    #[error("Unreachable error: {0}")]
    #[diagnostic(
        code(mpl_lang::unreachable),
        help("This error should never be reached")
    )]
    Unreachable(&'static str),

    /// Param is defined multiple times
    #[error("The param ${param} is defined multiple times")]
    #[diagnostic(
        code(mpl_lang::param_defined_multiple_times),
        help("This param has been defined more than once")
    )]
    ParamDefinedMultipleTimes {
        /// The source location of the duplicate definition
        #[label("duplicate definition")]
        span: SourceSpan,
        /// The param
        param: String,
    },

    /// Param is not defined
    #[error("The param ${param} is not defined")]
    #[diagnostic(code(mpl_lang::undefined_param))]
    UndefinedParam {
        /// The source location of the undefine param
        #[label("undefined param")]
        span: SourceSpan,
        /// The param
        param: String,
    },
    /// Invalid tag type
    #[error("The type {tpe} is not a valid type for tags")]
    #[diagnostic(code(mpl_lang::invalid_tag_type))]
    InvalidTagType {
        /// The source location of the invalid type
        #[label("invalid type")]
        span: miette::SourceSpan,
        /// The invalid type
        tpe: String,
    },
}

impl From<PestError<Rule>> for ParseError {
    fn from(err: PestError<Rule>) -> Self {
        let (start, mut len) = match err.location {
            InputLocation::Pos(pos) => (pos, 0),
            InputLocation::Span((start, end)) => (start, end - start),
        };

        let (label, message, suggestion) = match &err.variant {
            ErrorVariant::ParsingError {
                positives,
                negatives,
            } => {
                let mut keywords = Vec::new();
                let mut operations = Vec::new();
                let mut other = Vec::new();

                for rule in positives {
                    let name = friendly_rule(*rule);
                    if name.contains("keyword") {
                        keywords.push(name);
                    } else if name.contains("operation") {
                        operations.push(name);
                    } else {
                        other.push(name);
                    }
                }

                let mut label = String::new();
                if keywords.is_empty() && operations.is_empty() && other.is_empty() {
                    label.push_str("unexpected token");
                } else {
                    label.push_str("expected one of:\n");
                    if !keywords.is_empty() {
                        let kws: Vec<_> = keywords
                            .iter()
                            .map(|k| k.trim_end_matches(" keyword"))
                            .collect();
                        let _ = writeln!(label, "  keywords: {}", join_with_or(&kws));
                    }
                    if !operations.is_empty() {
                        let ops: Vec<_> = operations
                            .iter()
                            .map(|o| {
                                o.trim_start_matches("a ")
                                    .trim_start_matches("an ")
                                    .trim_end_matches(" operation")
                            })
                            .collect();
                        let _ = writeln!(label, "  operations: {}", join_with_or(&ops));
                    }
                    if !other.is_empty() {
                        for name in &other {
                            let _ = writeln!(label, "  - {name}");
                        }
                    }
                }

                let mut msg = "unexpected token or operation".to_string();
                if !negatives.is_empty() {
                    if !msg.is_empty() {
                        msg.push_str("  ");
                    }
                    msg.push_str("but found ");
                    msg.push_str(&friendly_rules(negatives));
                }

                let line_pos = match &err.line_col {
                    LineColLocation::Pos((_, col)) | LineColLocation::Span((_, col), _) => {
                        col.saturating_sub(1)
                    }
                };
                let suggestion = generate_suggestion(err.line(), line_pos, positives);

                // If the span is a single position, try to expand it to cover the full token
                if len == 0 {
                    len = token_length(err.line(), line_pos);
                }

                let label = label.trim_end().to_string();
                (label, msg, suggestion)
            }
            ErrorVariant::CustomError { message } => (message.clone(), message.clone(), None),
        };

        ParseError::SyntaxError {
            span: SourceSpan::new(start.into(), len),
            label,
            message,
            suggestion,
        }
    }
}

/// Join a list of items with commas and "or" before the last item
fn join_with_or(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        2 => format!("{} or {}", items[0], items[1]),
        _ => {
            let last = items[items.len() - 1];
            let rest = &items[..items.len() - 1];
            format!("{}, or {last}", rest.join(", "))
        }
    }
}

/// Convert a Pest `Pair` span to a miette `SourceSpan`
pub(crate) fn pair_to_source_span(pair: &Pair<Rule>) -> SourceSpan {
    let span = pair.as_span();
    let start = span.start();
    let len = span.end() - start;
    SourceSpan::new(start.into(), len)
}

/// Convert a list of rules to a friendly name
fn friendly_rules(rules: &[Rule]) -> String {
    let names: Vec<_> = rules.iter().copied().map(friendly_rule).collect();

    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        2 => format!("{} or {}", names[0], names[1]),
        _ => {
            let last = &names[names.len() - 1];
            let rest = &names[..names.len() - 1];
            format!("{}, or {last}", rest.join(", "))
        }
    }
}

/// Convert a rule to a friendly name
fn friendly_rule(rule: Rule) -> String {
    match rule {
        // Control
        Rule::EOI => "end of query".to_string(),
        Rule::pipe_keyword => "`|` (pipe)".to_string(),

        // Time
        Rule::time_range => "time range (e.g.,  [1h..])".to_string(),
        Rule::time_relative => "relative time (e.g., 5m, 1h, 7d)".to_string(),
        Rule::time_timestamp => "timestamp".to_string(),
        Rule::time_rfc_3339 => "RFC3339 timestamp".to_string(),
        Rule::time_modifier => "time modifier".to_string(),

        // Keywords
        Rule::filter_keyword | Rule::kw_filter => "`filter` keyword".to_string(),
        Rule::kw_where => "`where` keyword".to_string(),
        Rule::r#as => "`as` keyword".to_string(),

        // Ops
        Rule::cmp => "a comparison operator (==, !=, <, >, <=, >=)".to_string(),
        Rule::cmp_re => "a regex operator (==, !=)".to_string(),
        Rule::regex => "a regex pattern (e.g., /pattern/)".to_string(),

        // Values
        Rule::value => "value (string, number, or bool)".to_string(),
        Rule::string => "string value".to_string(),
        Rule::number => "number".to_string(),
        Rule::bool => "bool (true or false)".to_string(),

        // Idents
        Rule::plain_ident => "identifier".to_string(),
        Rule::escaped_ident => "escaped identifier".to_string(),
        Rule::source => "source metric".to_string(),
        Rule::metric_name => "metric name".to_string(),
        Rule::metric_id => "metric identifier (e.g., dataset:metric)".to_string(),
        Rule::dataset => "dataset name".to_string(),

        // Aggrs
        Rule::align => "an align operation".to_string(),
        Rule::group_by => "a group by operation".to_string(),
        Rule::bucket_by => "a bucket by operation".to_string(),
        Rule::map => "a map operation".to_string(),
        Rule::replace => "a replace operation".to_string(),
        Rule::join => "a join operation".to_string(),

        // Query types
        Rule::simple_query => "simple query".to_string(),
        Rule::compute_query => "compute query".to_string(),

        // Directives
        Rule::directive => "directive".to_string(),

        // Params
        Rule::param => "param".to_string(),
        Rule::param_ident => "param identifier".to_string(),
        Rule::param_type => {
            "param type (Duration, Dataset, Regex, string, int, float, bool)".to_string()
        }

        // Funs
        Rule::func => "function".to_string(),
        Rule::compute_fn => "compute function".to_string(),
        Rule::bucket_by_fn => {
            "bucket function (histogram, interpolate_delta_histogram)".to_string()
        }
        Rule::bucket_by_with_conversion_fn => {
            "bucket function (interpolate_cumulative_histogram)".to_string()
        }
        Rule::bucket_conversion => "conversion method (rate, increase)".to_string(),
        Rule::bucket_specs => "bucket specifications".to_string(),
        Rule::bucket_fn_call | Rule::bucket_fn_call_simple => "bucket function call".to_string(),
        Rule::bucket_fn_call_with_conversion => "bucket function call with conversion".to_string(),

        // Filters
        Rule::filter_rule => "filter rule".to_string(),
        Rule::filter_expr => "filter expression".to_string(),
        Rule::sample_expr => "sample expression".to_string(),
        Rule::value_filter => "value filter".to_string(),
        Rule::regex_filter => "regex filter".to_string(),
        Rule::kw_is => "`is` keyword".to_string(),
        Rule::is_filter => "type filter (e.g., is string)".to_string(),
        Rule::tag_type => "tag type (string, int, float, or bool)".to_string(),

        // Tags
        Rule::tags => "tags (comma-separated field names)".to_string(),
        Rule::tag => "tag name".to_string(),

        // Fallback for any other rules
        _ => {
            let name = format!("{rule:?}");
            name.to_lowercase().replace('_', " ")
        }
    }
}

/// Suggestion for typos / corrections
#[derive(Debug, Clone)]
pub struct Suggestion(String);

impl Suggestion {
    /// The suggested text
    #[must_use]
    pub fn suggestion(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Suggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Did you mean \"{}\"?", self.0)
    }
}

/// Generate a suggestion for a typo based on the expected rules
fn generate_suggestion(
    line: &str,
    error_pos: usize,
    expected_rules: &[Rule],
) -> Option<Suggestion> {
    let actual_token = extract_token(line, error_pos)?;

    if actual_token.len() < 2 {
        return None;
    }

    let possible_keywords = rules_keywords(expected_rules);

    let mut best_match: Option<(&str, f64)> = None;

    for keyword in &possible_keywords {
        let similarity = jaro(&actual_token.to_lowercase(), &keyword.to_lowercase());

        if similarity > 0.8 {
            if let Some((_, best_score)) = best_match {
                if similarity > best_score {
                    best_match = Some((keyword, similarity));
                }
            } else {
                best_match = Some((keyword, similarity));
            }
        }
    }

    best_match.map(|(keyword, _)| Suggestion(keyword.to_string()))
}

/// Extract the token at the given position from the line
fn extract_token(line: &str, pos: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();

    if pos >= chars.len() {
        return None;
    }

    // Skip whitespace forward to find the next token
    let mut pos = pos;
    while pos < chars.len() && chars[pos].is_whitespace() {
        pos += 1;
    }

    if pos >= chars.len() {
        return None;
    }

    // Find the start of the token (go backwards)
    let mut start = pos;
    while start > 0 && chars[start - 1].is_alphanumeric() {
        start -= 1;
    }

    // Find the end of the token (go forwards)
    let mut end = pos;
    while end < chars.len() && chars[end].is_alphanumeric() {
        end += 1;
    }

    if start < end {
        Some(chars[start..end].iter().collect())
    } else {
        None
    }
}

/// Extract the length of the token at the given position
fn token_length(line: &str, pos: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();

    if pos >= chars.len() {
        return 0;
    }

    if !chars[pos].is_alphanumeric() {
        return 1;
    }

    let mut end = pos;
    while end < chars.len() && chars[end].is_alphanumeric() {
        end += 1;
    }

    end - pos
}

/// Get a list of common keywords that correspond to a list of rules
fn rules_keywords(rules: &[Rule]) -> Vec<&'static str> {
    let mut keywords = Vec::new();

    for rule in rules {
        match rule {
            Rule::filter_keyword | Rule::kw_filter | Rule::kw_where => {
                keywords.push("where");
                keywords.push("filter");
            }
            Rule::r#as => keywords.push("as"),
            Rule::align => keywords.push("align"),
            Rule::group_by => keywords.push("group"),
            Rule::bucket_by => keywords.push("bucket"),
            Rule::map => keywords.push("map"),
            Rule::replace => keywords.push("replace"),
            Rule::join => keywords.push("join"),
            Rule::kw_is => keywords.push("is"),
            Rule::tag_type => {
                keywords.push("string");
                keywords.push("int");
                keywords.push("float");
                keywords.push("bool");
            }
            _ => {}
        }
    }

    keywords
}
