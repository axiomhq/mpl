use std::{collections::HashMap, hash::BuildHasher};

use miette::{Diagnostic, SourceSpan};

use crate::{
    STDLIB,
    ast::{
        self, Ast, AstError, AstWarning, ComputeQuery, FilterAnd, FilterCmp, FilterNot, FilterOr,
        FilterParen, FunctionCall, IdentOrVariable, NonTrivalItem as _, Part, Query as AstQuery,
        Rule, SimpleQuery, SyntaxExpr, SyntaxRule, Variable,
    },
    linker::{Function, FunctionId, FunctionTrait, Module, ModuleId},
    query::{
        self, Aggregate, Align, As, BucketBy, Cmp, DirectiveValue, Directives, Filter,
        FilterOrIfDef, GroupBy, MetricId, ParamDeclaration, ParamType, ParamValue, Params,
        RelativeTime, Source, TagExtend, TagType, TerminalParamType, TimeUnit,
    },
    tags::TagValue,
    types::{BucketSpec, BucketType, ConversionMethod, Dataset, Metric, Parameterized},
};

use super::ast::Ident;

pub(crate) const SYSTEM_PARAM_PREFIX: &str = "__";

/// Error for param parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseParamError {
    /// The value is not a valid literal of the declared type.
    #[error("The value is not a valid literal of the declared type: {0:?}")]
    Invalid(Vec<AstError>),
    /// The value is a literal of the wrong type.
    #[error("Param is declared as type {declared}, but the provided value is of type {found}")]
    TypeMismatch {
        /// The declared type of the param.
        declared: ParamType,
        /// The type the provided value parsed as.
        found: TerminalParamType,
    },
    /// A param of type null carries no value.
    #[error("None Type Params are not supported")]
    NoneParam,
}
impl From<Vec<AstError>> for ParseParamError {
    fn from(errors: Vec<AstError>) -> Self {
        ParseParamError::Invalid(errors)
    }
}

pub(crate) fn parse_param_value(
    param: &ParamDeclaration,
    src: &str,
) -> Result<ParamValue, ParseParamError> {
    match param.typ.typ() {
        TerminalParamType::Dataset => Ok(ParamValue::Dataset(Dataset::new(
            ast::parse_ident_value(src)?.into_string(),
        ))),
        TerminalParamType::Duration => Ok(ParamValue::Duration(RelativeTime {
            value: ast::parse_duration_value(src)?,
            unit: TimeUnit::Second,
        })),
        TerminalParamType::Regex => Ok(ParamValue::Regex(ast::parse_regex_value(src)?.into())),
        TerminalParamType::Timestamp => match ast::parse_const_value(src)? {
            TagValue::Int(v) if let Ok(v) = u64::try_from(v) => Ok(ParamValue::Timestamp(v)),
            value => Err(ParseParamError::TypeMismatch {
                declared: param.typ,
                found: TerminalParamType::Tag(value.tpe()),
            }),
        },
        TerminalParamType::Tag(TagType::Null) => Err(ParseParamError::NoneParam),
        TerminalParamType::Tag(tag) => match (tag, ast::parse_const_value(src)?) {
            (TagType::String, TagValue::String(v)) => Ok(ParamValue::String(v.to_string())),
            (TagType::Int, TagValue::Int(v)) => Ok(ParamValue::Int(v)),
            (TagType::Float, TagValue::Float(v)) => Ok(ParamValue::Float(v)),
            (TagType::Bool, TagValue::Bool(v)) => Ok(ParamValue::Bool(v)),
            (TagType::Array, TagValue::Array(v)) => Ok(ParamValue::Array(v)),
            (_, value) => Err(ParseParamError::TypeMismatch {
                declared: param.typ,
                found: TerminalParamType::Tag(value.tpe()),
            }),
        },
    }
}

type Result<T, E = ParseError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests;

