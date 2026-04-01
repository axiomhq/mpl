//! Playground step-based parser.
//!
//! Parses an MPL query into a flat list of [`PipeStep`]s for the playground UI.
//! Each step carries a source span, a display label (via `Display`), and either
//! a parsed AST node or a parse error. Error recovery happens at `|` boundaries:
//! if a pipe fails to parse, the error is recorded and parsing continues.

use std::fmt::{self, Display};

use miette::SourceSpan;
use pest::Parser as _;
use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::{
    errors::pair_to_source_span,
    linker::ComputeFunction,
    parser::{MPLParser, Rule},
    query::{Aggregate, Directives, Filter, Params, Source},
};

/// A single pipeline step for the playground.
#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct PipeStep {
    /// Byte range in the source text.
    #[tsify(type = "{ offset: number, length: number }")]
    pub span: SourceSpan,
    /// Canonical display text for this step.
    pub label: String,
    /// The parsed node, if successful.
    pub node: Option<StepNode>,
    /// Parse error message, if the step failed.
    pub error: Option<String>,
}

/// The AST node for a successfully parsed step.
#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum StepNode {
    /// The data source (`dataset:metric [timerange]`).
    Source(Source),
    /// A filter clause (`| where/filter expr`).
    Filter(Filter),
    /// An aggregate pipe (`| map/align/group/bucket/as`).
    Aggregate(Aggregate),
    /// A sample clause (`| sample 0.5`).
    Sample(f64),
    /// A compute query (`(q1, q2) | compute name using fn`).
    Compute {
        /// Steps for the left sub-query.
        left: Vec<PipeStep>,
        /// Steps for the right sub-query.
        right: Vec<PipeStep>,
        /// Output metric name.
        name: String,
        /// The compute function.
        op: ComputeFunction,
    },
}

impl Display for StepNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepNode::Source(s) => write!(f, "{s}"),
            StepNode::Filter(fl) => write!(f, "| where {fl}"),
            StepNode::Aggregate(a) => write!(f, "{a}"),
            StepNode::Sample(v) => write!(f, "| sample {v}"),
            StepNode::Compute {
                left,
                right,
                name,
                op,
                ..
            } => {
                writeln!(f, "(")?;
                for step in left {
                    writeln!(f, "  {}", step.label)?;
                }
                writeln!(f, ",")?;
                for step in right {
                    writeln!(f, "  {}", step.label)?;
                }
                write!(f, ")\n| compute {name} using {op}")
            }
        }
    }
}

/// Result of step-based parsing.
#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct ParseStepsResult {
    /// The pipeline steps.
    pub steps: Vec<PipeStep>,
}

/// Parses an MPL query into pipeline steps with error recovery.
#[wasm_bindgen]
pub fn parse_steps(query: &str) -> Result<ParseStepsResult, String> {
    let pairs = MPLParser::parse(Rule::file, query).map_err(|e| e.to_string())?;
    let parser = crate::parser::Parser::default();

    let mut steps = Vec::new();
    let mut directives = Directives::default();
    let mut params = Params::default();

    let mut iter = pairs.into_iter().peekable();

    // Parse directives and params
    while let Some(pair) = iter.peek() {
        match pair.as_rule() {
            Rule::directive => {
                let pair = iter.next().expect("peeked");
                match crate::parser::Parser::parse_directive(pair) {
                    Ok((k, v)) => {
                        directives.insert(k, v);
                    }
                    Err(_) => {
                        // Directive parse failure — skip
                    }
                }
            }
            Rule::param => {
                let pair = iter.next().expect("peeked");
                match crate::parser::Parser::parse_param(&params, pair) {
                    Ok(p) => params.push(p),
                    Err(_) => {
                        // Param parse failure — skip
                    }
                }
            }
            _ => break,
        }
    }

    // Next should be the query (simple_query or compute_query)
    if let Some(query_pair) = iter.next() {
        match query_pair.as_rule() {
            Rule::simple_query => {
                parse_simple_steps(&parser, query_pair, query, &params, &mut steps);
            }
            Rule::compute_query => {
                parse_compute_steps(&parser, query_pair, query, &directives, &params, &mut steps);
            }
            Rule::EOI => {}
            _ => {}
        }
    }

    // Remaining pairs (post-query) — shouldn't happen with well-formed input
    for pair in iter {
        if pair.as_rule() == Rule::EOI {
            break;
        }
    }

    Ok(ParseStepsResult { steps })
}

