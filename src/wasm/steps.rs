//! Step-based parser for the playground.

use std::fmt::{self, Display};

use miette::SourceSpan;
use pest::Parser as _;
use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{
    errors::pair_to_source_span,
    linker::ComputeFunction,
    parser::{self, MPLParser, Parser, Rule},
    query::{Aggregate, Directives, Filter, Params, Source},
    types::ComputeType,
};

/// A single pipeline step.
#[derive(Debug, Clone, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
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
#[derive(Debug, Clone, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
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
#[derive(Debug, Clone, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct ParseStepsResult {
    /// The pipeline steps.
    pub steps: Vec<PipeStep>,
}

/// Parses an MPL query into pipeline steps with error recovery.
#[wasm_bindgen]
pub fn parse_steps(query: &str) -> Result<ParseStepsResult, String> {
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

    Ok(ParseStepsResult { steps })
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
mod tests {
    use super::*;

    #[test]
    fn simple_query() {
        let steps = parse_steps("test:metric").unwrap().steps;
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0].node, StepNode::Source(_)));
        assert_eq!(steps[0].canonical, "`test`:`metric`");
    }

    #[test]
    fn filter_and_aggregate() {
        let steps = parse_steps("test:metric\n| where code >= 500\n| group using sum")
            .unwrap()
            .steps;
        assert_eq!(steps.len(), 3);
        assert!(matches!(steps[0].node, StepNode::Source(_)));
        assert!(matches!(steps[1].node, StepNode::Filter(_)));
        assert!(matches!(steps[2].node, StepNode::Aggregate(_)));
    }

    #[test]
    fn unknown_function_produces_error_step() {
        let steps = parse_steps("test:metric\n| align to 5m using unknown_fn\n| group using sum")
            .unwrap()
            .steps;
        assert_eq!(steps.len(), 3);
        assert!(matches!(steps[0].node, StepNode::Source(_)));
        assert!(matches!(steps[1].node, StepNode::Error(_)));
        assert!(matches!(steps[2].node, StepNode::Aggregate(_)));
    }

    #[test]
    fn unsupported_replace_produces_error_step() {
        let steps = parse_steps("test:metric\n| replace x = y ~ #s/foo/bar/\n| group using sum")
            .unwrap()
            .steps;
        assert!(
            steps
                .iter()
                .any(|s| matches!(&s.node, StepNode::Error(msg) if msg.contains("not supported")))
        );
        assert!(
            steps
                .iter()
                .any(|s| matches!(s.node, StepNode::Aggregate(_)))
        );
    }

    #[test]
    fn unsupported_join_produces_error_step() {
        let steps = parse_steps("test:metric\n| join x from test:other by y\n| group using sum")
            .unwrap()
            .steps;
        assert!(
            steps
                .iter()
                .any(|s| matches!(&s.node, StepNode::Error(msg) if msg.contains("not supported")))
        );
    }

    #[test]
    fn syntax_error_throws() {
        assert!(parse_steps("test:metric\n| blahblah").is_err());
    }

    #[test]
    fn compute_query() {
        let steps = parse_steps("(\n  test:a,\n  test:b\n)\n| compute ratio using /")
            .unwrap()
            .steps;
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0].node, StepNode::Compute { .. }));
    }

    #[test]
    fn compute_with_post_pipes() {
        let steps =
            parse_steps("(\n  test:a,\n  test:b\n)\n| compute ratio using /\n| group using sum")
                .unwrap()
                .steps;
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0].node, StepNode::Compute { .. }));
        assert!(matches!(steps[1].node, StepNode::Aggregate(_)));
    }

    #[test]
    fn sample_step() {
        let steps = parse_steps("test:metric\n| sample 0.5\n| group using sum")
            .unwrap()
            .steps;
        assert_eq!(steps.len(), 3);
        assert!(matches!(steps[1].node, StepNode::Sample(v) if (v - 0.5).abs() < f64::EPSILON));
    }

    #[test]
    fn directives_and_params() {
        let steps = parse_steps(
            "param $ds: Dataset;\nparam $dur: Duration;\n$ds:metric\n| align to $dur using avg",
        )
        .unwrap()
        .steps;
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0].node, StepNode::Source(_)));
    }

    #[test]
    fn canonical_has_no_comments() {
        let steps = parse_steps("// comment\ntest:metric\n// another\n| group using sum")
            .unwrap()
            .steps;
        assert!(!steps[0].canonical.contains("//"));
        assert!(!steps[1].canonical.contains("//"));
    }

    #[test]
    fn source_with_as() {
        let steps = parse_steps("`com.app.test`:ingest_pressure as cake")
            .unwrap()
            .steps;
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0].node, StepNode::Source(_)));
        assert!(matches!(
            steps[1].node,
            StepNode::Aggregate(Aggregate::As(_))
        ));
    }

    #[test]
    fn map_operations() {
        let s = parse_steps("test:metric\n| map rate").unwrap().steps;
        assert!(matches!(s[1].node, StepNode::Aggregate(_)));

        let s = parse_steps("test:metric\n| map * 5").unwrap().steps;
        assert!(matches!(s[1].node, StepNode::Aggregate(_)));

        let s = parse_steps("test:metric\n| map is::lt(100)").unwrap().steps;
        assert!(matches!(s[1].node, StepNode::Aggregate(_)));
    }

    #[test]
    fn align_prom_rate() {
        let steps = parse_steps("test:metric\n| align to 5m using prom::rate")
            .unwrap()
            .steps;
        assert!(matches!(steps[1].node, StepNode::Aggregate(_)));
    }

    #[test]
    fn bucket_histogram() {
        let steps = parse_steps(
            "test:metric\n| bucket by method, path to 5m using interpolate_delta_histogram(0.90, max, 0.99)",
        )
        .unwrap()
        .steps;
        assert!(matches!(steps[1].node, StepNode::Aggregate(_)));
    }

    #[test]
    fn set_directives() {
        let steps = parse_steps("set strict;\nset x = 42;\ntest:metric")
            .unwrap()
            .steps;
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0].node, StepNode::Source(_)));
    }

    #[test]
    fn spans_are_correct() {
        let input = "test:metric\n| where code >= 500\n| group using sum";
        let steps = parse_steps(input).unwrap().steps;
        for step in &steps {
            let end = step.span.offset() + step.span.len();
            assert!(end <= input.len());
        }
    }

    // Display impls (lines 61-65)
    #[test]
    fn display_source() {
        let steps = parse_steps("test:metric").unwrap().steps;
        assert_eq!(format!("{}", steps[0].node), "`test`:`metric`");
    }

    #[test]
    fn display_filter() {
        let steps = parse_steps("test:metric\n| where code >= 500")
            .unwrap()
            .steps;
        let s = format!("{}", steps[1].node);
        assert!(s.starts_with("| where"));
    }

    #[test]
    fn display_aggregate() {
        let steps = parse_steps("test:metric\n| group using sum").unwrap().steps;
        let s = format!("{}", steps[1].node);
        assert!(s.contains("group"));
    }

    #[test]
    fn display_sample() {
        let node = StepNode::Sample(0.5);
        assert_eq!(format!("{node}"), "| sample 0.5");
    }

    #[test]
    fn display_error() {
        let node = StepNode::Error("bad".into());
        assert_eq!(format!("{node}"), "/* error: bad */");
    }

    #[test]
    fn display_compute() {
        let steps = parse_steps("(\n  test:a,\n  test:b\n)\n| compute ratio using /")
            .unwrap()
            .steps;
        let s = format!("{}", steps[0].node);
        assert!(s.contains("compute"));
        assert!(s.contains("ratio"));
    }

    // Source parse error (lines 168-174) — unresolved param in source
    #[test]
    fn source_with_unresolved_param_produces_error() {
        let steps = parse_steps("$missing:metric").unwrap().steps;
        assert!(steps.iter().any(|s| matches!(&s.node, StepNode::Error(_))));
    }

    // Filter parse error (lines 206-212) — unresolved param in filter
    #[test]
    fn filter_with_unresolved_param_produces_error() {
        let steps = parse_steps("test:metric\n| where x == $missing")
            .unwrap()
            .steps;
        assert!(steps.iter().any(|s| matches!(&s.node, StepNode::Error(_))));
    }

    // Nested compute — left sub-query is compute (lines 252-262)
    #[test]
    fn nested_compute_left() {
        let input = "(\n  (\n    test:a,\n    test:b\n  )\n  | compute inner using /,\n  test:c\n)\n| compute outer using *";
        let steps = parse_steps(input).unwrap().steps;
        assert_eq!(steps.len(), 1);
        if let StepNode::Compute { left, .. } = &steps[0].node {
            assert_eq!(left.len(), 1);
            assert!(matches!(left[0].node, StepNode::Compute { .. }));
        } else {
            panic!("expected compute");
        }
    }

    // Nested compute — right sub-query is compute (lines 272-282)
    #[test]
    fn nested_compute_right() {
        let input = "(\n  test:a,\n  (\n    test:b,\n    test:c\n  )\n  | compute inner using /\n)\n| compute outer using *";
        let steps = parse_steps(input).unwrap().steps;
        assert_eq!(steps.len(), 1);
        if let StepNode::Compute { right, .. } = &steps[0].node {
            assert_eq!(right.len(), 1);
            assert!(matches!(right[0].node, StepNode::Compute { .. }));
        } else {
            panic!("expected compute");
        }
    }

    // Post-compute pipe error (lines 326-332)
    #[test]
    fn compute_with_post_pipe_error() {
        let steps = parse_steps(
            "(\n  test:a,\n  test:b\n)\n| compute ratio using /\n| align to 5m using unknown_fn",
        )
        .unwrap()
        .steps;
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0].node, StepNode::Compute { .. }));
        assert!(matches!(steps[1].node, StepNode::Error(_)));
    }
}