/// `MPL` parsing error
#[derive(thiserror::Error, Debug, Diagnostic)]
pub enum ParseError {
    /// AST errors
    #[error(transparent)]
    #[diagnostic(transparent)]
    AST(
        /// prior AST errors
        AstError,
    ),
    /// Invalid value for directive
    #[error("Invalid value for directive")]
    InvalidDirectiveValue {
        /// The invalid value
        value: TagValue,
        /// The location
        #[label("Invalid directive value: {value}")]
        span: SourceSpan,
    },
    /// No query was provided
    #[error("No query was provided")]
    MissingQuery,
    /// A host-registered system param without the reserved prefix
    #[error("The system param ${param} is missing the system prefix")]
    #[diagnostic(
        code(mpl_lang::system_param_missing_prefix),
        help("The system param is missing the `{SYSTEM_PARAM_PREFIX}` prefix")
    )]
    SystemParamMissingPrefix {
        /// The param
        param: String,
    },
    /// `in` used with a non-array right-hand side
    #[error("`in` requires an array on the right-hand side")]
    #[diagnostic(
        code(mpl_lang::in_requires_array),
        help("Use an array literal (e.g. `in [200, 201]`) or an array-typed param")
    )]
    InRequiresArray {
        /// The location of the offending right-hand side
        #[label("expected an array here")]
        span: SourceSpan,
    },
    /// Directive in the wrong place
    #[error("Directive in the wrong place")]
    DirectiveInWongPlace {
        /// The location
        #[label("You can not place a directive here")]
        span: SourceSpan,
    },
    /// Param in the wrong place
    #[error("Param in the wrong place")]
    ParamInWongPlace {
        /// The location
        #[label("You can not place a param here")]
        span: SourceSpan,
    },
    /// Rule not supported after compute
    #[error("Rule not supported after compute")]
    RuleNotSupportedAfterCompute {
        /// The location
        #[label("Rule not supported after compute")]
        span: SourceSpan,
    },
    /// Unknown function
    #[error("Unknown function: {name}")]
    UnknownFunction {
        /// The function name
        name: String,
        /// The location
        #[label("Unknown function: {name}")]
        span: SourceSpan,
    },
    /// Invalid argument count
    #[error("Invalid argument count for function: {function} (expected {expected}, got {actual})")]
    InvalidArgumentCount {
        /// The function name
        function: String,
        /// The expected number of arguments
        expected: usize,
        /// The actual number of arguments
        actual: usize,
        /// The location
        #[label(
            "Invalid argument count for function: {function} (expected {expected}, got {actual})"
        )]
        span: SourceSpan,
    },
    /// Invalid argument type
    #[error(
        "Invalid argument type for argument {n} of function: {function} (expected {expected}, got {actual})"
    )]
    InvalidArgumentType {
        /// The function name
        function: String,
        /// The expected argument type
        expected: TagType,
        /// The actual argument type
        actual: TagType,
        /// The argument index
        n: usize,
        /// The location
        #[label(
            "Invalid argument type for argument {n} of function: {function} (expected {expected}, got {actual})"
        )]
        span: SourceSpan,
    },
    /// Undefined variable
    #[error("Undefined variable: {name}")]
    UndefinedVariable {
        /// The variable name
        name: String,
        /// The location
        #[label("Undefined variable: {name}")]
        span: SourceSpan,
    },
    /// Invalid variable type
    #[error("Invalid variable type: expected {expected}, got {actual}")]
    InvalidVariableType {
        /// The location
        #[label("Invalid variable type, expected {expected}")]
        variable_span: SourceSpan,
        /// The location
        #[label("The variable was declared here as {expected}")]
        declaration_span: SourceSpan,
        /// The expected type
        expected: query::ParamType,
        /// The actual type
        actual: query::ParamType,
    },
    /// Variables are not supported here
    #[error("Variables are not supported here")]
    VariablesNotSupported {
        /// The location
        #[label("Variables are not supported here")]
        span: SourceSpan,
    },
    /// Invalid bucket spec
    #[error("Invalid bucket spec for function: {function} ({spec})")]
    InvalidBucketSpec {
        /// The function name
        function: String,
        /// The bucket spec
        spec: String,
        /// The argument index
        n: usize,
        /// The location
        #[label("Invalid bucket spec for function: {function} ({spec})")]
        span: SourceSpan,
    },
    /// Invalid metric
    #[error("Invalid metric: {metric}")]
    InvalidMetric {
        /// The metric name
        metric: String,
        /// The location
        #[label("Invalid metric: {metric}")]
        span: SourceSpan,
    },
    /// Expected a filter
    #[error("Expected a filter")]
    ExpectedFilter {
        /// The location
        #[label("Expected a filter")]
        span: SourceSpan,
    },
    /// A warning
    #[error("Parameter in ifdef must be optional")]
    MustBeOptional {
        /// The name of the parameter
        name: String,
        /// The location
        #[label("Parameter `{name}` must be optional")]
        span: SourceSpan,
        /// The location
        #[label("Parameter declared here")]
        decl_span: SourceSpan,
    },
    /// Missing bucket spec for function
    #[error("Missing bucket spec for function: {function}")]
    MissingBucketSpec {
        /// The function name
        function: String,
        /// The location
        #[label("Missing bucket spec for function: {function}")]
        span: SourceSpan,
    },
    /// Parameter declared twice
    #[error("Parameter declared twice: {name}")]
    ParamDeclaredTwice {
        /// The name of the parameter
        name: String,
        /// The location of the first declaration
        #[label("Parameter first declared here")]
        first_span: SourceSpan,
        /// The location of the second declaration
        #[label("Parameter re-declared here")]
        span: SourceSpan,
    },
    /// This rule is not supported here
    #[error("This rule is not supported here")]
    RuleNotSupportedHere {
        /// The location
        #[label("This rule is not supported here")]
        span: SourceSpan,
    },
}
/// A warning
#[derive(thiserror::Error, Debug, Diagnostic)]
pub enum Warning {
    /// A warning from the AST parser
    #[error(transparent)]
    #[diagnostic(transparent)]
    Ast(AstWarning),
    /// A reserved prefix was used for a parameter name
    #[error("Reserved prefix {SYSTEM_PARAM_PREFIX} is not allowed for parameter names: {name}")]
    ReservedPrefix {
        /// The name of the parameter
        name: String,
        /// The location of the parameter
        #[label("Parameter declared here")]
        span: SourceSpan,
    },
    /// Duplicated group declaration
    #[error("Duplicated group declaration: {name}")]
    DuplicatedGroup {
        /// The name of the group
        name: String,
        /// The location of the first declaration
        #[label("The group {name} is declared multiple times")]
        span: SourceSpan,
    },
}

