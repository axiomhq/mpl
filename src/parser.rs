use std::{num::ParseFloatError, str::FromStr};

use chrono::DateTime;
use pest::iterators::{Pair, Pairs};
use pest_derive::Parser;
use regex::Regex;
use strumbra::SharedString;

use crate::{
    ParseError,
    errors::pair_to_source_span,
    linker::{
        AlignFunction, ComputeFunction, Function, FunctionId, GroupFunction, Module, ModuleId,
    },
    query::{
        Aggregate, Align, As, BucketBy, Cmp, DirectiveValue, Directives, Filter, GroupBy, Join,
        Mapping, MetricId, Param, ParamType, ParamValue, Params, Query, RelativeTime, Source,
        TagType, Time, TimeRange, TimeUnit,
    },
    stdlib::STDLIB,
    tags::TagValue,
    types::{BucketSpec, BucketType, ConversionMethod, Dataset, Metric, Parameterized},
};

#[cfg(test)]
mod tests;

#[derive(Parser)]
#[grammar = "mpl.pest"] // relative to src
pub(crate) struct MPLParser;

type Result<T> = std::result::Result<T, ParseError>;

trait PairsHelper<'i, R>
where
    R: pest::RuleType,
{
    fn n(&mut self) -> Result<Pair<'i, R>>;
    fn assert_empty(&mut self) -> Result<()>;
}

impl<'i> PairsHelper<'i, Rule> for Pairs<'i, Rule> {
    fn n(&mut self) -> Result<Pair<'i, Rule>> {
        self.peek()
            .map(|p| pair_to_source_span(&p))
            .ok_or_else(|| ParseError::EOF {
                span: miette::SourceSpan::new(0.into(), 0),
            })
            .and_then(|span| self.next().ok_or(ParseError::EOF { span }))
    }
    fn assert_empty(&mut self) -> Result<()> {
        if !Pairs::is_empty(self) {
            let first = self.peek();
            let span = first.map_or_else(
                || miette::SourceSpan::new(0.into(), 0),
                |p| pair_to_source_span(&p),
            );
            return Err(ParseError::UnexpectedTokens {
                span,
                rules: self.map(|p| p.as_rule()).collect(),
            });
        }
        Ok(())
    }
}

trait PairHelper<R>
where
    R: pest::RuleType,
{
    fn assert_type(&self, expected: R) -> Result<()>;
}

impl PairHelper<Rule> for Pair<'_, Rule> {
    fn assert_type(&self, expected: Rule) -> Result<()> {
        if self.as_rule() != expected {
            return Err(ParseError::Unexpected {
                span: pair_to_source_span(self),
                rule: self.as_rule(),
                expected: vec![expected],
            });
        }
        Ok(())
    }
}

fn unescape(data: &str, delim: char) -> String {
    let mut escaped = false;
    let mut res = String::with_capacity(data.len() - 2);
    for c in data
        .trim_start_matches(delim)
        .trim_end_matches(delim)
        .chars()
    {
        if escaped {
            escaped = false;
            match c {
                'r' => res.push('\r'),
                'n' => res.push('\n'),
                't' => res.push('\t'),
                'b' => res.push('\x08'),
                'f' => res.push('\x0C'),
                '\\' => res.push('\\'),
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

fn parse_ident(source: &Pair<'_, Rule>) -> Result<String> {
    match source.as_rule() {
        Rule::plain_ident => Ok(source.as_str().to_string()),
        Rule::escaped_ident => Ok(unescape(source.as_str(), '`').clone()),
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(source),
            rule,
            expected: vec![Rule::plain_ident, Rule::escaped_ident],
        }),
    }
}

fn resolve_param<'p>(source: &Pair<'_, Rule>, params: &'p Params) -> Result<&'p Param> {
    let name = match source.as_rule() {
        Rule::plain_ident => source.as_str(),
        rule => {
            return Err(ParseError::Unexpected {
                span: pair_to_source_span(source),
                rule,
                expected: vec![Rule::plain_ident],
            });
        }
    };

    let param = params
        .iter()
        .find(|p| p.name == name)
        .ok_or(ParseError::UndefinedParam {
            span: pair_to_source_span(source),
            param: name.to_string(),
        })?;

    Ok(param)
}

fn parse_source_ident(source: &Pair<'_, Rule>) -> Result<String> {
    match source.as_rule() {
        Rule::plain_ident => Ok(source.as_str().to_string()),
        Rule::escaped_ident => Ok(unescape(source.as_str(), '`').clone()),
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(source),
            rule,
            expected: vec![Rule::ident],
        }),
    }
}

fn parse_source_ident_param(
    source: Pair<'_, Rule>,
    params: &Params,
) -> Result<Parameterized<String>> {
    match source.as_rule() {
        Rule::plain_ident => Ok(Parameterized::Concrete(source.as_str().to_string())),
        Rule::escaped_ident => Ok(Parameterized::Concrete(
            unescape(source.as_str(), '`').clone(),
        )),
        Rule::param_ident => {
            let span = pair_to_source_span(&source);
            let mut inner = source.into_inner();
            let next = inner.n()?;
            let param = resolve_param(&next, params)?;
            Ok(Parameterized::Param {
                span,
                param: param.clone(),
            })
        }
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&source),
            rule,
            expected: vec![Rule::ident, Rule::param_ident],
        }),
    }
}

fn parse_param_ident(source: Pair<'_, Rule>) -> Result<String> {
    match source.as_rule() {
        Rule::param_ident => {
            let mut inner = source.into_inner();
            let next = inner.n()?;
            match next.as_rule() {
                Rule::plain_ident => Ok(next.as_str().to_string()),
                rule => Err(ParseError::Unexpected {
                    span: pair_to_source_span(&next),
                    rule,
                    expected: vec![Rule::plain_ident],
                }),
            }
        }
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&source),
            rule,
            expected: vec![Rule::param_ident],
        }),
    }
}

