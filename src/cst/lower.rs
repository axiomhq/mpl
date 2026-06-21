//! Lowering pass: a [`Parse`] tree → the existing [`crate::query`] AST.
//!
//! The lowering walks the lossless tree and produces exactly the same
//! [`Query`] shape the (now retired) `pest` front end built, so every
//! downstream consumer (visitors, typecheck, the formatter) is unaffected.
//!
//! It also hosts the external `param_value` entry point used to parse
//! caller-supplied runtime parameter values, and the string/number/regex
//! helpers that used to live alongside the `pest` parser.

use std::{collections::HashMap, hash::BuildHasher, num::ParseFloatError, str::FromStr};

use chrono::DateTime;
use miette::SourceSpan;
use regex::Regex;
use rowan::TextRange;

use super::{Parse, SyntaxKind, SyntaxNode, SyntaxToken};
use crate::{
    ParseError,
    linker::{Function, FunctionId, ModuleId},
    query::{
        Aggregate, Align, As, BucketBy, Cmp, DirectiveValue, Directives, Expr, Filter,
        FilterOrIfDef, GroupBy, Mapping, MetricId, ParamDeclaration, ParamType, ParamValue, Params,
        Query, RelativeTime, Source, StringFragment, TagExtend, TagType, TerminalParamType, Time,
        TimeRange, TimeUnit, WarningReason, Warnings,
    },
    tags::TagValue,
    types::{BucketSpec, BucketType, ConversionMethod, Dataset, Metric, Parameterized},
};

const SYSTEM_PARAM_PREFIX: &str = "__";

type Result<T> = std::result::Result<T, ParseError>;

/// Lower a parsed tree into the [`Query`] AST with no host-supplied system
/// params (convenience wrapper used by the CST tests).
pub fn lower(parse: &Parse) -> Result<Query> {
    let params: HashMap<String, ParamType> = HashMap::new();
    lower_with_params(parse, params).map(|(query, _warnings)| query)
}

/// Lower a parsed tree into the [`Query`] AST, injecting `system_params` and
/// collecting [`Warnings`]. This is the entry point [`crate::compile`] uses.
pub fn lower_with_params<H: BuildHasher>(
    parse: &Parse,
    system_params: HashMap<String, ParamType, H>,
) -> Result<(Query, Warnings)> {
    if let Some(err) = parse.errors().first() {
        return Err(ParseError::SyntaxError {
            span: span_of(err.range),
            label: err.message.clone(),
            message: err.message.clone(),
            suggestion: None,
        });
    }
    let root = parse.syntax();
    let mut ctx = Ctx {
        params: Params::default(),
        directives: Directives::default(),
        warnings: Warnings::new(),
    };

    // System params first, mirroring the old parser ordering.
    for (name, typ) in system_params {
        if !name.starts_with(SYSTEM_PARAM_PREFIX) {
            return Err(ParseError::SystemParamMissingPrefix { param: name });
        }
        ctx.params.push(ParamDeclaration {
            span: SourceSpan::new(0.into(), 0),
            name,
            typ,
        });
    }

    // Directives and param declarations, in document order.
    for node in root.children() {
        match node.kind() {
            SyntaxKind::DIRECTIVE => {
                let (name, value) = lower_directive(&node)?;
                ctx.directives.insert(name, value);
            }
            SyntaxKind::PARAM_DECL => ctx.add_param(&node)?,
            _ => {}
        }
    }

    let query_node = root
        .children()
        .find(|n| matches!(n.kind(), SyntaxKind::QUERY | SyntaxKind::COMPUTE_QUERY))
        .ok_or_else(|| eof(&root))?;
    let query = ctx.lower_query_node(&query_node)?;
    Ok((query, ctx.warnings))
}

/// Lowering context: the shared params/directives/warnings for a whole file.
struct Ctx {
    params: Params,
    directives: Directives,
    warnings: Warnings,
}

impl Ctx {
    fn lower_query_node(&mut self, node: &SyntaxNode) -> Result<Query> {
        match node.kind() {
            SyntaxKind::QUERY => self.lower_simple_query(node),
            SyntaxKind::COMPUTE_QUERY => self.lower_compute_query(node),
            _ => Err(unexpected(node, "a query")),
        }
    }

    fn lower_simple_query(&mut self, node: &SyntaxNode) -> Result<Query> {
        let source_node = child(node, SyntaxKind::SOURCE).ok_or_else(|| eof(node))?;
        let (source, source_as) = lower_source(&source_node, &self.params)?;

        let mut filters = Vec::new();
        let mut aggregates = Vec::new();
        let mut extends = Vec::new();
        let mut sample = None;
        if let Some(as_) = source_as {
            aggregates.push(Aggregate::As(as_));
        }

        for child_node in node.children() {
            match child_node.kind() {
                SyntaxKind::SOURCE => {}
                SyntaxKind::FILTER_RULE => {
                    filters.push(FilterOrIfDef::Filter(self.lower_filter_rule(&child_node)?));
                }
                SyntaxKind::IFDEF_RULE => filters.push(self.lower_ifdef(&child_node)?),
                SyntaxKind::SAMPLE_RULE => {
                    if sample.is_none() {
                        sample = Some(lower_sample(&child_node)?);
                    }
                }
                SyntaxKind::EXTEND_RULE => extends.extend(self.lower_extend(&child_node)?),
                _ => aggregates.push(self.lower_aggregate(&child_node)?),
            }
        }

        Ok(Query::Simple {
            source,
            filters,
            aggregates,
            directives: self.directives.clone(),
            params: self.params.clone(),
            extends,
            sample,
        })
    }