impl Warning {
    pub(crate) fn span(&self) -> SourceSpan {
        match self {
            Warning::Ast(ast_warning) => ast_warning.span(),
            Warning::DuplicatedGroup { span, .. } | Warning::ReservedPrefix { span, .. } => *span,
        }
    }
}

/// Parser from AST -> Query
pub struct Parser {
    stdlib: &'static Module,
    ast: Ast,
}

impl Parser {
    /// Creates a new parser from an AST
    #[must_use]
    pub fn new(ast: Ast) -> Self {
        Parser {
            ast,
            stdlib: &STDLIB,
        }
    }
    /// Parses the AST into a query
    pub fn lower<H: BuildHasher>(
        self,
        system_params: HashMap<String, ParamType, H>,
    ) -> Result<(crate::Query, Vec<Warning>), Vec<ParseError>> {
        let Ast {
            errors,
            warnings,
            mut parts,
        } = self.ast;
        let mut errors: Vec<_> = errors.into_iter().map(ParseError::AST).collect();
        let mut warnings: Vec<_> = warnings.into_iter().map(Warning::Ast).collect();
        let mut directives = HashMap::new();
        // we rever so we can pop the content
        parts.reverse();
        while parts.last().is_some_and(Part::is_directive)
            && let Some(Part::Directive(d)) = parts.pop()
        {
            let v = match d.value {
                None => DirectiveValue::None,
                Some(TagValue::Int(i)) => DirectiveValue::Int(i),
                Some(TagValue::Float(f)) => DirectiveValue::Float(f),
                Some(TagValue::String(s)) => DirectiveValue::String(s.to_string()),
                Some(TagValue::Bool(b)) => DirectiveValue::Bool(b),
                Some(value) => {
                    errors.push(ParseError::InvalidDirectiveValue {
                        value,
                        span: d.node.span(),
                    });
                    continue;
                }
            };
            directives.insert(d.name.to_string(), v);
        }

        let mut params = system_param_declarations(system_params, &mut errors);
        while parts.last().is_some_and(Part::is_param)
            && let Some(Part::Param(p)) = parts.pop()
        {
            let name = p.name.to_string();
            if let Some(old) = params.iter().find(|f| f.name == name) {
                errors.push(ParseError::ParamDeclaredTwice {
                    name,
                    first_span: old.span,
                    span: p.node.span(),
                });
                continue;
            }
            if name.starts_with(SYSTEM_PARAM_PREFIX) {
                // TODO: make this an error
                warnings.push(Warning::ReservedPrefix {
                    name: name.clone(),
                    span: p.node.span(),
                });
            }
            params.push(ParamDeclaration {
                span: p.node.span(),
                name,
                typ: p.ty,
                system: false,
            });
        }

        match parts.pop() {
            None => {
                errors.push(ParseError::MissingQuery);
                Err(errors)
            }
            Some(Part::Query(q)) => {
                let p = QueryParser {
                    stdlib: self.stdlib,
                    directives: directives.clone(),
                    params: params.clone(),
                    warnings: Vec::new(),
                };
                let q = p.parse(q).map(|(q, mut w)| {
                    w.extend(warnings);
                    (q, w)
                });
                match q {
                    Ok(q) if errors.is_empty() => Ok(q),
                    Ok(_) => Err(errors),
                    Err(error) => {
                        errors.push(error);
                        Err(errors)
                    }
                }
            }
            Some(Part::Directive(d)) => {
                errors.push(ParseError::DirectiveInWongPlace {
                    span: d.node.span(),
                });
                Err(errors)
            }
            Some(Part::Param(p)) => {
                errors.push(ParseError::ParamInWongPlace {
                    span: p.node.span(),
                });
                // This is unreachable
                Err(errors)
            }
        }
    }
}

struct QueryParser {
    stdlib: &'static Module,
    directives: Directives,
    params: Params,
    warnings: Vec<Warning>,
}