fn parse_dataset(source: Pair<Rule>, params: &Params) -> Result<Parameterized<Dataset>> {
    source.assert_type(Rule::dataset)?;
    let mut inner = source.into_inner();

    let source = inner.n()?;
    let dataset = parse_source_ident_param(source, params)?.map_concrete(Dataset::new);
    Ok(dataset)
}

fn parse_metric_name(source: Pair<Rule>) -> Result<Metric> {
    source.assert_type(Rule::metric_name)?;
    let mut inner = source.into_inner();

    let source = inner.n()?;
    let metric = Metric::try_from(parse_source_ident(&source)?)?;
    Ok(metric)
}

fn parse_metric_id(source: Pair<Rule>, params: &Params) -> Result<MetricId> {
    source.assert_type(Rule::metric_id)?;
    let mut inner = source.into_inner();
    let dataset = parse_dataset(inner.n()?, params)?;
    let metric = parse_metric_name(inner.n()?)?;
    inner.assert_empty()?;
    Ok(MetricId { dataset, metric })
}

fn parse_parameterized_relative_time(
    source: Pair<Rule>,
    params: &Params,
) -> Result<Parameterized<RelativeTime>> {
    source.assert_type(Rule::time_relative_parameterized)?;
    let mut inner = source.into_inner();

    let next = inner.n()?;

    // do we have a param that we need to resolve first?
    if matches!(next.as_rule(), Rule::param_ident) {
        let span = pair_to_source_span(&next);
        let mut inner = next.into_inner();
        let next = inner.n()?;
        let param = resolve_param(&next, params)?;
        return Ok(Parameterized::Param {
            span,
            param: param.clone(),
        });
    }

    parse_relative_time_inner(inner, &next).map(Parameterized::Concrete)
}

fn parse_relative_time(source: Pair<Rule>) -> Result<RelativeTime> {
    source.assert_type(Rule::time_relative)?;
    let mut inner = source.into_inner();
    let next = inner.n()?;
    parse_relative_time_inner(inner, &next)
}

fn parse_relative_time_inner(
    mut inner: Pairs<'_, Rule>,
    next: &Pair<Rule>,
) -> Result<RelativeTime> {
    let value = next.as_str().parse::<u64>()?;
    let unit = inner.n()?;
    let unit = match unit.as_rule() {
        Rule::time_unit_ms => TimeUnit::Millisecond,
        Rule::time_unit_second => TimeUnit::Second,
        Rule::time_unit_minute => TimeUnit::Minute,
        Rule::time_unit_hour => TimeUnit::Hour,
        Rule::time_unit_day => TimeUnit::Day,
        Rule::time_unit_week => TimeUnit::Week,
        Rule::time_unit_month => TimeUnit::Month,
        Rule::time_unit_year => TimeUnit::Year,
        rule => {
            return Err(ParseError::Unexpected {
                span: pair_to_source_span(&unit),
                rule,
                expected: vec![
                    Rule::time_unit_ms,
                    Rule::time_unit_second,
                    Rule::time_unit_minute,
                    Rule::time_unit_hour,
                    Rule::time_unit_day,
                    Rule::time_unit_week,
                    Rule::time_unit_month,
                    Rule::time_unit_year,
                ],
            });
        }
    };

    Ok(RelativeTime { value, unit })
}

fn parse_time(source: Pair<Rule>) -> Result<Time> {
    match source.as_rule() {
        Rule::time_relative => Ok(Time::Relative(parse_relative_time(source)?)),
        Rule::time_timestamp => Ok(Time::Timestamp(source.as_str().parse()?)),
        Rule::time_rfc_3339 => Ok(Time::RFC3339(DateTime::parse_from_rfc3339(
            source.as_str(),
        )?)),
        Rule::time_modifier => Ok(Time::Modifier(source.as_str().to_string())),
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&source),
            rule,
            expected: vec![
                Rule::time_relative,
                Rule::time_timestamp,
                Rule::time_rfc_3339,
                Rule::time_modifier,
            ],
        }),
    }
}

fn parse_time_range(source: Pair<Rule>) -> Result<TimeRange> {
    source.assert_type(Rule::time_range)?;
    let mut inner = source.into_inner();

    let start = parse_time(inner.n()?)?;
    let end = if inner.is_empty() {
        None
    } else {
        Some(parse_time(inner.n()?)?)
    };
    inner.assert_empty()?;
    Ok(TimeRange { start, end })
}

fn parse_source(source: Pair<Rule>, params: &Params) -> Result<(Source, Option<As>)> {
    source.assert_type(Rule::source)?;
    let mut inner = source.into_inner();

    let metric_id = parse_metric_id(inner.n()?, params)?;
    let next = inner.next();

    match next {
        Some(next) if next.as_rule() == Rule::r#as => {
            let as_ = parse_as(next)?;
            inner.assert_empty()?;
            Ok((
                Source {
                    metric_id,
                    time: None,
                },
                Some(as_),
            ))
        }
        Some(next) if next.as_rule() == Rule::time_range => {
            let time = Some(parse_time_range(next)?);
            if let Some(next) = inner.next() {
                let as_ = parse_as(next)?;
                inner.assert_empty()?;
                Ok((Source { metric_id, time }, Some(as_)))
            } else {
                Ok((Source { metric_id, time }, None))
            }
        }
        Some(next) => Err(ParseError::Unexpected {
            span: pair_to_source_span(&next),
            rule: next.as_rule(),
            expected: vec![Rule::r#as, Rule::time_range],
        }),
        None => Ok((
            Source {
                metric_id,
                time: None,
            },
            None,
        )),
    }
}