    fn lower_compute_query(&mut self, node: &SyntaxNode) -> Result<Query> {
        let mut sub = node
            .children()
            .filter(|n| matches!(n.kind(), SyntaxKind::QUERY | SyntaxKind::COMPUTE_QUERY));
        let left_node = sub.next().ok_or_else(|| eof(node))?;
        let right_node = sub.next().ok_or_else(|| eof(node))?;
        let left = Box::new(self.lower_query_node(&left_node)?);
        let right = Box::new(self.lower_query_node(&right_node)?);

        let compute_rule = child(node, SyntaxKind::COMPUTE_RULE)
            .ok_or_else(|| unexpected(node, "a `| compute` rule"))?;
        let name_node =
            child(&compute_rule, SyntaxKind::METRIC_NAME).ok_or_else(|| eof(&compute_rule))?;
        let name = lower_metric_name(&name_node)?;
        let func_node = child(&compute_rule, SyntaxKind::FUNC).ok_or_else(|| eof(&compute_rule))?;
        let op = lower_compute_fn(&func_node)?;

        let mut aggregates = Vec::new();
        let mut extends = Vec::new();
        for child_node in node.children() {
            match child_node.kind() {
                SyntaxKind::QUERY | SyntaxKind::COMPUTE_QUERY | SyntaxKind::COMPUTE_RULE => {}
                SyntaxKind::EXTEND_RULE => extends.extend(self.lower_extend(&child_node)?),
                _ => aggregates.push(self.lower_aggregate(&child_node)?),
            }
        }

        Ok(Query::Compute {
            left,
            right,
            name,
            op,
            aggregates,
            extends,
            directives: self.directives.clone(),
            params: self.params.clone(),
        })
    }

    /// Lower a pipe rule that contributes an [`Aggregate`].
    fn lower_aggregate(&mut self, node: &SyntaxNode) -> Result<Aggregate> {
        match node.kind() {
            SyntaxKind::ALIGN_RULE => Ok(Aggregate::Align(self.lower_align(node)?)),
            SyntaxKind::MAP_RULE => Ok(Aggregate::Map(lower_map(node)?)),
            SyntaxKind::GROUP_RULE => Ok(Aggregate::GroupBy(lower_group(node)?)),
            SyntaxKind::BUCKET_RULE => Ok(Aggregate::Bucket(self.lower_bucket(node)?)),
            SyntaxKind::AS_CLAUSE => Ok(Aggregate::As(lower_as(node)?)),
            SyntaxKind::JOIN_RULE => Err(ParseError::NotSupported {
                span: span_of(node.text_range()),
                feature: "join".to_string(),
            }),
            SyntaxKind::REPLACE_RULE => Err(ParseError::NotSupported {
                span: span_of(node.text_range()),
                feature: "replace".to_string(),
            }),
            _ => Err(unexpected(node, "a pipe rule")),
        }
    }

    // ── filters ──────────────────────────────────────────────────

    fn lower_filter_rule(&self, node: &SyntaxNode) -> Result<Filter> {
        let or_node = child(node, SyntaxKind::FILTER_OR).ok_or_else(|| eof(node))?;
        self.lower_filter_or(&or_node)
    }

    fn lower_filter_or(&self, node: &SyntaxNode) -> Result<Filter> {
        let mut parts = children(node, SyntaxKind::FILTER_AND)
            .map(|n| self.lower_filter_and(&n))
            .collect::<Result<Vec<_>>>()?;
        if parts.len() == 1 {
            Ok(parts.remove(0))
        } else {
            Ok(Filter::Or(parts))
        }
    }

    fn lower_filter_and(&self, node: &SyntaxNode) -> Result<Filter> {
        let mut parts = children(node, SyntaxKind::FILTER_NOT)
            .map(|n| self.lower_filter_not(&n))
            .collect::<Result<Vec<_>>>()?;
        if parts.len() == 1 {
            Ok(parts.remove(0))
        } else {
            Ok(Filter::And(parts))
        }
    }

    fn lower_filter_not(&self, node: &SyntaxNode) -> Result<Filter> {
        let clause = child(node, SyntaxKind::FILTER_CLAUSE).ok_or_else(|| eof(node))?;
        let inner = self.lower_filter_clause(&clause)?;
        if has_keyword(node, "not") {
            Ok(Filter::Not(Box::new(inner)))
        } else {
            Ok(inner)
        }
    }

    fn lower_filter_clause(&self, node: &SyntaxNode) -> Result<Filter> {
        if let Some(or_node) = child(node, SyntaxKind::FILTER_OR) {
            return self.lower_filter_or(&or_node);
        }
        let atom = child(node, SyntaxKind::FILTER_ATOM).ok_or_else(|| eof(node))?;
        self.lower_filter_atom(&atom)
    }

    fn lower_filter_atom(&self, node: &SyntaxNode) -> Result<Filter> {
        let field = ident_text(node).ok_or_else(|| unexpected(node, "a tag name"))?;

        if let Some(value) = child(node, SyntaxKind::VALUE_FILTER) {
            let rhs = self.lower_value_filter(&value)?;
            return Ok(Filter::Cmp { field, rhs });
        }
        if let Some(regex) = child(node, SyntaxKind::REGEX_FILTER) {
            let rhs = lower_regex_filter(&regex)?;
            return Ok(Filter::Cmp { field, rhs });
        }
        if let Some(is_filter) = child(node, SyntaxKind::IS_FILTER) {
            let rhs = lower_is_filter(&is_filter)?;
            return Ok(Filter::Cmp { field, rhs });
        }
        Err(unexpected(node, "a comparison, regex or `is` filter"))
    }

