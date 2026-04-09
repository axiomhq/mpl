//! Step-based parser for the playground.

use std::fmt::{self, Display};

use derive_more::Deref;

use miette::SourceSpan;
use pest::Parser as _;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{
    errors::pair_to_source_span,
    linker::ComputeFunction,
    parser::{self, MPLParser, Parser, Rule},
    query::{Aggregate, Directives, Filter, Params, Source},
    types::ComputeType,
};

/// A single pipeline step.
#[derive(Debug, Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PipeStep {
    /// Byte range in the source text.
    #[tsify(type = "{ offset: number, length: number }")]
    pub span: SourceSpan,
    /// Canonical display text for this step.
    pub canonical: String,
    /// What was parsed at this step.
    pub node: StepNode,
}

/// The AST node for a parsed step.
#[derive(Debug, Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum StepNode {
    /// The data source.
    Source(Source),
    /// A filter clause.
    Filter(Filter),
    /// An aggregate pipe.
    Aggregate(Aggregate),
    /// A sample clause.
    Sample(f64),
    /// A parse error.
    Error(String),
    /// A compute query.
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
            StepNode::Error(msg) => write!(f, "/* error: {msg} */"),
            StepNode::Compute {
                left,
                right,
                name,
                op,
            } => {
                writeln!(f, "(")?;
                for step in left {
                    writeln!(f, "  {}", step.canonical)?;
                }
                writeln!(f, ",")?;
                for step in right {
                    writeln!(f, "  {}", step.canonical)?;
                }
                write!(f, ")\n| compute {name} using {op}")
            }
        }
    }
}

/// Result of step-based parsing.
#[derive(Debug, Clone, Deref, Serialize, Deserialize, Tsify)]
#[serde(transparent)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ParseSteps(pub Vec<PipeStep>);

/// Parses an MPL query into pipeline steps with error recovery.
#[wasm_bindgen]
pub fn parse_steps(query: &str) -> Result<ParseSteps, String> {
    let pairs = MPLParser::parse(Rule::file, query).map_err(|e| e.to_string())?;
    let parser = Parser::default();

    let mut steps = Vec::new();
    let mut directives = Directives::default();
    let mut params = Params::default();

    let mut iter = pairs.into_iter().peekable();

    while let Some(pair) = iter.peek() {
        match pair.as_rule() {
            Rule::directive => {
                let pair = iter.next().expect("peeked");
                if let Ok((k, v)) = Parser::parse_directive(pair) {
                    directives.insert(k, v);
                }
            }
            Rule::param => {
                let pair = iter.next().expect("peeked");
                if let Ok(p) = Parser::parse_param(&params, pair) {
                    params.push(p);
                }
            }
            _ => break,
        }
    }

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

    Ok(ParseSteps(steps))
}

fn parse_simple_steps(
    parser: &Parser,
    query_pair: pest::iterators::Pair<Rule>,
    source_text: &str,
    params: &Params,
    steps: &mut Vec<PipeStep>,
) {
    let mut pairs = query_pair.into_inner();

    if let Some(source_pair) = pairs.next() {
        let span = pair_to_source_span(&source_pair);
        match parser::parse_source(source_pair, params) {
            Ok((source, as_)) => {
                let canonical = format!("{source}");
                steps.push(PipeStep {
                    span,
                    canonical,
                    node: StepNode::Source(source),
                });
                if let Some(as_) = as_ {
                    let canonical = format!("| as {}", as_.name);
                    steps.push(PipeStep {
                        span,
                        canonical,
                        node: StepNode::Aggregate(Aggregate::As(as_)),
                    });
                }
            }
            Err(e) => {
                steps.push(PipeStep {
                    span,
                    canonical: extract_span_text(source_text, span),
                    node: StepNode::Error(e.to_string()),
                });
            }
        }
    }

    for pair in pairs {
        let span = pair_to_source_span(&pair);
        match pair.as_rule() {
            Rule::EOI => break,
            Rule::sample_rule => match parser::parse_sample(pair) {
                Ok(v) => {
                    steps.push(PipeStep {
                        span,
                        canonical: format!("| sample {v}"),
                        node: StepNode::Sample(v),
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        canonical: extract_span_text(source_text, span),
                        node: StepNode::Error(e.to_string()),
                    });
                }
            },
            Rule::filter_rule => match parser::parse_filter(pair, params) {
                Ok(filter) => {
                    steps.push(PipeStep {
                        span,
                        canonical: format!("| where {filter}"),
                        node: StepNode::Filter(filter),
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        canonical: extract_span_text(source_text, span),
                        node: StepNode::Error(e.to_string()),
                    });
                }
            },
            Rule::pipe_rule => match parser.parse_pipe(pair, params) {
                Ok(agg) => {
                    steps.push(PipeStep {
                        span,
                        canonical: format!("{agg}"),
                        node: StepNode::Aggregate(agg),
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        canonical: extract_span_text(source_text, span),
                        node: StepNode::Error(e.to_string()),
                    });
                }
            },
            _ => {}
        }
    }
}

fn parse_compute_steps(
    parser: &Parser,
    query_pair: pest::iterators::Pair<Rule>,
    source_text: &str,
    directives: &Directives,
    params: &Params,
    steps: &mut Vec<PipeStep>,
) {
    let source_span = pair_to_source_span(&query_pair);
    let mut pairs = query_pair.into_inner();

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

    let mut name = String::new();
    let mut op = None;
    if let Some(compute_rule_pair) = pairs.next() {
        if compute_rule_pair.as_rule() == Rule::compute_rule {
            let mut inner = compute_rule_pair.into_inner();
            inner.next(); // pipe_keyword
            if let Some(n) = inner.next() {
                name = n.as_str().to_string();
            }
            if let Some(fn_pair) = inner.next() {
                op = parser.parse_compute_fn(fn_pair).ok();
            }
        }
    }

    let compute_node = StepNode::Compute {
        left: left_steps,
        right: right_steps,
        name: name.clone(),
        op: op.unwrap_or(ComputeFunction::Builtin(ComputeType::Div)),
    };
    let canonical = format!("{compute_node}");
    steps.push(PipeStep {
        span: source_span,
        canonical,
        node: compute_node,
    });

    for pair in pairs {
        let span = pair_to_source_span(&pair);
        match pair.as_rule() {
            Rule::EOI => break,
            Rule::pipe_rule => match parser.parse_pipe(pair, params) {
                Ok(agg) => {
                    steps.push(PipeStep {
                        span,
                        canonical: format!("{agg}"),
                        node: StepNode::Aggregate(agg),
                    });
                }
                Err(e) => {
                    steps.push(PipeStep {
                        span,
                        canonical: extract_span_text(source_text, span),
                        node: StepNode::Error(e.to_string()),
                    });
                }
            },
            _ => {}
        }
    }
}

fn extract_span_text(source: &str, span: SourceSpan) -> String {
    let start = span.offset();
    let end = start + span.len();
    source.get(start..end).unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests;