fn parse_cmp<'input>(source: &Pair<'input, Rule>) -> Result<&'input str> {
    source.assert_type(Rule::cmp)?;
    Ok(source.as_str())
}
fn parse_cmp_re<'input>(source: &Pair<'input, Rule>) -> Result<&'input str> {
    source.assert_type(Rule::cmp_re)?;
    Ok(source.as_str())
}

#[derive(Debug, Clone, PartialEq, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}
impl Number {
    // Can't do anything about it
    #[allow(clippy::cast_precision_loss)]
    fn as_f64(self) -> f64 {
        match self {
            Number::Int(value) => value as f64,
            Number::Float(value) => value,
        }
    }
}

fn parse_int(source: &Pair<Rule>) -> Result<i64> {
    source.assert_type(Rule::int)?;
    let res = source.as_str().trim().parse()?;
    Ok(res)
}

fn parse_number(source: Pair<Rule>) -> Result<Number> {
    source.assert_type(Rule::number)?;
    let mut inner = source.into_inner();
    let next = inner.n()?;
    let res = match next.as_rule() {
        Rule::int => Ok(Number::Int(parse_int(&next)?)),
        Rule::float => Ok(Number::Float(next.as_str().parse()?)),
        Rule::inf => match next.as_str() {
            "inf" | "+inf" => Ok(Number::Float(f64::INFINITY)),
            "-inf" => Ok(Number::Float(f64::NEG_INFINITY)),
            _ => Err(ParseError::Unexpected {
                span: pair_to_source_span(&next),
                rule: next.as_rule(),
                expected: vec![],
            }),
        },
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&next),
            rule,
            expected: vec![Rule::int, Rule::float],
        }),
    };
    inner.assert_empty()?;
    res
}

fn parse_directive_value(source: Pair<Rule>) -> Result<DirectiveValue> {
    source.assert_type(Rule::value)?;
    let mut inner = source.into_inner();
    let next = inner.n()?;
    match next.as_rule() {
        Rule::string => Ok(DirectiveValue::String(unescape(next.as_str(), '"'))),
        Rule::number => match parse_number(next)? {
            Number::Int(value) => Ok(DirectiveValue::Int(value)),
            Number::Float(value) => Ok(DirectiveValue::Float(value)),
        },
        Rule::bool => Ok(DirectiveValue::Bool(next.as_str().to_string().parse()?)),
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&next),
            rule,
            expected: vec![Rule::string, Rule::number, Rule::bool],
        }),
    }
}
fn parse_param_native_type(source: &Pair<Rule>) -> Result<ParamType> {
    source.assert_type(Rule::param_native_type)?;
    match source.as_str() {
        "dataset" => Ok(ParamType::Dataset),
        "duration" => Ok(ParamType::Duration),
        "regex" => Ok(ParamType::Regex),
        _ => Err(ParseError::Unexpected {
            span: pair_to_source_span(source),
            rule: Rule::param_type,
            expected: vec![Rule::param_type],
        }),
    }
}

fn parse_param_type(source: Pair<Rule>) -> Result<ParamType> {
    source.assert_type(Rule::param_type)?;
    let mut inner = source.into_inner();
    let next = inner.n()?;
    let r = match next.as_rule() {
        Rule::tag_type => ParamType::Tag(parse_tag_type(&next)?),
        Rule::param_native_type => parse_param_native_type(&next)?,
        _ => {
            return Err(ParseError::Unexpected {
                span: pair_to_source_span(&next),
                rule: Rule::param_type,
                expected: vec![Rule::tag_type, Rule::param_native_type],
            });
        }
    };
    inner.assert_empty()?;
    Ok(r)
}

fn parse_regex(source: &Pair<'_, Rule>) -> Result<Regex> {
    source.assert_type(Rule::regex)?;
    Ok(regex::Regex::new(&unescape(&source.as_str()[1..], '/'))?)
}

fn parse_value_filter(field: String, source: Pair<Rule>, params: &Params) -> Result<Filter> {
    source.assert_type(Rule::value_filter)?;
    let mut inner = source.into_inner();
    let operator_pair = inner.n()?;
    let operator = parse_cmp(&operator_pair)?;
    let next = inner.n()?;

    next.assert_type(Rule::value)?;
    let mut inner = next.into_inner();
    let next = inner.n()?;

    let value = if next.as_rule() == Rule::param_ident {
        // param
        let span = pair_to_source_span(&next);
        let mut inner = next.into_inner();
        let next = inner.n()?;
        let param = resolve_param(&next, params)?;
        Parameterized::Param {
            span,
            param: param.clone(),
        }
    } else {
        // concrete value
        match next.as_rule() {
            Rule::string => Parameterized::Concrete(TagValue::String(SharedString::try_from(
                unescape(next.as_str(), '"'),
            )?)),
            Rule::number => match parse_number(next)? {
                Number::Int(value) => Parameterized::Concrete(TagValue::Int(value)),
                Number::Float(value) => Parameterized::Concrete(TagValue::Float(value)),
            },
            Rule::bool => {
                Parameterized::Concrete(TagValue::Bool(next.as_str().to_string().parse()?))
            }
            rule => {
                return Err(ParseError::Unexpected {
                    span: pair_to_source_span(&next),
                    rule,
                    expected: vec![Rule::string, Rule::number, Rule::bool],
                });
            }
        }
    };

    let rhs = match operator {
        "==" => Cmp::Eq(value),
        "!=" => Cmp::Ne(value),
        ">" => Cmp::Gt(value),
        ">=" => Cmp::Ge(value),
        "<" => Cmp::Lt(value),
        "<=" => Cmp::Le(value),
        other => {
            return Err(ParseError::UnsupportedTagComparison {
                span: pair_to_source_span(&operator_pair),
                op: other.to_string(),
            });
        }
    };
    Ok(Filter::Cmp { field, rhs })
}