impl QueryParser {
    fn parse(mut self, q: AstQuery) -> Result<(crate::Query, Vec<Warning>)> {
        let q = self.query(q)?;
        Ok((q, self.warnings))
    }

    fn query(&mut self, q: AstQuery) -> Result<crate::Query> {
        match q {
            AstQuery::Compute(c) => self.compute_query(*c),
            AstQuery::Simple(s) => self.simple_query(s),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn simple_query(
        &mut self,
        SimpleQuery {
            node: _,
            dataset,
            metric,
            alias,
            rules,
        }: SimpleQuery,
    ) -> Result<crate::Query> {
        let dataset = match dataset {
            IdentOrVariable::Ident(d) => Parameterized::Concrete(Dataset::new(d.into_string())),
            IdentOrVariable::Var(v) => {
                self.get_param_typed(&v, ParamType::Terminal(query::TerminalParamType::Dataset))?
            }
        };
        let metric_id = MetricId {
            dataset,
            metric: Metric::new(&metric).map_err(|_| ParseError::InvalidMetric {
                span: metric.node().span(),
                metric: metric.into_string(),
            })?,
        };
        let source = Source {
            metric_id,
            time: None,
        };

        let mut aggregates = Vec::new();
        if let Some(alias) = alias {
            let name = Metric::new(&alias).map_err(|_| ParseError::InvalidMetric {
                metric: alias.to_string(),
                span: alias.span(),
            })?;
            aggregates.push(Aggregate::As(As { name }));
        }
        let mut extends = Vec::new();
        let mut filters = Vec::new();
        let mut rules = rules.into_iter().peekable();
        let sample = if rules.peek().is_some_and(|r| r.is_sample())
            && let Some(SyntaxRule {
                rule: Rule::Sample(s),
                ..
            }) = rules.next()
        {
            Some(s)
        } else {
            None
        };
        while rules.peek().is_some_and(|r| r.is_filter())
            && let Some(SyntaxRule { node, rule }) = rules.next()
        {
            match rule {
                Rule::Filter(f) => filters.push(FilterOrIfDef::Filter(self.filter(f)?)),
                Rule::IfDef {
                    var,
                    if_branch,
                    else_branch,
                } => {
                    let Rule::Filter(filter) = *if_branch else {
                        return Err(ParseError::ExpectedFilter { span: node.span() });
                    };
                    let filter = self.filter(filter)?;
                    let else_filter = if let Some(else_branch) = else_branch {
                        let Rule::Filter(else_filter) = *else_branch else {
                            return Err(ParseError::ExpectedFilter { span: node.span() });
                        };
                        Some(self.filter(else_filter)?)
                    } else {
                        None
                    };
                    let param = self.get_param_decl(&var)?;
                    if !param.is_optional() {
                        return Err(ParseError::MustBeOptional {
                            name: var.name().to_string(),
                            decl_span: param.span,
                            span: var.span(),
                        });
                    }

                    filters.push(FilterOrIfDef::Ifdef {
                        param,
                        filter,
                        else_filter,
                    });
                }
                _ => {
                    return Err(ParseError::RuleNotSupportedHere { span: node.span() });
                }
            }
        }
        while rules.peek().is_some_and(|r| r.is_aggr())
            && let Some(SyntaxRule { node, rule }) = rules.next()
        {
            match rule {
                Rule::Map(func) => aggregates.push(self.map_to_aggr(&func)?),
                Rule::Align { duration, func } => {
                    aggregates.push(self.align_to_aggr(duration, &func)?);
                }
                Rule::Group { groups, func } => aggregates.push(self.group_to_aggr(groups, &func)?),
                Rule::Bucket {
                    groups,
                    duration,
                    func,
                } => aggregates.push(self.bucket_to_aggr(groups, duration, func)?),
                _ => {
                    return Err(ParseError::RuleNotSupportedHere { span: node.span() });
                }
            }
        }
        for SyntaxRule { node, rule } in rules {
            match rule {
                // TODO: Remove pipe as
                Rule::As(alias) => {
                    let name = Metric::new(&alias).map_err(|_| ParseError::InvalidMetric {
                        metric: alias.to_string(),
                        span: alias.span(),
                    })?;
                    aggregates.push(Aggregate::As(As { name }));
                }
                Rule::Extern(extend_parts) => extends.append(&mut self.extend(extend_parts)?),
                _ => {
                    return Err(ParseError::RuleNotSupportedHere { span: node.span() });
                }
            }
        }

        Ok(crate::Query::Simple {
            source,
            filters,
            aggregates,
            directives: self.directives.clone(),
            params: self.params.clone(),
            extends,
            sample,
        })
    }

    fn compute_query(
        &mut self,
        ComputeQuery {
            node: _,
            l,
            r,
            name,
            func,
            rules,
        }: ComputeQuery,
    ) -> Result<crate::Query> {
        let left = Box::new(self.query(l)?);
        let right = Box::new(self.query(r)?);
        let f = call_to_function(&func)?;
        let op = self
            .stdlib
            .compute_fn(&f)
            .ok_or_else(|| ParseError::UnknownFunction {
                span: func.node.span(),
                name: f.name.name().to_string(),
            })?
            .clone();

        if !func.args.is_empty() {
            return Err(ParseError::InvalidArgumentCount {
                function: f.name.name().to_string(),
                expected: 0,
                actual: func.args.len(),
                span: func.node.span(),
            });
        }

        let mut aggregates = Vec::new();
        let mut extends = Vec::new();
        let mut rules = rules.into_iter().peekable();
        while rules.peek().is_some_and(|r| r.is_aggr())
            && let Some(SyntaxRule { node, rule }) = rules.next()
        {
            match rule {
                Rule::IfDef { .. } | Rule::Sample(_) | Rule::Filter(_) => {
                    return Err(ParseError::RuleNotSupportedAfterCompute { span: node.span() });
                }
                Rule::Map(func) => aggregates.push(self.map_to_aggr(&func)?),
                Rule::Align { duration, func } => {
                    aggregates.push(self.align_to_aggr(duration, &func)?);
                }
                Rule::Group { groups, func } => aggregates.push(self.group_to_aggr(groups, &func)?),
                Rule::Bucket {
                    groups,
                    duration,
                    func,
                } => aggregates.push(self.bucket_to_aggr(groups, duration, func)?),
                _ => {
                    return Err(ParseError::RuleNotSupportedHere { span: node.span() });
                }
            }
            {}
        }
        for SyntaxRule { node, rule } in rules {
            match rule {
                Rule::IfDef { .. } | Rule::Sample(_) | Rule::Filter(_) => {
                    return Err(ParseError::RuleNotSupportedAfterCompute { span: node.span() });
                }
                // TODO: remove me maybe
                Rule::As(alias) => {
                    let name = Metric::new(&alias).map_err(|_| ParseError::InvalidMetric {
                        metric: alias.to_string(),
                        span: alias.span(),
                    })?;
                    aggregates.push(Aggregate::As(As { name }));
                }
                Rule::Extern(extend_parts) => extends.append(&mut self.extend(extend_parts)?),
                _ => return Err(ParseError::RuleNotSupportedHere { span: node.span() }),
            }
        }
        let name = Metric::new(&name).map_err(|_| ParseError::InvalidMetric {
            metric: name.to_string(),
            span: name.span(),
        })?;

        Ok(crate::Query::Compute {
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

    #[allow(clippy::too_many_lines)]
    fn bucket_to_aggr(
        &mut self,
        groups: Vec<ast::Ident>,
        duration: Option<ast::Duration>,
        func: FunctionCall,
    ) -> Result<Aggregate> {
        let span = func.span();
        let f = call_to_function(&func)?;
        let mut function =
            *self
                .stdlib
                .bucket_function(f.name.name())
                .ok_or(ParseError::UnknownFunction {
                    name: f.name.name().to_string(),
                    span: func.node.span(),
                })?;
        let time = duration
            .map(|d| match d {
                ast::Duration::Const(value) => Ok(Parameterized::Concrete(RelativeTime {
                    value,
                    unit: TimeUnit::Second,
                })),
                ast::Duration::Var(variable) => self.get_param_typed(
                    &variable,
                    ParamType::Terminal(query::TerminalParamType::Duration),
                ),
            })
            .transpose()?;
        let tags = self.groups_to_tags(groups);
        let mut itr = func.args.into_iter();
        // This is really hacky, but we need it for compatibility with the old parser
        // we can clean this up once we drop support for the old parser
        let base =
            if let BucketType::InterpolateCumulativeHistogram(conversion_method) = &mut function {
                let Some(SyntaxExpr { node, expr }) = itr.next() else {
                    return Err(ParseError::InvalidArgumentCount {
                        function: f.name.name().to_string(),
                        span,
                        expected: 2,
                        actual: 0,
                    });
                };
                match expr {
                    ast::Expr::Ident(ident) if ident.name() == "increase" => {
                        *conversion_method = ConversionMethod::Increase;
                    }

                    ast::Expr::Ident(ident) if ident.name() == "rate" => {
                        *conversion_method = ConversionMethod::Rate;
                    }
                    _ => {
                        return Err(ParseError::InvalidBucketSpec {
                            function: f.name.name().to_string(),
                            span: node.span(),
                            spec: "not increase or rate".to_string(),
                            n: 1,
                        });
                    }
                }
                2
            } else {
                1
            };
        let spec = itr
            .enumerate()
            .map(|(n, SyntaxExpr { expr, node })| match expr {
                ast::Expr::Ident(ident) if ident.name() == "avg" => Ok(BucketSpec::Avg),
                ast::Expr::Ident(ident) if ident.name() == "count" => Ok(BucketSpec::Count),
                ast::Expr::Ident(ident) if ident.name() == "sum" => Ok(BucketSpec::Sum),
                ast::Expr::Ident(ident) if ident.name() == "min" => Ok(BucketSpec::Min),
                ast::Expr::Ident(ident) if ident.name() == "max" => Ok(BucketSpec::Max),
                ast::Expr::Ident(ident) => Err(ParseError::InvalidBucketSpec {
                    function: f.name.name().to_string(),
                    spec: ident.into_string(),
                    span: node.span(),
                    n: n + base,
                }),
                ast::Expr::Const(TagValue::Float(f)) => Ok(BucketSpec::Percentile(f)),
                ast::Expr::Const(TagValue::Int(1)) => Ok(BucketSpec::Percentile(1.0)),
                ast::Expr::Const(TagValue::Int(0)) => Ok(BucketSpec::Percentile(0.0)),
                ast::Expr::Const(c) => Err(ParseError::InvalidArgumentType {
                    function: f.name.name().to_string(),
                    expected: TagType::Float,
                    actual: c.tpe(),
                    span: node.span(),
                    n: n + base,
                }),
                ast::Expr::String(_) => Err(ParseError::InvalidArgumentType {
                    function: f.name.name().to_string(),
                    expected: TagType::Float,
                    actual: TagType::String,
                    span: node.span(),
                    n: n + base,
                }),
                ast::Expr::Array(_) => Err(ParseError::InvalidArgumentType {
                    function: f.name.name().to_string(),
                    expected: TagType::Float,
                    actual: TagType::Array,
                    span: node.span(),
                    n: n + base,
                }),
                // TODO: what if we did?
                ast::Expr::Var(_) => Err(ParseError::VariablesNotSupported { span: node.span() }),
            })
            .collect::<Result<Vec<_>>>()?;

        if spec.is_empty() {
            Err(ParseError::MissingBucketSpec {
                function: f.name.name().to_string(),
                span,
            })
        } else {
            Ok(Aggregate::Bucket(BucketBy {
                span,
                function,
                time,
                tags,
                spec,
            }))
        }
    }
    fn get_param_typed<T>(&self, variable: &Variable, ty: ParamType) -> Result<Parameterized<T>> {
        let Some(param) = self.params.iter().find(|v| v.name == variable.name()) else {
            return Err(ParseError::UndefinedVariable {
                name: variable.name().to_string(),
                span: variable.span(),
            });
        };

        if param.typ != ty {
            return Err(ParseError::InvalidVariableType {
                declaration_span: param.span,
                variable_span: variable.span(),
                expected: param.typ,
                actual: ty,
            });
        }

        Ok(Parameterized::Param {
            span: variable.span(),
            param: param.clone(),
        })
    }

    fn get_param_decl(&self, variable: &Variable) -> Result<ParamDeclaration> {
        let Some(param) = self.params.iter().find(|v| v.name == variable.name()) else {
            return Err(ParseError::UndefinedVariable {
                name: variable.name().to_string(),
                span: variable.span(),
            });
        };

        Ok(param.clone())
    }

    fn group_to_aggr(&mut self, groups: Vec<ast::Ident>, func: &FunctionCall) -> Result<Aggregate> {
        let span = func.node.span();
        let f = call_to_function(func)?;
        let function = self
            .stdlib
            .group_fn(&f)
            .ok_or(ParseError::UnknownFunction {
                name: f.name.name().to_string(),
                span: func.node.span(),
            })?
            .clone();
        if func.args.len() != function.args().len() || !func.args.is_empty() {
            return Err(ParseError::InvalidArgumentCount {
                function: f.name.name().to_string(),
                expected: function.args().len(),
                actual: func.args.len(),
                span: func.node.span(),
            });
        }
        let tags = self.groups_to_tags(groups);
        Ok(Aggregate::GroupBy(GroupBy {
            span,
            function,
            tags,
        }))
    }

    fn groups_to_tags(&mut self, groups: Vec<Ident>) -> Vec<String> {
        let mut res = Vec::new();
        for g in groups {
            if res.iter().any(|i| i == g.name()) {
                self.warnings.push(Warning::DuplicatedGroup {
                    name: g.to_string(),
                    span: g.span(),
                });
            }
            res.push(g.to_string());
        }
        res
    }

    fn align_to_aggr(
        &self,
        duration: Option<ast::Duration>,
        func: &FunctionCall,
    ) -> Result<Aggregate> {
        let f = call_to_function(func)?;
        let function = self
            .stdlib
            .align_fn(&f)
            .ok_or(ParseError::UnknownFunction {
                name: f.name.name().to_string(),
                span: func.node.span(),
            })?
            .clone();
        let time = duration
            .map(|d| match d {
                ast::Duration::Const(value) => Ok(Parameterized::Concrete(RelativeTime {
                    value,
                    unit: TimeUnit::Second,
                })),
                ast::Duration::Var(variable) => self.get_param_typed(
                    &variable,
                    ParamType::Terminal(query::TerminalParamType::Duration),
                ),
            })
            .transpose()?;

        if func.args.len() != function.args().len() || !func.args.is_empty() {
            return Err(ParseError::InvalidArgumentCount {
                function: f.name.name().to_string(),
                expected: function.args().len(),
                actual: func.args.len(),
                span: func.node.span(),
            });
        }

        Ok(Aggregate::Align(Align { function, time }))
    }

    fn map_to_aggr(&self, func: &FunctionCall) -> Result<Aggregate> {
        let f = call_to_function(func)?;
        let function = self.stdlib.map_fn(&f).ok_or(ParseError::UnknownFunction {
            name: f.name.name().to_string(),
            span: func.node.span(),
        })?;
        if func.args.len() != function.args().len() || func.args.len() > 1 {
            return Err(ParseError::InvalidArgumentCount {
                function: f.name.name().to_string(),
                expected: function.args().len(),
                actual: func.args.len(),
                span: func.node.span(),
            });
        }
        let arg = if let Some(SyntaxExpr { node, expr }) = func.args.first() {
            match expr {
                #[allow(clippy::cast_precision_loss)] // we accept ints as floats
                ast::Expr::Const(TagValue::Int(f)) => Some(*f as f64),
                ast::Expr::Const(TagValue::Float(f)) => Some(*f),
                ast::Expr::Var(_) => {
                    return Err(ParseError::VariablesNotSupported { span: node.span() });
                }
                ast::Expr::Array(_) => {
                    return Err(ParseError::InvalidArgumentType {
                        function: f.name.name().to_string(),
                        expected: TagType::Float,
                        actual: TagType::Array,
                        n: 1,
                        span: node.span(),
                    });
                }
                ast::Expr::Const(c) => {
                    return Err(ParseError::InvalidArgumentType {
                        function: f.name.name().to_string(),
                        expected: TagType::Float,
                        actual: c.tpe(),
                        n: 1,
                        span: node.span(),
                    });
                }
                ast::Expr::String(_) | ast::Expr::Ident(_) => {
                    return Err(ParseError::InvalidArgumentType {
                        function: f.name.name().to_string(),
                        expected: TagType::Float,
                        actual: TagType::String,
                        n: 1,
                        span: node.span(),
                    });
                }
            }
        } else {
            None
        };
        Ok(Aggregate::Map(query::Mapping {
            function: function.clone(),
            arg,
        }))
    }

    fn extend(&self, extend_parts: Vec<ast::ExtendPart>) -> Result<Vec<TagExtend>> {
        extend_parts
            .into_iter()
            .map(|p| {
                Ok(TagExtend {
                    tag: p.name.to_string(),
                    value: self.expr(p.value)?,
                })
            })
            .collect()
    }

    fn expr(&self, SyntaxExpr { node: _, expr }: SyntaxExpr) -> Result<query::Expr> {
        match expr {
            ast::Expr::Ident(ident) => Ok(query::Expr::Tag(ident.to_string())),
            ast::Expr::String(string_parts) => Ok(query::Expr::String(
                string_parts
                    .into_iter()
                    .map(|p| match p {
                        ast::StringPart::Const(s) => Ok(query::StringFragment::Text(s)),
                        ast::StringPart::Expr(e) => Ok(query::StringFragment::Expr(self.expr(e)?)),
                    })
                    .collect::<Result<Vec<query::StringFragment>>>()?,
            )),
            ast::Expr::Var(variable) => Ok(query::Expr::Param {
                span: variable.span(),
                param: self.get_param_decl(&variable)?,
            }),
            ast::Expr::Const(tag_value) => Ok(query::Expr::Const(tag_value)),
            ast::Expr::Array(syntax_exprs) => Ok(query::Expr::Array(
                syntax_exprs
                    .into_iter()
                    .map(|e| self.expr(e))
                    .collect::<Result<Vec<query::Expr>>>()?,
            )),
        }
    }

    fn filter(&self, FilterOr(fs): FilterOr) -> Result<Filter> {
        let mut fs = fs
            .into_iter()
            .map(|f| self.filter_and(f))
            .collect::<Result<Vec<Filter>>>()?;
        if fs.len() == 1
            && let Some(f) = fs.pop()
        {
            Ok(f)
        } else {
            Ok(Filter::Or(fs))
        }
    }
    fn filter_and(&self, FilterAnd(fs): FilterAnd) -> Result<Filter> {
        let mut fs = fs
            .into_iter()
            .map(|f| self.filter_not(f))
            .collect::<Result<Vec<Filter>>>()?;
        if fs.len() == 1
            && let Some(f) = fs.pop()
        {
            Ok(f)
        } else {
            Ok(Filter::And(fs))
        }
    }
    fn filter_not(&self, f: FilterNot) -> Result<Filter> {
        match f {
            FilterNot::Not(f) => Ok(Filter::Not(Box::new(self.filter_paren(f)?))),
            FilterNot::Yes(f) => self.filter_paren(f),
        }
    }
    fn filter_paren(&self, f: FilterParen) -> Result<Filter> {
        match f {
            FilterParen::Paren(f) => self.filter(*f),
            FilterParen::Cmp(f) => self.filter_cmp(f),
        }
    }
    /// Lowers `field in rhs`, which holds only when `rhs` names a set of values.
    fn in_cmp(&self, lhs: Ident, rhs: SyntaxExpr) -> Result<Filter> {
        let span = rhs.node.span();
        let value = self.expr(rhs)?;
        if !value.is_array() {
            return Err(ParseError::InRequiresArray { span });
        }
        Ok(Filter::Cmp {
            field: lhs.into_string(),
            rhs: Cmp::In(value),
        })
    }

    fn filter_cmp(&self, f: FilterCmp) -> Result<Filter> {
        let r = match f {
            FilterCmp::Eq {
                lhs,
                rhs:
                    SyntaxExpr {
                        node,
                        expr: ast::Expr::Var(v),
                    },
            } => {
                let decl = self.get_param_decl(&v)?;
                if decl.typ() == TerminalParamType::Regex {
                    Filter::Cmp {
                        field: lhs.into_string(),
                        rhs: Cmp::RegEx(Parameterized::Param {
                            span: node.span(),
                            param: decl,
                        }),
                    }
                } else {
                    Filter::Cmp {
                        field: lhs.into_string(),
                        rhs: Cmp::Eq(self.expr(SyntaxExpr {
                            node,
                            expr: ast::Expr::Var(v),
                        })?),
                    }
                }
            }
            FilterCmp::Eq { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Eq(self.expr(rhs)?),
            },
            FilterCmp::Neq {
                lhs,
                rhs:
                    SyntaxExpr {
                        node,
                        expr: ast::Expr::Var(v),
                    },
            } => {
                let decl = self.get_param_decl(&v)?;
                if decl.typ() == TerminalParamType::Regex {
                    Filter::Cmp {
                        field: lhs.into_string(),
                        rhs: Cmp::RegExNot(Parameterized::Param {
                            span: node.span(),
                            param: decl,
                        }),
                    }
                } else {
                    Filter::Cmp {
                        field: lhs.into_string(),
                        rhs: Cmp::Ne(self.expr(SyntaxExpr {
                            node,
                            expr: ast::Expr::Var(v),
                        })?),
                    }
                }
            }

            FilterCmp::Neq { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Ne(self.expr(rhs)?),
            },
            FilterCmp::Lt { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Lt(self.expr(rhs)?),
            },
            FilterCmp::Gt { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Gt(self.expr(rhs)?),
            },
            FilterCmp::Lte { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Le(self.expr(rhs)?),
            },
            FilterCmp::Gte { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Ge(self.expr(rhs)?),
            },
            FilterCmp::In { lhs, rhs } => self.in_cmp(lhs, rhs)?,
            FilterCmp::EqRe { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::RegEx(Parameterized::Concrete(rhs.into())),
            },
            FilterCmp::NeqRe { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::RegExNot(Parameterized::Concrete(rhs.into())),
            },
            FilterCmp::Is { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Is(rhs),
            },
        };
        Ok(r)
    }
}

/// The declarations a host's registrations stand for.
///
/// The reserved prefix is what separates them from the names a query may declare, so a
/// registration that omits it is reported to the host rather than quietly becoming an
/// ordinary param.
fn system_param_declarations<H: BuildHasher>(
    system_params: HashMap<String, ParamType, H>,
    errors: &mut Vec<ParseError>,
) -> Vec<ParamDeclaration> {
    system_params
        .into_iter()
        .filter_map(|(name, typ)| {
            if !name.starts_with(SYSTEM_PARAM_PREFIX) {
                errors.push(ParseError::SystemParamMissingPrefix { param: name });
                return None;
            }
            Some(ParamDeclaration {
                span: SourceSpan::new(0.into(), 0),
                name,
                typ,
                system: true,
            })
        })
        .collect()
}

fn call_to_function(f: &FunctionCall) -> Result<Function> {
    let Some((name, module_path)) = f.name.split_last() else {
        return Err(ParseError::UnknownFunction {
            span: f.node.span(),
            name: String::new(),
        });
    };
    let name = FunctionId::new(name);
    let module_path = module_path.iter().map(|p| ModuleId::new(p)).collect();
    Ok(Function { module_path, name })
}