    fn lower_value_filter(&self, node: &SyntaxNode) -> Result<Cmp> {
        let op = token(node, SyntaxKind::CMP_OP).ok_or_else(|| eof(node))?;
        let expr_node = child(node, SyntaxKind::EXPR).ok_or_else(|| eof(node))?;
        let value = self.lower_expr(&expr_node)?;
        Ok(match op.text() {
            "==" => Cmp::Eq(value),
            "!=" => Cmp::Ne(value),
            ">" => Cmp::Gt(value),
            ">=" => Cmp::Ge(value),
            "<" => Cmp::Lt(value),
            "<=" => Cmp::Le(value),
            other => {
                return Err(ParseError::UnsupportedTagComparison {
                    span: token_span(&op),
                    op: other.to_string(),
                });
            }
        })
    }

    fn lower_ifdef(&self, node: &SyntaxNode) -> Result<FilterOrIfDef> {
        let param_tok = token(node, SyntaxKind::PARAM_IDENT).ok_or_else(|| eof(node))?;
        let span = token_span(&param_tok);
        let param = resolve_param(&param_tok, &self.params)?;
        if !param.is_optional() {
            return Err(ParseError::IfdefNotOptional { span, param });
        }

        // The first FILTER_OR is the if-branch, the second (if present) the
        // else-branch.
        let mut branches = children(node, SyntaxKind::FILTER_OR);
        let filter_node = branches.next().ok_or_else(|| eof(node))?;
        let filter = self.lower_filter_or(&filter_node)?;
        let else_filter = match branches.next() {
            Some(else_node) => Some(self.lower_filter_or(&else_node)?),
            None => None,
        };

        Ok(FilterOrIfDef::Ifdef {
            param,
            filter,
            else_filter,
        })
    }

    // ── source / align / map / group / bucket ────────────────────

    fn lower_align(&self, node: &SyntaxNode) -> Result<Align> {
        // Sliding-window align (`over <rel>`) is parsed but rejected, exactly
        // as the old pest path did.
        if has_keyword(node, "over") {
            return Err(ParseError::NotImplemented("sliding windows"));
        }

        let time = match child(node, SyntaxKind::REL_TIME) {
            Some(rel) => Some(lower_rel_time_param(&rel, &self.params)?),
            None => None,
        };

        let func_node = child(node, SyntaxKind::FUNC).ok_or_else(|| eof(node))?;
        let function = lower_align_func(&func_node)?;

        Ok(Align { function, time })
    }

    fn lower_bucket(&self, node: &SyntaxNode) -> Result<BucketBy> {
        let span = span_of(node.text_range());
        let tags = child(node, SyntaxKind::TAGS)
            .map(|t| lower_tags(&t))
            .unwrap_or_default();
        let time = match child(node, SyntaxKind::REL_TIME) {
            Some(rel) => Some(lower_rel_time_param(&rel, &self.params)?),
            None => None,
        };
        let fn_node = child(node, SyntaxKind::BUCKET_FN).ok_or_else(|| eof(node))?;
        let (function, spec) = lower_bucket_fn(&fn_node)?;
        Ok(BucketBy {
            span,
            function,
            time,
            tags,
            spec,
        })
    }

    fn lower_extend(&self, node: &SyntaxNode) -> Result<Vec<TagExtend>> {
        children(node, SyntaxKind::EXTEND_EXPR)
            .map(|n| {
                let tag = ident_text(&n).ok_or_else(|| unexpected(&n, "a tag name"))?;
                let expr_node = child(&n, SyntaxKind::EXPR).ok_or_else(|| eof(&n))?;
                let value = self.lower_expr(&expr_node)?;
                Ok(TagExtend { tag, value })
            })
            .collect()
    }

    fn lower_expr(&self, node: &SyntaxNode) -> Result<Expr> {
        if let Some(param_tok) = token(node, SyntaxKind::PARAM_IDENT) {
            let param = resolve_param(&param_tok, &self.params)?;
            return Ok(Expr::Param {
                span: token_span(&param_tok),
                param,
            });
        }
        if let Some(string_node) = child(node, SyntaxKind::STRING) {
            return self.lower_string(&string_node);
        }
        if let Some(bool_tok) = token(node, SyntaxKind::BOOL_LIT) {
            return Ok(Expr::Const(TagValue::Bool(
                bool_tok.text().parse().map_err(ParseError::InvalidBool)?,
            )));
        }
        if token(node, SyntaxKind::INF_LIT).is_some() {
            let value = if node.text().to_string().trim_start().starts_with('-') {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
            return Ok(Expr::Const(TagValue::Float(value)));
        }
        if let Some(float_tok) = token(node, SyntaxKind::FLOAT) {
            let text = node.text().to_string();
            return Ok(Expr::Const(TagValue::Float(
                text.trim()
                    .parse()
                    .or_else(|_| float_tok.text().parse())
                    .map_err(ParseError::InvalidFloat)?,
            )));
        }
        if let Some(int_tok) = token(node, SyntaxKind::INT) {
            let text = node.text().to_string();
            let value = text.trim().parse().map_err(ParseError::InvalidInteger);
            return Ok(Expr::Const(TagValue::Int(
                value.unwrap_or_else(|_| int_tok.text().parse().unwrap_or_default()),
            )));
        }
        let tag = ident_text(node).ok_or_else(|| unexpected(node, "a value"))?;
        Ok(Expr::Tag(tag))
    }

    /// Lower a `STRING` node (with descended `${ … }` interpolations) into an
    /// [`Expr`]. The CST already carries the literal fragments and the embedded
    /// expressions as real subtrees, so this just unescapes the text fragments
    /// and reuses [`Ctx::lower_expr`] for the interpolated expressions.
    fn lower_string(&self, node: &SyntaxNode) -> Result<Expr> {
        let mut parts: Vec<StringFragment> = Vec::new();
        for element in node.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::STRING_FRAGMENT => {
                    // Boundary fragments carry the surrounding `"`; `unescape_and_trim`
                    // strips them (a no-op on the inner fragments) and unescapes.
                    let text = unescape_and_trim(t.text(), '"');
                    if !text.is_empty() {
                        parts.push(StringFragment::Text(text));
                    }
                }
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::EXPR => {
                    parts.push(StringFragment::Expr(self.lower_expr(&n)?));
                }
                _ => {}
            }
        }