#[derive(Debug, thiserror::Error)]
pub enum ParseParamError {
    #[error("Failed to parse: {0}")]
    Parse(#[from] ParseError),
    #[error("Failed to param as bool: {0}")]
    ParseBool(<bool as FromStr>::Err),
    #[error("Failed to parse as float: {0}")]
    ParseFloat(#[from] ParseFloatError),
    #[error("Failed to parse identifier: {0}")]
    SharedStringError(#[from] strumbra::Error),
    #[error(
        "Param is declared as type {declared_typ}, but the provided type was parsed as `{rule:?}`"
    )]
    TypeMismatch { declared_typ: ParamType, rule: Rule },
    #[error("None Type Params are not supported")]
    NoneParam,
}

pub(crate) fn parse_param_value(
    param: &Param,
    mut source: Pairs<'_, Rule>,
) -> core::result::Result<ParamValue, ParseParamError> {
    let next = source.n()?;
    next.assert_type(Rule::param_value)?;
    let mut inner = next.into_inner();
    let next = inner.n()?;

    match param.typ {
        ParamType::Dataset => match next.as_rule() {
            Rule::plain_ident | Rule::escaped_ident => {
                Ok(ParamValue::Dataset(Dataset::new(parse_ident(&next)?)))
            }
            rule => Err(ParseParamError::TypeMismatch {
                declared_typ: param.typ,
                rule,
            }),
        },
        ParamType::Duration => match next.as_rule() {
            Rule::time_relative => Ok(ParamValue::Duration(parse_relative_time(next)?)),
            rule => Err(ParseParamError::TypeMismatch {
                declared_typ: param.typ,
                rule,
            }),
        },
        ParamType::Tag(TagType::String) => match next.as_rule() {
            Rule::string => Ok(ParamValue::String(unescape(next.as_str(), '"'))),
            rule => Err(ParseParamError::TypeMismatch {
                declared_typ: param.typ,
                rule,
            }),
        },
        ParamType::Tag(TagType::Int) => match next.as_rule() {
            Rule::int => Ok(ParamValue::Int(parse_int(&next)?)),
            rule => Err(ParseParamError::TypeMismatch {
                declared_typ: param.typ,
                rule,
            }),
        },
        ParamType::Tag(TagType::Float) => match next.as_rule() {
            Rule::float => Ok(ParamValue::Float(next.as_str().parse()?)),
            rule => Err(ParseParamError::TypeMismatch {
                declared_typ: param.typ,
                rule,
            }),
        },
        ParamType::Tag(TagType::Bool) => match next.as_rule() {
            Rule::bool => Ok(ParamValue::Bool(
                next.as_str()
                    .to_string()
                    .parse()
                    .map_err(ParseParamError::ParseBool)?,
            )),
            rule => Err(ParseParamError::TypeMismatch {
                declared_typ: param.typ,
                rule,
            }),
        },
        ParamType::Tag(TagType::None) => Err(ParseParamError::NoneParam),
        ParamType::Regex => match next.as_rule() {
            Rule::regex => Ok(ParamValue::Regex(parse_regex(&next)?.into())),
            rule => Err(ParseParamError::TypeMismatch {
                declared_typ: param.typ,
                rule,
            }),
        },
    }
}

fn parse_regex_filter(field: String, source: Pair<Rule>) -> Result<Filter> {
    source.assert_type(Rule::regex_filter)?;
    let mut inner = source.into_inner();
    let operator_pair = inner.n()?;
    let operator = parse_cmp_re(&operator_pair)?;
    let rhs = match operator {
        // we always use a concrete value because we matched on the regex syntax
        "==" => Cmp::RegEx(Parameterized::Concrete(parse_regex(&inner.n()?)?.into())),
        "!=" => Cmp::RegExNot(Parameterized::Concrete(parse_regex(&inner.n()?)?.into())),
        other => {
            return Err(ParseError::UnsupportedRegexpComparison {
                span: pair_to_source_span(&operator_pair),
                op: other.to_string(),
            });
        }
    };
    inner.assert_empty()?;
    Ok(Filter::Cmp { field, rhs })
}

fn parse_tag_type(source: &Pair<Rule>) -> Result<TagType> {
    source.assert_type(Rule::tag_type)?;
    let tpe = source.as_str();
    match tpe {
        "string" => Ok(TagType::String),
        "int" => Ok(TagType::Int),
        "float" => Ok(TagType::Float),
        "bool" => Ok(TagType::Bool),
        other => Err(ParseError::InvalidTagType {
            span: pair_to_source_span(source),
            tpe: other.to_string(),
        }),
    }
}

fn parse_is_filter(field: String, source: Pair<Rule>) -> Result<Filter> {
    source.assert_type(Rule::is_filter)?;
    let mut inner = source.into_inner();
    let operator = inner.n()?;
    operator.assert_type(Rule::kw_is)?;
    let tpe = parse_tag_type(&inner.n()?)?;
    let rhs = Cmp::Is(tpe);
    inner.assert_empty()?;
    Ok(Filter::Cmp { field, rhs })
}

fn parse_filter_atom(source: Pair<Rule>, params: &Params) -> Result<Filter> {
    source.assert_type(Rule::filter_atom)?;
    let mut inner = source.into_inner();

    let next = inner.n()?;
    let field = parse_ident(&next)?;

    let next = inner.n()?;
    let res = match next.as_rule() {
        Rule::regex_filter => parse_regex_filter(field, next),
        Rule::value_filter => parse_value_filter(field, next, params),
        Rule::is_filter => parse_is_filter(field, next),
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&next),
            rule,
            expected: vec![Rule::regex_filter, Rule::value_filter, Rule::is_filter],
        }),
    }?;

    inner.assert_empty()?;
    Ok(res)
}