fn parse_simple_steps(
    parser: &crate::parser::Parser,
    query_pair: pest::iterators::Pair<Rule>,
    source_text: &str,
    params: &Params,
    steps: &mut Vec<PipeStep>,
) {
    let mut pairs = query_pair.into_inner();

    // Source
    if let Some(source_pair) = pairs.next() {
        let span = pair_to_source_span(&source_pair);
        match crate::parser::parse_source(source_pair, params) {
            Ok((source, as_)) => {
                let label = format!("{source}");
                steps.push(PipeStep {
                    span,
                    label,
                    node: Some(StepNode::Source(source)),
                    error: None,
                });
                if let Some(as_) = as_ {
                    let label = format!("| as {}", as_.name);
                    steps.push(PipeStep {
                        span,
                        label,
                        node: Some(StepNode::Aggregate(Aggregate::As(as_))),
                        error: None,
                    });
                }
            }
            Err(e) => {
                let label = extract_span_text(source_text, span);
                steps.push(PipeStep {
                    span,
                    label,
                    node: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    // Remaining: sample_rule, filter_rule, pipe_rule
    for pair in pairs {
        let span = pair_to_source_span(&pair);
        match pair.as_rule() {
            Rule::EOI => break,
            Rule::sample_rule => match crate::parser::parse_sample(pair) {
                Ok(v) => {
                    steps.push(PipeStep {
                        span,
                        label: format!("| sample {v}"),
                        node: Some(StepNode::Sample(v)),
                        error: None,
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        label: extract_span_text(source_text, span),
                        node: None,
                        error: Some(e.to_string()),
                    });
                }
            },
            Rule::filter_rule => match crate::parser::parse_filter(pair, params) {
                Ok(filter) => {
                    let label = format!("| where {filter}");
                    steps.push(PipeStep {
                        span,
                        label,
                        node: Some(StepNode::Filter(filter)),
                        error: None,
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        label: extract_span_text(source_text, span),
                        node: None,
                        error: Some(e.to_string()),
                    });
                }
            },
            Rule::pipe_rule => match parser.parse_pipe(pair, params) {
                Ok(agg) => {
                    let label = format!("{agg}");
                    steps.push(PipeStep {
                        span,
                        label,
                        node: Some(StepNode::Aggregate(agg)),
                        error: None,
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        label: extract_span_text(source_text, span),
                        node: None,
                        error: Some(e.to_string()),
                    });
                }
            },
            _ => {}
        }
    }
}

fn parse_compute_steps(
    parser: &crate::parser::Parser,
    query_pair: pest::iterators::Pair<Rule>,
    source_text: &str,
    directives: &Directives,
    params: &Params,
    steps: &mut Vec<PipeStep>,
) {
    let source_span = pair_to_source_span(&query_pair);
    let mut pairs = query_pair.into_inner();

    // Left sub-query
    let mut left_steps = Vec::new();
    if let Some(left_pair) = pairs.next() {
        match left_pair.as_rule() {
            Rule::simple_query => {
                parse_simple_steps(parser, left_pair, source_text, params, &mut left_steps);
            }
            Rule::compute_query => {
                parse_compute_steps(
                    parser,
                    left_pair,
                    source_text,
                    directives,
                    params,
                    &mut left_steps,
                );
            }
            _ => {}
        }
    }

    // Right sub-query
    let mut right_steps = Vec::new();
    if let Some(right_pair) = pairs.next() {
        match right_pair.as_rule() {
            Rule::simple_query => {
                parse_simple_steps(parser, right_pair, source_text, params, &mut right_steps);
            }
            Rule::compute_query => {
                parse_compute_steps(
                    parser,
                    right_pair,
                    source_text,
                    directives,
                    params,
                    &mut right_steps,
                );
            }
            _ => {}
        }
    }

    // compute_rule: | compute name using fn
    let mut name = String::new();
    let mut op = None;
    if let Some(compute_rule_pair) = pairs.next() {
        if compute_rule_pair.as_rule() == Rule::compute_rule {
            let mut inner = compute_rule_pair.into_inner();
            // pipe_keyword
            inner.next();
            // metric_name
            if let Some(n) = inner.next() {
                name = n.as_str().to_string();
            }
            // compute_fn
            if let Some(fn_pair) = inner.next() {
                op = parser.parse_compute_fn(fn_pair).ok();
            }
        }
    }

    // Build the compute source span — from start of ( to end of compute_rule
    // We use source_span which covers the whole compute_query pair
    let compute_node = StepNode::Compute {
        left: left_steps,
        right: right_steps,
        name: name.clone(),
        op: op.unwrap_or(ComputeFunction::Builtin(crate::types::ComputeType::Div)),
    };
    let label = format!("{compute_node}");
    steps.push(PipeStep {
        span: source_span,
        label,
        node: Some(compute_node),
        error: None,
    });

    // Post-compute pipe_rule*
    for pair in pairs {
        let span = pair_to_source_span(&pair);
        match pair.as_rule() {
            Rule::EOI => break,
            Rule::pipe_rule => match parser.parse_pipe(pair, params) {
                Ok(agg) => {
                    let label = format!("{agg}");
                    steps.push(PipeStep {
                        span,
                        label,
                        node: Some(StepNode::Aggregate(agg)),
                        error: None,
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        label: extract_span_text(source_text, span),
                        node: None,
                        error: Some(e.to_string()),
                    });
                }
            },
            _ => {}
        }
    }
}

/// Extract raw text from source using a SourceSpan.
fn extract_span_text(source: &str, span: SourceSpan) -> String {
    let start = span.offset();
    let end = start + span.len();
    source.get(start..end).unwrap_or("").trim().to_string()
}