        if parts.iter().all(|p| matches!(p, StringFragment::Text(_))) {
            let joined: String = parts
                .into_iter()
                .map(|p| match p {
                    StringFragment::Text(t) => t,
                    StringFragment::Expr(_) => String::new(),
                })
                .collect();
            return Ok(Expr::Const(TagValue::String(joined.try_into()?)));
        }
        Ok(Expr::String(parts))
    }

    // ── params / param types ─────────────────────────────────────

    fn add_param(&mut self, node: &SyntaxNode) -> Result<()> {
        let name_tok = token(node, SyntaxKind::PARAM_IDENT).ok_or_else(|| eof(node))?;
        let span = token_span(&name_tok);
        let name = param_name(&name_tok);

        if name.starts_with(SYSTEM_PARAM_PREFIX) {
            self.warnings.push_span(
                span,
                WarningReason::ParamUsingSystemPrefix {
                    param: name.clone(),
                },
            );
        } else if self.params.iter().any(|p| p.name == name) {
            return Err(ParseError::ParamDefinedMultipleTimes { span, param: name });
        }

        let type_node = child(node, SyntaxKind::PARAM_TYPE).ok_or_else(|| eof(node))?;
        let typ = self.lower_param_type(&type_node)?;
        self.params.push(ParamDeclaration { span, name, typ });
        Ok(())
    }

    fn lower_param_type(&mut self, node: &SyntaxNode) -> Result<ParamType> {
        let mut names = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::TYPE_NAME);
        let first = names.next().ok_or_else(|| eof(node))?;

        if first.text() == "Option" {
            let inner = names.next().ok_or_else(|| eof(node))?;
            let terminal = self.terminal_type(&inner)?;
            match terminal {
                TerminalParamType::Duration | TerminalParamType::Dataset => Err(unexpected(
                    node,
                    "an optional tag type (string, int, float, bool) or Regex",
                )),
                other => Ok(ParamType::Optional(other)),
            }
        } else {
            Ok(ParamType::Terminal(self.terminal_type(&first)?))
        }
    }

    fn terminal_type(&mut self, tok: &SyntaxToken) -> Result<TerminalParamType> {
        Ok(match tok.text() {
            "string" => TerminalParamType::Tag(TagType::String),
            "int" => TerminalParamType::Tag(TagType::Int),
            "float" => TerminalParamType::Tag(TagType::Float),
            "bool" => TerminalParamType::Tag(TagType::Bool),
            "Duration" => TerminalParamType::Duration,
            "duration" => {
                self.warnings
                    .push_span(token_span(tok), WarningReason::OldDuration);
                TerminalParamType::Duration
            }
            "Dataset" => TerminalParamType::Dataset,
            "Regex" => TerminalParamType::Regex,
            other => {
                return Err(ParseError::SyntaxError {
                    span: token_span(tok),
                    label: format!("unknown type `{other}`"),
                    message: "invalid param type".to_string(),
                    suggestion: None,
                });
            }
        })
    }
}

// ── source ───────────────────────────────────────────────────────

fn lower_source(node: &SyntaxNode, params: &Params) -> Result<(Source, Option<As>)> {
    let metric_id_node = child(node, SyntaxKind::METRIC_ID).ok_or_else(|| eof(node))?;
    let metric_id = lower_metric_id(&metric_id_node, params)?;

    let time = match child(node, SyntaxKind::TIME_RANGE) {
        Some(tr) => Some(lower_time_range(&tr)?),
        None => None,
    };

    let as_ = match child(node, SyntaxKind::AS_CLAUSE) {
        Some(as_node) => Some(lower_as(&as_node)?),
        None => None,
    };

    Ok((Source { metric_id, time }, as_))
}

fn lower_metric_id(node: &SyntaxNode, params: &Params) -> Result<MetricId> {
    let dataset_node = child(node, SyntaxKind::DATASET).ok_or_else(|| eof(node))?;
    let dataset = lower_dataset(&dataset_node, params)?;

    let metric_node = child(node, SyntaxKind::METRIC_NAME).ok_or_else(|| eof(node))?;
    let metric = lower_metric_name(&metric_node)?;

    Ok(MetricId { dataset, metric })
}

fn lower_dataset(node: &SyntaxNode, params: &Params) -> Result<Parameterized<Dataset>> {
    if let Some(param_tok) = token(node, SyntaxKind::PARAM_IDENT) {
        let param = resolve_param(&param_tok, params)?;
        return Ok(Parameterized::Param {
            span: token_span(&param_tok),
            param,
        });
    }
    let name = ident_text(node).ok_or_else(|| unexpected(node, "a dataset name"))?;
    Ok(Parameterized::Concrete(Dataset::new(name)))
}

fn lower_metric_name(node: &SyntaxNode) -> Result<Metric> {
    let name = ident_text(node).ok_or_else(|| unexpected(node, "a metric name"))?;
    Ok(Metric::try_from(name)?)
}

fn lower_as(node: &SyntaxNode) -> Result<As> {
    let metric_node = child(node, SyntaxKind::METRIC_NAME).ok_or_else(|| eof(node))?;
    Ok(As {
        name: lower_metric_name(&metric_node)?,
    })
}

// ── time ─────────────────────────────────────────────────────────