fn parse_filter_clause(source: Pair<'_, Rule>, params: &Params) -> Result<Filter> {
    source.assert_type(Rule::filter_clause)?;
    let mut inner = source.into_inner();
    let next = inner.n()?;
    let res = match next.as_rule() {
        Rule::filter_atom => parse_filter_atom(next, params),
        Rule::filter_or => parse_or(next, params),
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&next),
            rule,
            expected: vec![Rule::filter_atom, Rule::filter_or],
        }),
    }?;

    inner.assert_empty()?;
    Ok(res)
}

fn parse_filter_not(source: Pair<'_, Rule>, params: &Params) -> Result<Filter> {
    source.assert_type(Rule::filter_not)?;
    let mut inner = source.into_inner();
    let next = inner.n()?;
    let res = if next.as_rule() == Rule::kw_not {
        Filter::Not(Box::new(parse_filter_clause(inner.n()?, params)?))
    } else {
        parse_filter_clause(next, params)?
    };
    inner.assert_empty()?;
    Ok(res)
}
fn parse_and(source: Pair<Rule>, params: &Params) -> Result<Filter> {
    source.assert_type(Rule::filter_and)?;
    let inner = source.into_inner();
    let mut res = inner
        .into_iter()
        .map(|source| parse_filter_not(source, params))
        .collect::<Result<Vec<_>>>()?;
    if res.len() == 1 {
        res.pop().ok_or(ParseError::Unreachable(
            "len 1 filter should never be empty",
        ))
    } else {
        Ok(Filter::And(res))
    }
}
fn parse_or(source: Pair<Rule>, params: &Params) -> Result<Filter> {
    source.assert_type(Rule::filter_or)?;
    let inner = source.into_inner();
    let mut res = inner
        .into_iter()
        .map(|source| parse_and(source, params))
        .collect::<Result<Vec<_>>>()?;
    if res.len() == 1 {
        res.pop().ok_or(ParseError::Unreachable(
            "len 1 filter should never be empty",
        ))
    } else {
        Ok(Filter::Or(res))
    }
}

fn parse_filter(source: Pair<Rule>, params: &Params) -> Result<Filter> {
    source.assert_type(Rule::filter_rule)?;
    let mut inner = source.into_inner();

    let keyword = inner.n()?;
    keyword.assert_type(Rule::pipe_keyword)?;

    let _filter_kw = inner.n()?; // kw_filter / kw_where
    let next = inner.n()?;
    let res = parse_or(next, params)?;
    inner.assert_empty()?;
    Ok(res)
}

fn parse_sample(source: Pair<Rule>) -> Result<f64> {
    source.assert_type(Rule::sample_rule)?;
    let mut inner = source.into_inner();

    let keyword = inner.n()?;
    keyword.assert_type(Rule::pipe_keyword)?;

    let next = inner.n()?;
    next.assert_type(Rule::sample_expr)?;
    let mut inner = next.into_inner();
    let next = inner.n()?;
    next.assert_type(Rule::kw_sample)?;
    let next = inner.n()?;
    let res = parse_number(next)?;
    inner.assert_empty()?;
    Ok(res.as_f64())
}