fn lower_time_range(node: &SyntaxNode) -> Result<TimeRange> {
    let mut times = node.children().filter(|n| is_time_node(n.kind()));
    let start_node = times.next().ok_or_else(|| eof(node))?;
    let start = lower_time(&start_node)?;
    let end = match times.next() {
        Some(end_node) => Some(lower_time(&end_node)?),
        None => None,
    };
    Ok(TimeRange { start, end })
}

fn is_time_node(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::REL_TIME
            | SyntaxKind::TIME_TIMESTAMP
            | SyntaxKind::TIME_RFC3339
            | SyntaxKind::TIME_MODIFIER
    )
}

fn lower_time(node: &SyntaxNode) -> Result<Time> {
    match node.kind() {
        SyntaxKind::REL_TIME => Ok(Time::Relative(lower_rel_time(node)?)),
        SyntaxKind::TIME_TIMESTAMP => {
            let tok = token(node, SyntaxKind::INT).ok_or_else(|| eof(node))?;
            Ok(Time::Timestamp(
                tok.text().parse().map_err(ParseError::InvalidInteger)?,
            ))
        }
        SyntaxKind::TIME_RFC3339 => {
            let tok = token(node, SyntaxKind::RFC3339).ok_or_else(|| eof(node))?;
            Ok(Time::RFC3339(
                DateTime::parse_from_rfc3339(tok.text()).map_err(ParseError::InvalidDate)?,
            ))
        }
        SyntaxKind::TIME_MODIFIER => {
            // `+1h` / `-30m`: the modifier text is the concatenation of its
            // (trivia-free) tokens.
            let text: String = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia())
                .map(|t| t.text().to_string())
                .collect();
            Ok(Time::Modifier(text))
        }
        _ => Err(unexpected(node, "a time")),
    }
}

fn lower_rel_time(node: &SyntaxNode) -> Result<RelativeTime> {
    let value_tok = token(node, SyntaxKind::INT)
        .ok_or_else(|| unexpected(node, "a relative time like `5m`"))?;
    let value = value_tok
        .text()
        .parse::<u64>()
        .map_err(ParseError::InvalidInteger)?;
    let unit_tok =
        token(node, SyntaxKind::TIME_UNIT).ok_or_else(|| unexpected(node, "a time unit"))?;
    let unit = match unit_tok.text() {
        "ms" => TimeUnit::Millisecond,
        "s" => TimeUnit::Second,
        "m" => TimeUnit::Minute,
        "h" => TimeUnit::Hour,
        "d" => TimeUnit::Day,
        "w" => TimeUnit::Week,
        "M" => TimeUnit::Month,
        "y" => TimeUnit::Year,
        other => {
            return Err(ParseError::SyntaxError {
                span: token_span(&unit_tok),
                label: format!("unknown time unit `{other}`"),
                message: "invalid time unit".to_string(),
                suggestion: None,
            });
        }
    };
    Ok(RelativeTime { value, unit })
}

fn lower_rel_time_param(node: &SyntaxNode, params: &Params) -> Result<Parameterized<RelativeTime>> {
    if let Some(param_tok) = token(node, SyntaxKind::PARAM_IDENT) {
        let param = resolve_param(&param_tok, params)?;
        return Ok(Parameterized::Param {
            span: token_span(&param_tok),
            param,
        });
    }
    Ok(Parameterized::Concrete(lower_rel_time(node)?))
}

// ── functions: align / map / group / bucket / compute ────────────

fn lower_align_func(node: &SyntaxNode) -> Result<crate::linker::AlignFunction> {
    let function = func_to_function(node);
    crate::STDLIB
        .align_fn(&function)
        .cloned()
        .ok_or_else(|| ParseError::UnsupportedAlignFunction {
            span: span_of(node.text_range()),
            name: function.to_string(),
        })
}

fn lower_compute_fn(node: &SyntaxNode) -> Result<crate::linker::ComputeFunction> {
    let function = func_to_function(node);
    crate::STDLIB.compute_fn(&function).cloned().ok_or_else(|| {
        ParseError::UnsupportedComputeFunction {
            span: span_of(node.text_range()),
            name: function.to_string(),
        }
    })
}

fn lower_map(node: &SyntaxNode) -> Result<Mapping> {
    let arg = match child(node, SyntaxKind::NUMBER) {
        Some(n) => Some(lower_number(&n)?.as_f64()),
        None => None,
    };

    if let Some(func_node) = child(node, SyntaxKind::FUNC) {
        let function = func_to_function(&func_node);
        let mapfn =
            crate::STDLIB
                .map_fn(&function)
                .ok_or_else(|| ParseError::UnsupportedMapFunction {
                    span: span_of(func_node.text_range()),
                    name: function.to_string(),
                })?;
        return Ok(Mapping {
            function: mapfn.clone(),
            arg,
        });
    }

    // map_eval: a bare calc op token + a number.
    let op_tok = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| {
            matches!(
                t.kind(),
                SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH
            )
        })
        .ok_or_else(|| unexpected(node, "a map operation"))?;
    let function = Function {
        module_path: vec![],
        name: FunctionId::new(op_tok.text()),
    };
    let mapfn =
        crate::STDLIB
            .map_fn(&function)
            .ok_or_else(|| ParseError::UnsupportedMapEvaluation {
                span: token_span(&op_tok),
                name: function.to_string(),
            })?;
    Ok(Mapping {
        function: mapfn.clone(),
        arg,
    })
}

fn lower_group(node: &SyntaxNode) -> Result<GroupBy> {
    let span = span_of(node.text_range());
    let tags = child(node, SyntaxKind::TAGS)
        .map(|t| lower_tags(&t))
        .unwrap_or_default();
    let func_node = child(node, SyntaxKind::FUNC).ok_or_else(|| eof(node))?;
    let function = func_to_function(&func_node);
    let function = crate::STDLIB.group_fn(&function).cloned().ok_or_else(|| {
        ParseError::UnsupportedGroupFunction {
            span: span_of(func_node.text_range()),
            name: function.to_string(),
        }
    })?;
    Ok(GroupBy {
        span,
        function,
        tags,
    })
}

fn lower_bucket_fn(node: &SyntaxNode) -> Result<(BucketType, Vec<BucketSpec>)> {
    let name_tok = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::KEYWORD)
        .ok_or_else(|| unexpected(node, "a bucket function"))?;
    let mut specs = children(node, SyntaxKind::BUCKET_SPEC);

    match name_tok.text() {
        "histogram" => Ok((
            BucketType::Histogram,
            specs
                .map(|s| lower_bucket_spec(&s))
                .collect::<Result<_>>()?,
        )),
        "interpolate_delta_histogram" => Ok((
            BucketType::InterpolateDeltaHistogram,
            specs
                .map(|s| lower_bucket_spec(&s))
                .collect::<Result<_>>()?,
        )),
        "interpolate_cumulative_histogram" => {
            let conv_node = specs.next().ok_or_else(|| eof(node))?;
            let mode = lower_bucket_conversion(&conv_node)?;
            let spec = specs
                .map(|s| lower_bucket_spec(&s))
                .collect::<Result<_>>()?;
            Ok((BucketType::InterpolateCumulativeHistogram(mode), spec))
        }
        other => Err(ParseError::UnsupportedBucketFunction {
            span: token_span(&name_tok),
            name: other.to_string(),
        }),
    }
}

fn lower_bucket_conversion(node: &SyntaxNode) -> Result<ConversionMethod> {
    let tok = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::KEYWORD)
        .ok_or_else(|| unexpected(node, "a conversion method"))?;
    match tok.text() {
        "rate" => Ok(ConversionMethod::Rate),
        "increase" => Ok(ConversionMethod::Increase),
        other => Err(ParseError::UnsupportedBucketFunction {
            span: token_span(&tok),
            name: other.to_string(),
        }),
    }
}

fn lower_bucket_spec(node: &SyntaxNode) -> Result<BucketSpec> {
    if let Some(kw) = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::KEYWORD)
    {
        return Ok(match kw.text() {
            "count" => BucketSpec::Count,
            "avg" => BucketSpec::Avg,
            "sum" => BucketSpec::Sum,
            "min" => BucketSpec::Min,
            "max" => BucketSpec::Max,
            other => {
                return Err(ParseError::UnsupportedBucketFunction {
                    span: token_span(&kw),
                    name: other.to_string(),
                });
            }
        });
    }
    let number =
        child(node, SyntaxKind::NUMBER).ok_or_else(|| unexpected(node, "a bucket spec"))?;
    Ok(BucketSpec::Percentile(lower_number(&number)?.as_f64()))
}

fn lower_sample(node: &SyntaxNode) -> Result<f64> {
    let number = child(node, SyntaxKind::NUMBER).ok_or_else(|| eof(node))?;
    Ok(lower_number(&number)?.as_f64())
}

fn lower_tags(node: &SyntaxNode) -> Vec<String> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter_map(|t| match t.kind() {
            SyntaxKind::IDENT => Some(t.text().to_string()),
            SyntaxKind::ESCAPED_IDENT => Some(unescape_and_trim(t.text(), '`')),
            _ => None,
        })
        .collect()
}

/// Build a [`Function`] from a `FUNC` node (idents, or a single operator
/// token for the `+`/`-`/`*`/`/` compute/map-eval forms).
fn func_to_function(node: &SyntaxNode) -> Function {
    let mut idents: Vec<String> = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::IDENT)
        .map(|t| t.text().to_string())
        .collect();
    if let Some(name) = idents.pop() {
        let module_path = idents.into_iter().map(|m| ModuleId::new(&m)).collect();
        return Function {
            module_path,
            name: FunctionId::new(&name),
        };
    }
    // operator function
    let op = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| {
            matches!(
                t.kind(),
                SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH
            )
        })
        .map(|t| t.text().to_string())
        .unwrap_or_default();
    Function {
        module_path: vec![],
        name: FunctionId::new(&op),
    }
}

// ── regex / is filters ───────────────────────────────────────────

fn lower_regex_filter(node: &SyntaxNode) -> Result<Cmp> {
    let op = token(node, SyntaxKind::CMP_OP).ok_or_else(|| eof(node))?;
    let regex_tok = token(node, SyntaxKind::REGEX).ok_or_else(|| eof(node))?;
    let regex = parse_regex(regex_tok.text())?;
    Ok(match op.text() {
        "==" => Cmp::RegEx(Parameterized::Concrete(regex.into())),
        "!=" => Cmp::RegExNot(Parameterized::Concrete(regex.into())),
        other => {
            return Err(ParseError::UnsupportedRegexpComparison {
                span: token_span(&op),
                op: other.to_string(),
            });
        }
    })
}

fn lower_is_filter(node: &SyntaxNode) -> Result<Cmp> {
    let type_tok = token(node, SyntaxKind::TYPE_NAME).ok_or_else(|| eof(node))?;
    let tag_type = match type_tok.text() {
        "string" => TagType::String,
        "int" => TagType::Int,
        "float" => TagType::Float,
        "bool" => TagType::Bool,
        other => {
            return Err(ParseError::InvalidTagType {
                span: token_span(&type_tok),
                tpe: other.to_string(),
            });
        }
    };
    Ok(Cmp::Is(tag_type))
}

// ── directives ───────────────────────────────────────────────────