// FIXME(arne): Should params work in the `as` statement?
fn parse_as(source: Pair<Rule>) -> Result<As> {
    source.assert_type(Rule::r#as)?;
    let mut inner = source.into_inner();
    let name = parse_metric_name(inner.n()?)?;
    inner.assert_empty()?;
    Ok(As { name })
}

fn parse_bucket_by_fn(source: &Pair<'_, Rule>) -> Result<BucketType> {
    source.assert_type(Rule::bucket_by_fn)?;
    match source.as_str() {
        "histogram" => Ok(BucketType::Histogram),
        "interpolate_delta_histogram" => Ok(BucketType::InterpolateDeltaHistogram),
        other => Err(ParseError::UnsupportedBucketFunction {
            span: pair_to_source_span(source),
            name: other.to_string(),
        }),
    }
}

fn parse_join(source: Pair<Rule>, params: &Params) -> Result<Join> {
    source.assert_type(Rule::join)?;
    let span = pair_to_source_span(&source);
    let mut inner = source.into_inner();

    let tag = inner
        .n()?
        .into_inner()
        .map(|field| parse_ident(&field))
        .collect::<Result<_>>()?;
    let from = parse_metric_id(inner.n()?, params)?;
    let by = inner
        .n()?
        .into_inner()
        .map(|field| parse_ident(&field))
        .collect::<Result<_>>()?;

    inner.assert_empty()?;
    Ok(Join {
        span,
        tag,
        from,
        by,
    })
}

fn parse_bucket_conversion(source: &Pair<'_, Rule>) -> Result<ConversionMethod> {
    source.assert_type(Rule::bucket_conversion)?;
    match source.as_str() {
        "rate" => Ok(ConversionMethod::Rate),
        "increase" => Ok(ConversionMethod::Increase),
        other => Err(ParseError::UnsupportedBucketFunction {
            span: pair_to_source_span(source),
            name: other.to_string(),
        }),
    }
}

fn parse_bucket_spec(source: Pair<Rule>) -> Result<BucketSpec> {
    source.assert_type(Rule::bucket_spec)?;
    Ok(match source.as_str() {
        "count" => BucketSpec::Count,
        "avg" => BucketSpec::Avg,
        "sum" => BucketSpec::Sum,
        "min" => BucketSpec::Min,
        "max" => BucketSpec::Max,
        _ => {
            let mut inner = source.into_inner();
            let next = inner.n()?;
            let perc = parse_number(next)?;
            BucketSpec::Percentile(perc.as_f64())
        }
    })
}

fn parse_bucket_specs(source: Pair<Rule>) -> Result<Vec<BucketSpec>> {
    source.assert_type(Rule::bucket_specs)?;
    source
        .into_inner()
        .map(parse_bucket_spec)
        .collect::<Result<_>>()
}

fn parse_bucket_fn_call_simple(source: Pair<Rule>) -> Result<(BucketType, Vec<BucketSpec>)> {
    source.assert_type(Rule::bucket_fn_call_simple)?;
    let mut inner = source.into_inner();
    let function = parse_bucket_by_fn(&inner.n()?)?;
    let specs = parse_bucket_specs(inner.n()?)?;
    inner.assert_empty()?;
    Ok((function, specs))
}

fn parse_bucket_fn_call_with_conversion(
    source: Pair<Rule>,
) -> Result<(BucketType, Vec<BucketSpec>)> {
    source.assert_type(Rule::bucket_fn_call_with_conversion)?;
    let mut inner = source.into_inner();
    let _fn_name = inner.n()?;
    let mode = parse_bucket_conversion(&inner.n()?)?;
    let specs = parse_bucket_specs(inner.n()?)?;
    inner.assert_empty()?;
    Ok((BucketType::InterpolateCumulativeHistogram(mode), specs))
}

fn parse_bucket_fn_call(source: Pair<Rule>) -> Result<(BucketType, Vec<BucketSpec>)> {
    source.assert_type(Rule::bucket_fn_call)?;
    let mut inner = source.into_inner();
    let next = inner.n()?;
    let result = match next.as_rule() {
        Rule::bucket_fn_call_simple => parse_bucket_fn_call_simple(next),
        Rule::bucket_fn_call_with_conversion => parse_bucket_fn_call_with_conversion(next),
        rule => Err(ParseError::Unexpected {
            span: pair_to_source_span(&next),
            rule,
            expected: vec![
                Rule::bucket_fn_call_simple,
                Rule::bucket_fn_call_with_conversion,
            ],
        }),
    };
    inner.assert_empty()?;
    result
}

fn parse_bucket_by(source: Pair<Rule>, params: &Params) -> Result<BucketBy> {
    source.assert_type(Rule::bucket_by)?;
    let span = pair_to_source_span(&source);
    let mut inner = source.into_inner();
    let next = inner.n()?;

    let (tags, next) = if next.as_rule() == Rule::tags {
        let fields = next
            .into_inner()
            .map(|field| parse_ident(&field))
            .collect::<Result<_>>()?;
        (fields, inner.n()?)
    } else {
        (Vec::new(), next)
    };
    let time = parse_parameterized_relative_time(next, params)?;
    let (function, spec) = parse_bucket_fn_call(inner.n()?)?;

    inner.assert_empty()?;
    Ok(BucketBy {
        span,
        function,
        time,
        tags,
        spec,
    })
}

fn parse_function_id(source: Pair<Rule>) -> Result<Function> {
    let mut inner = source.into_inner();
    let mut next = inner.n()?;
    let mut module = Vec::new();
    while next.as_rule() == Rule::module {
        let mut this = std::mem::replace(&mut next, inner.n()?).into_inner();
        let name = parse_ident(&this.n()?)?;
        this.assert_empty()?;
        module.push(ModuleId(name));
    }
    let name = parse_ident(&next)?;
    let name = FunctionId(name);
    inner.assert_empty()?;
    Ok(Function {
        name,
        module_path: module,
    })
}

impl Parser {
    pub(crate) fn parse_query(&self, pairs: &mut Pairs<Rule>) -> Result<Query> {
        let mut next = pairs.next().ok_or(ParseError::EOF {
            span: miette::SourceSpan::new(0.into(), 0),
        })?;
        let mut directives = Directives::default();
        let mut params = Params::default();
        loop {
            match next.as_rule() {
                Rule::directive => {
                    let span = pair_to_source_span(&next);
                    let (directive, value) = Parser::parse_directive(next)?;
                    directives.insert(directive, value);
                    next = pairs.next().ok_or(ParseError::EOF { span })?;
                }
                Rule::param => {
                    let span = pair_to_source_span(&next);
                    params.push(Parser::parse_param(&params, next)?);
                    next = pairs.next().ok_or(ParseError::EOF { span })?;
                }
                _ => break,
            }
        }

        self.parse_query_(directives, params, next)
    }

    fn parse_directive(source: Pair<'_, Rule>) -> Result<(String, DirectiveValue)> {
        let mut inner = source.into_inner();
        let directive = parse_ident(&inner.n()?)?;
        let value = if let Some(v) = inner.next() {
            match v.as_rule() {
                Rule::plain_ident | Rule::escaped_ident => DirectiveValue::Ident(parse_ident(&v)?),
                Rule::value => parse_directive_value(v)?,
                rule => Err(ParseError::Unexpected {
                    span: pair_to_source_span(&v),
                    rule,
                    expected: vec![Rule::escaped_ident, Rule::plain_ident, Rule::value],
                })?,
            }
        } else {
            DirectiveValue::None
        };
        Ok((directive, value))
    }

    fn parse_param(params: &Params, source: Pair<'_, Rule>) -> Result<Param> {
        let mut inner = source.into_inner();
        let next = inner.n()?;
        let span = pair_to_source_span(&next);

        let name = parse_param_ident(next)?;
        if params.iter().any(|p| p.name == name) {
            return Err(ParseError::ParamDefinedMultipleTimes { span, param: name });
        }

        let typ = parse_param_type(inner.n()?)?;

        Ok(Param { span, name, typ })
    }

    fn parse_query_(
        &self,
        directives: Directives,
        params: Params,
        query: Pair<Rule>,
    ) -> Result<Query> {
        match query.as_rule() {
            Rule::simple_query => {
                let inner = query.into_inner();
                self.parse_simple_query(directives, params, inner)
            }
            Rule::compute_query => {
                let inner = query.into_inner();
                self.parse_compute_query(directives, params, inner)
            }
            rule => Err(ParseError::Unexpected {
                span: pair_to_source_span(&query),
                rule,
                expected: vec![Rule::simple_query, Rule::compute_query],
            }),
        }
    }

    fn parse_compute_query(
        &self,
        directives: Directives,
        params: Params,
        mut pairs: Pairs<Rule>,
    ) -> Result<Query> {
        let next = pairs.next().ok_or(ParseError::EOF {
            span: miette::SourceSpan::new(0.into(), 0),
        })?;
        let left = Box::new(self.parse_query_(directives.clone(), params.clone(), next)?);
        let next = pairs.next().ok_or(ParseError::EOF {
            span: miette::SourceSpan::new(0.into(), 0),
        })?;
        let right = Box::new(self.parse_query_(directives.clone(), params.clone(), next)?);

        let next = pairs.next().ok_or(ParseError::EOF {
            span: miette::SourceSpan::new(0.into(), 0),
        })?;
        match next.as_rule() {
            Rule::compute_rule => {}
            Rule::time_range => {
                return Err(ParseError::NotSupported {
                    span: pair_to_source_span(&next),
                    rule: Rule::time_range,
                });
            }
            rule => {
                return Err(ParseError::Unexpected {
                    span: pair_to_source_span(&next),
                    rule,
                    expected: vec![Rule::compute_rule, Rule::time_range],
                });
            }
        }
        let mut compute_rule = next.into_inner();

        let keyword = compute_rule.n()?;
        keyword.assert_type(Rule::pipe_keyword)?;

        let next = compute_rule.next().ok_or(ParseError::EOF {
            span: pair_to_source_span(&keyword),
        })?;
        let name = parse_metric_name(next)?;

        let next = compute_rule.next().ok_or(ParseError::EOF {
            span: pair_to_source_span(&keyword),
        })?;
        let op = self.parse_compute_fn(next)?;

        let mut aggregates = Vec::new();

        // Read post-compute pipe_rule* from compute_query (the outer pairs),
        for next in &mut pairs {
            match next.as_rule() {
                Rule::EOI => break,
                Rule::pipe_rule => aggregates.push(self.parse_pipe(next, &params)?),
                rule => {
                    return Err(ParseError::Unexpected {
                        span: pair_to_source_span(&next),
                        rule,
                        expected: vec![Rule::EOI, Rule::pipe_rule],
                    });
                }
            }
        }

        Ok(Query::Compute {
            left,
            right,
            name,
            op,
            aggregates,
            directives,
            params,
        })
    }
    fn parse_simple_query(
        &self,
        directives: Directives,
        params: Params,
        mut pairs: Pairs<Rule>,
    ) -> Result<Query> {
        let (source, as_) = parse_source(
            pairs.next().ok_or(ParseError::EOF {
                span: miette::SourceSpan::new(0.into(), 0),
            })?,
            &params,
        )?;
        let mut sample = None;
        let mut filters = Vec::new();
        let mut aggregates = Vec::new();
        if let Some(as_) = as_ {
            aggregates.push(Aggregate::As(as_));
        }

        for next in &mut pairs {
            match next.as_rule() {
                Rule::EOI => break,
                // we only allow one sample rule
                Rule::sample_rule if sample.is_some() => {}
                Rule::sample_rule => sample = Some(parse_sample(next)?),
                Rule::filter_rule => filters.push(parse_filter(next, &params)?),
                Rule::pipe_rule => aggregates.push(self.parse_pipe(next, &params)?),
                rule => {
                    return Err(ParseError::Unexpected {
                        span: pair_to_source_span(&next),
                        rule,
                        expected: vec![Rule::EOI, Rule::filter_rule, Rule::pipe_rule],
                    });
                }
            }
        }

        Ok(Query::Simple {
            sample,
            source,
            filters,
            aggregates,
            directives,
            params,
        })
    }

    fn parse_pipe(&self, source: Pair<Rule>, params: &Params) -> Result<Aggregate> {
        source.assert_type(Rule::pipe_rule)?;
        let mut inner = source.into_inner();

        let keyword = inner.n()?;
        keyword.assert_type(Rule::pipe_keyword)?;

        let next = inner.n()?;
        inner.assert_empty()?;
        match next.as_rule() {
            Rule::map => Ok(Aggregate::Map(self.parse_map(next)?)),
            Rule::align => Ok(Aggregate::Align(self.parse_align(next, params)?)),
            Rule::group_by => Ok(Aggregate::GroupBy(self.parse_group_by(next)?)),
            Rule::bucket_by => Ok(Aggregate::Bucket(parse_bucket_by(next, params)?)),
            Rule::join => Ok(Aggregate::Join(parse_join(next, params)?)),
            rule @ Rule::replace => Err(ParseError::NotSupported {
                span: pair_to_source_span(&next),
                rule,
            }),
            Rule::r#as => Ok(Aggregate::As(parse_as(next)?)),
            rule => Err(ParseError::Unexpected {
                span: pair_to_source_span(&next),
                rule,
                expected: vec![
                    Rule::map,
                    Rule::align,
                    Rule::group_by,
                    Rule::bucket_by,
                    Rule::join,
                    Rule::replace,
                    Rule::r#as,
                ],
            }),
        }
    }
    fn parse_map(&self, source: Pair<Rule>) -> Result<Mapping> {
        source.assert_type(Rule::map)?;
        let mut inner = source.into_inner();
        let next = inner.n()?;
        let map = match next.as_rule() {
            Rule::map_fn => self.parse_map_fn(next),
            Rule::map_eval => self.parse_map_eval(next),
            rule => Err(ParseError::Unexpected {
                span: pair_to_source_span(&next),
                rule,
                expected: vec![Rule::map_fn, Rule::map_eval],
            }),
        }?;
        inner.assert_empty()?;
        Ok(map)
    }
    fn parse_map_fn(&self, source: Pair<Rule>) -> Result<Mapping> {
        source.assert_type(Rule::map_fn)?;
        let span = pair_to_source_span(&source);
        let mut inner = source.into_inner();
        let func_pair = inner.n()?;
        let func = parse_function_id(func_pair)?;
        let arg = inner
            .next()
            .map(|n| parse_number(n).map(Number::as_f64))
            .transpose()?;
        let Some(function) = self.stdlib.map_fn(&func) else {
            return Err(ParseError::UnsupportedMapFunction {
                span,
                name: func.to_string(),
            });
        };

        inner.assert_empty()?;

        Ok(Mapping {
            function: function.clone(),
            arg,
        })
    }

    fn parse_map_eval(&self, source: Pair<Rule>) -> Result<Mapping> {
        source.assert_type(Rule::map_eval)?;
        let span = pair_to_source_span(&source);
        let mut inner = source.into_inner();
        let next = inner.n()?;
        let func = match next.as_rule() {
            Rule::func => parse_function_id(next)?,
            Rule::map_calc_op => Function {
                module_path: vec![],
                name: FunctionId::new(next.as_str()),
            },
            rule => {
                return Err(ParseError::Unexpected {
                    span: pair_to_source_span(&next),
                    rule,
                    expected: vec![Rule::func, Rule::map_calc_op],
                });
            }
        };
        let Some(function) = self.stdlib.map_fn(&func) else {
            return Err(ParseError::UnsupportedMapEvaluation {
                span,
                name: func.to_string(),
            });
        };
        let arg = Some(parse_number(inner.n()?)?.as_f64());
        Ok(Mapping {
            function: function.clone(),
            arg,
        })
    }

    fn parse_align(&self, source: Pair<Rule>, params: &Params) -> Result<Align> {
        source.assert_type(Rule::align)?;
        let mut inner = source.into_inner();
        let time = parse_parameterized_relative_time(inner.n()?, params)?;
        let next = inner.n()?;
        if next.as_rule() == Rule::time_relative_parameterized {
            let _sliding_window = parse_parameterized_relative_time(next, params)?;
            let _function = self.parse_align_fn(inner.n()?)?;
            inner.assert_empty()?;
            Err(ParseError::NotImplemented("sliding windows"))
        } else {
            let function = self.parse_align_fn(next)?;
            inner.assert_empty()?;
            Ok(Align { function, time })
        }
    }

    fn parse_align_fn(&self, source: Pair<Rule>) -> Result<AlignFunction> {
        let span = pair_to_source_span(&source);
        let f = parse_function_id(source)?;
        let Some(function) = self.stdlib.align_fn(&f) else {
            return Err(ParseError::UnsupportedAlignFunction {
                span,
                name: f.to_string(),
            });
        };

        Ok(function.clone())
    }

    fn parse_group_by_fn(&self, source: Pair<Rule>) -> Result<GroupFunction> {
        let span = pair_to_source_span(&source);
        let f = parse_function_id(source)?;
        let Some(function) = self.stdlib.group_fn(&f) else {
            return Err(ParseError::UnsupportedGroupFunction {
                span,
                name: f.to_string(),
            });
        };

        Ok(function.clone())
    }

    fn parse_group_by(&self, source: Pair<Rule>) -> Result<GroupBy> {
        source.assert_type(Rule::group_by)?;
        let span = pair_to_source_span(&source);

        let mut inner = source.into_inner();
        let next = inner.n()?;

        let (tags, function) = if next.as_rule() == Rule::tags {
            let fields = next
                .into_inner()
                .map(|field| parse_ident(&field))
                .collect::<Result<_>>()?;
            let function = self.parse_group_by_fn(inner.n()?)?;
            (fields, function)
        } else {
            (Vec::new(), self.parse_group_by_fn(next)?)
        };
        inner.assert_empty()?;
        Ok(GroupBy {
            span,
            function,
            tags,
        })
    }

    fn parse_compute_fn(&self, source: Pair<Rule>) -> Result<ComputeFunction> {
        source.assert_type(Rule::compute_fn)?;
        let span = pair_to_source_span(&source);
        let mut inner = source.into_inner();
        let next = inner.n()?;
        let f = match next.as_rule() {
            Rule::func => parse_function_id(next)?,
            Rule::compute_op => Function {
                module_path: vec![],
                name: FunctionId::new(next.as_str()),
            },
            rule => {
                return Err(ParseError::Unexpected {
                    span: pair_to_source_span(&next),
                    rule,
                    expected: vec![Rule::func, Rule::compute_op],
                });
            }
        };
        inner.assert_empty()?;
        let Some(function) = self.stdlib.compute_fn(&f) else {
            return Err(ParseError::UnsupportedComputeFunction {
                span,
                name: f.to_string(),
            });
        };

        Ok(function.clone())
    }
}

pub(crate) struct Parser {
    stdlib: &'static Module,
}

impl Default for Parser {
    fn default() -> Self {
        Parser { stdlib: &STDLIB }
    }
}