fn lower_directive(node: &SyntaxNode) -> Result<(String, DirectiveValue)> {
    let name = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find_map(|t| match t.kind() {
            SyntaxKind::IDENT => Some(t.text().to_string()),
            SyntaxKind::ESCAPED_IDENT => Some(unescape_and_trim(t.text(), '`')),
            _ => None,
        })
        .ok_or_else(|| unexpected(node, "a directive name"))?;

    // Only a value after `=` should be treated as the directive value; a bare
    // `set foo;` has none.
    let has_eq = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .any(|t| t.kind() == SyntaxKind::EQ);

    let value = if !has_eq {
        DirectiveValue::None
    } else if let Some(string_node) = child(node, SyntaxKind::STRING) {
        // Directives are plain (non-interpolated) strings; use the node's full
        // text, stripping the quotes and unescaping as the old token path did.
        DirectiveValue::String(unescape_and_trim(&string_node.text().to_string(), '"'))
    } else if token(node, SyntaxKind::INF_LIT).is_some() {
        let v = if node.text().to_string().contains("-inf") {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        DirectiveValue::Float(v)
    } else if let Some(tok) = token(node, SyntaxKind::FLOAT) {
        DirectiveValue::Float(tok.text().parse().map_err(ParseError::InvalidFloat)?)
    } else if let Some(tok) = token(node, SyntaxKind::INT) {
        DirectiveValue::Int(tok.text().parse().map_err(ParseError::InvalidInteger)?)
    } else if let Some(tok) = token(node, SyntaxKind::BOOL_LIT) {
        DirectiveValue::Bool(tok.text().parse().map_err(ParseError::InvalidBool)?)
    } else {
        // a bare value identifier (`set x = foo;`) is the IDENT after the `=`
        match node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| matches!(t.kind(), SyntaxKind::IDENT | SyntaxKind::ESCAPED_IDENT))
            .nth(1)
        {
            Some(tok) if tok.kind() == SyntaxKind::ESCAPED_IDENT => {
                DirectiveValue::Ident(unescape_and_trim(tok.text(), '`'))
            }
            Some(tok) => DirectiveValue::Ident(tok.text().to_string()),
            None => DirectiveValue::None,
        }
    };
    Ok((name, value))
}

// ── numbers ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    #[allow(clippy::cast_precision_loss)]
    fn as_f64(self) -> f64 {
        match self {
            Number::Int(value) => value as f64,
            Number::Float(value) => value,
        }
    }
}

fn lower_number(node: &SyntaxNode) -> Result<Number> {
    let text = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .map(|t| t.text().to_string())
        .collect::<String>();
    if token(node, SyntaxKind::INF_LIT).is_some() {
        return Ok(Number::Float(if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        }));
    }
    if token(node, SyntaxKind::FLOAT).is_some() {
        return Ok(Number::Float(
            text.parse().map_err(ParseError::InvalidFloat)?,
        ));
    }
    Ok(Number::Int(
        text.parse().map_err(ParseError::InvalidInteger)?,
    ))
}

// ── external entry point: runtime param values ───────────────────

/// Error returned when parsing a caller-supplied runtime parameter value.
#[derive(Debug, thiserror::Error)]
pub enum ParseParamError {
    /// Failed to parse the value at all.
    #[error("Failed to parse: {0}")]
    Parse(#[from] ParseError),
    /// Failed to parse the value as a bool.
    #[error("Failed to param as bool: {0}")]
    ParseBool(<bool as FromStr>::Err),
    /// Failed to parse the value as a float.
    #[error("Failed to parse as float: {0}")]
    ParseFloat(#[from] ParseFloatError),
    /// Failed to construct a shared string.
    #[error("Failed to parse identifier: {0}")]
    SharedStringError(#[from] strumbra::Error),
    /// The provided value did not match the declared param type.
    #[error("Param is declared as type {declared_typ}, but the provided value did not match")]
    TypeMismatch {
        /// The declared param type.
        declared_typ: ParamType,
    },
    /// `None`-typed params are unsupported.
    #[error("None Type Params are not supported")]
    NoneParam,
}

/// Parse a caller-supplied runtime `value` for the given declared `param`.
///
/// Replaces the old pest `param_value` external entry point: the value is lexed
/// with the same modal CST lexer ([`super::parser::lex`]) and matched against
/// the param's declared type. A `String`-typed value lexes to a leading
/// [`SyntaxKind::STRING_FRAGMENT`] (the lexer descends into string literals),
/// so its quotes/escapes are handled by the canonical `unescape_and_trim`.
pub fn parse_param_value(
    param: &ParamDeclaration,
    value: &str,
) -> std::result::Result<ParamValue, ParseParamError> {
    // The leading (non-trivia) tokens of the value, from the one canonical lexer.
    let (lexed, _unterminated) = super::parser::lex(value);
    let toks: Vec<(SyntaxKind, &str)> = lexed
        .into_iter()
        .filter(|(kind, _)| !kind.is_trivia())
        .map(|(kind, range)| (kind, &value[range]))
        .collect();
    let mismatch = || ParseParamError::TypeMismatch {
        declared_typ: param.typ,
    };
    let first = toks.first().copied();

    match param.typ() {
        TerminalParamType::Dataset => match first {
            Some((SyntaxKind::IDENT, t)) => Ok(ParamValue::Dataset(Dataset::new(t.to_string()))),
            Some((SyntaxKind::ESCAPED_IDENT, t)) => {
                Ok(ParamValue::Dataset(Dataset::new(unescape_and_trim(t, '`'))))
            }
            _ => Err(mismatch()),
        },
        TerminalParamType::Duration => match (first, toks.get(1).copied()) {
            (Some((SyntaxKind::INT, v)), Some((SyntaxKind::IDENT, unit))) => {
                let value = v.parse::<u64>().map_err(|_| mismatch())?;
                let unit = time_unit(unit).ok_or_else(mismatch)?;
                Ok(ParamValue::Duration(RelativeTime { value, unit }))
            }
            _ => Err(mismatch()),
        },
        TerminalParamType::Regex => match first {
            Some((SyntaxKind::REGEX, t)) => Ok(ParamValue::Regex(parse_regex(t)?.into())),
            _ => Err(mismatch()),
        },
        TerminalParamType::Tag(TagType::String) => match first {
            Some((SyntaxKind::STRING_FRAGMENT, t)) => {
                Ok(ParamValue::String(unescape_and_trim(t, '"')))
            }
            _ => Err(mismatch()),
        },
        TerminalParamType::Tag(TagType::Int) => match first {
            Some((SyntaxKind::INT, t)) => Ok(ParamValue::Int(t.parse().map_err(|_| mismatch())?)),
            _ => Err(mismatch()),
        },
        TerminalParamType::Tag(TagType::Float) => match (first, toks.get(1).copied()) {
            (Some((SyntaxKind::FLOAT, t)), _) => Ok(ParamValue::Float(t.parse()?)),
            (Some((SyntaxKind::IDENT, "inf")), _)
            | (Some((SyntaxKind::PLUS, _)), Some((SyntaxKind::IDENT, "inf"))) => {
                Ok(ParamValue::Float(f64::INFINITY))
            }
            (Some((SyntaxKind::MINUS, _)), Some((SyntaxKind::IDENT, "inf"))) => {
                Ok(ParamValue::Float(f64::NEG_INFINITY))
            }
            _ => Err(mismatch()),
        },
        TerminalParamType::Tag(TagType::Bool) => match first {
            Some((SyntaxKind::IDENT, t @ ("true" | "false"))) => Ok(ParamValue::Bool(
                t.parse().map_err(ParseParamError::ParseBool)?,
            )),
            _ => Err(mismatch()),
        },
        TerminalParamType::Tag(TagType::Null) => Err(ParseParamError::NoneParam),
    }
}

fn time_unit(unit: &str) -> Option<TimeUnit> {
    Some(match unit {
        "ms" => TimeUnit::Millisecond,
        "s" => TimeUnit::Second,
        "m" => TimeUnit::Minute,
        "h" => TimeUnit::Hour,
        "d" => TimeUnit::Day,
        "w" => TimeUnit::Week,
        "M" => TimeUnit::Month,
        "y" => TimeUnit::Year,
        _ => return None,
    })
}

// ── shared helpers ───────────────────────────────────────────────

fn resolve_param(tok: &SyntaxToken, params: &Params) -> Result<ParamDeclaration> {
    let name = param_name(tok);
    params
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .ok_or_else(|| ParseError::UndefinedParam {
            span: token_span(tok),
            param: name,
        })
}

/// The bare name of a `$ident` / `` $`ident` `` parameter token.
fn param_name(tok: &SyntaxToken) -> String {
    let rest = tok.text().strip_prefix('$').unwrap_or(tok.text());
    if rest.starts_with('`') {
        unescape_and_trim(rest, '`')
    } else {
        rest.to_string()
    }
}

fn parse_regex(text: &str) -> Result<Regex> {
    // `#/pattern/` → `pattern`, mirroring the old `parse_regex`.
    let inner = match text.strip_prefix('#') {
        Some(rest) => rest.trim_start_matches('/').trim_end_matches('/'),
        None => text,
    };
    Ok(Regex::new(&unescape(inner, '/'))?)
}

fn child(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|n| n.kind() == kind)
}

fn children(node: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxNode> + '_ {
    node.children().filter(move |n| n.kind() == kind)
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == kind)
}

fn has_keyword(node: &SyntaxNode, text: &str) -> bool {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .any(|t| t.kind() == SyntaxKind::KEYWORD && t.text() == text)
}

/// Text of the first plain or escaped identifier directly under `node`.
fn ident_text(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find_map(|t| match t.kind() {
            SyntaxKind::IDENT => Some(t.text().to_string()),
            SyntaxKind::ESCAPED_IDENT => Some(unescape_and_trim(t.text(), '`')),
            _ => None,
        })
}

fn token_span(tok: &SyntaxToken) -> SourceSpan {
    span_of(tok.text_range())
}

fn span_of(range: TextRange) -> SourceSpan {
    SourceSpan::new(usize::from(range.start()).into(), usize::from(range.len()))
}

fn eof(node: &SyntaxNode) -> ParseError {
    ParseError::EOF {
        span: span_of(node.text_range()),
    }
}

fn unexpected(node: &SyntaxNode, expected: &str) -> ParseError {
    ParseError::SyntaxError {
        span: span_of(node.text_range()),
        label: format!("expected {expected}"),
        message: format!("expected {expected}"),
        suggestion: None,
    }
}

/// Canonical string unescaping helpers (shared by every literal lowering).
pub(crate) fn unescape_and_trim(data: &str, delim: char) -> String {
    unescape(
        data.trim_start_matches(delim).trim_end_matches(delim),
        delim,
    )
}

pub(crate) fn unescape(data: &str, delim: char) -> String {
    let mut escaped = false;
    let mut res = String::with_capacity(data.len());
    for c in data.chars() {
        if escaped {
            escaped = false;
            match c {
                'r' => res.push('\r'),
                'n' => res.push('\n'),
                't' => res.push('\t'),
                'b' => res.push('\x08'),
                'f' => res.push('\x0C'),
                '\\' => res.push('\\'),
                '$' => res.push('$'),
                c if c == delim => res.push(delim),
                _ => {
                    res.push('\\');
                    res.push(c);
                }
            }
        } else if c == '\\' {
            escaped = true;
        } else {
            res.push(c);
        }
    }
    res
}
