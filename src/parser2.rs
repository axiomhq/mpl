use std::collections::HashMap;

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
        self, Aggregate, Align, BucketBy, Cmp, DirectiveValue, Directives, Filter, FilterOrIfDef,
        GroupBy, MetricId, ParamDeclaration, ParamType, Params, RelativeTime, Source, TagExtend,
        TagType, TimeUnit,
    },
    tags::TagValue,
    types::{BucketSpec, Dataset, Metric, Parameterized},
};

type Result<T, E = ParseError> = std::result::Result<T, E>;

/// `MPL` parsing error
#[derive(thiserror::Error, Debug, Diagnostic)]
pub enum ParseError {
    /// This part is unimplemented
    #[error("Not implemented")]
    Unimplemented,
    /// AST errors
    #[error("AST errors")]
    AST(
        /// prior AST errors
        AstError,
    ),
    #[error("Invalid value for directive")]
    InvalidDirectiveValue {
        value: TagValue,
        #[label("Invalid directive value: {value}")]
        span: SourceSpan,
    },
    #[error("No query was provided")]
    MissingQuery,
    #[error("Directive in the wrong place")]
    DirectiveInWongPlace {
        #[label("You can not place a directive here")]
        span: SourceSpan,
    },
    #[error("Rule not supported after compute")]
    RuleNotSupportedAfterCompute {
        #[label("Rule not supported after compute")]
        span: SourceSpan,
    },
    #[error("Unknown function: {name}")]
    UnknownFunction {
        name: String,
        #[label("Unknown function: {name}")]
        span: SourceSpan,
    },
    #[error("Invalid argument count for function: {function} (expected {expected}, got {actual})")]
    InvalidArgumentCount {
        function: String,
        expected: usize,
        actual: usize,
        #[label(
            "Invalid argument count for function: {function} (expected {expected}, got {actual})"
        )]
        span: SourceSpan,
    },
    #[error(
        "Invalid argument type for argument {n} of function: {function} (expected {expected}, got {actual})"
    )]
    InvalidArgumentType {
        function: String,
        expected: TagType,
        actual: TagType,
        n: usize,
        #[label(
            "Invalid argument type for argument {n} of function: {function} (expected {expected}, got {actual})"
        )]
        span: SourceSpan,
    },
    #[error("Undefined variable: {name}")]
    UndefinedVariable {
        name: String,
        #[label("Undefined variable: {name}")]
        span: SourceSpan,
    },
    #[error("Invalid variable type: expected {expected}, got {actual}")]
    InvalidVariableType {
        #[label("Invalid variable type, expected {expected}")]
        variable_span: SourceSpan,
        #[label("The variable was declared here as {actual}")]
        declaration_span: SourceSpan,
        expected: query::ParamType,
        actual: query::ParamType,
    },
    #[error("Variables are not supported here")]
    VariablesNotSupported {
        #[label("Variables are not supported here")]
        span: SourceSpan,
    },
    #[error("Invalid bucket spec for function: {function} ({spec})")]
    InvalidBucketSpec {
        function: String,
        spec: String,
        n: usize,
        #[label("Invalid bucket spec for function: {function} ({spec})")]
        span: SourceSpan,
    },
    #[error("Invalid metric: {metric}")]
    InvalidMetric {
        metric: String,
        #[label("Invalid metric: {metric}")]
        span: SourceSpan,
    },
    #[error("Expected a filter")]
    ExpectedFilter { span: SourceSpan },
    #[error("Parameter in ifdef must be optional")]
    MustBeOptional {
        name: String,
        #[label("Parameter `{name}` must be optional")]
        span: SourceSpan,
        #[label("Parameter declared here")]
        decl_span: SourceSpan,
    },
}
#[derive(thiserror::Error, Debug, Diagnostic)]
pub enum Warning {
    #[error("AST warning")]
    Ast(AstWarning),
}
/// Parser from AST -> Query
pub struct Parser {
    stdlib: &'static Module,
    ast: Ast,
}

impl Parser {
    pub fn new(ast: Ast) -> Self {
        Parser {
            ast,
            stdlib: &STDLIB,
        }
    }

    pub fn parse(self) -> Result<crate::Query, Vec<ParseError>> {
        let Ast {
            errors,
            warnings,
            mut parts,
        } = self.ast;
        let mut errors: Vec<_> = errors.into_iter().map(ParseError::AST).collect();
        let _warnings: Vec<_> = warnings.into_iter().map(Warning::Ast).collect();
        let mut directives = HashMap::new();
        // we rever so we can pop the content
        parts.reverse();
        while parts.last().is_some_and(|p| p.is_directive()) {
            let Some(Part::Directive(d)) = parts.pop() else {
                continue;
            };
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

        let mut params = Vec::new();
        while parts.last().is_some_and(Part::is_param) {
            let Some(Part::Param(p)) = parts.pop() else {
                continue;
            };
            params.push(ParamDeclaration {
                span: p.node.span(),
                name: p.name.to_string(),
                typ: p.ty,
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
                };
                Ok(p.query(q).unwrap())
            }
            Some(Part::Directive(d)) => {
                errors.push(ParseError::DirectiveInWongPlace {
                    span: d.node.span(),
                });
                Err(errors)
            }
            Some(Part::Param(_)) => {
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
}

impl QueryParser {
    fn query(&self, q: AstQuery) -> Result<crate::Query> {
        match q {
            AstQuery::Compute(c) => self.compute_query(*c),
            AstQuery::Simple(s) => self.simple_query(s),
        }
    }

    fn simple_query(
        &self,
        SimpleQuery {
            node,
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
        let mut extends = Vec::new();
        let mut filters = Vec::new();
        let mut sample = None;
        for SyntaxRule { node, rule } in rules {
            match rule {
                Rule::Sample(s) => sample = Some(s),
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
                    })
                }
                Rule::As(_) => {
                    return Err(ParseError::RuleNotSupportedAfterCompute { span: node.span() });
                }
                Rule::Map(func) => aggregates.push(self.map_to_aggr(func)?),
                Rule::Align { duration, func } => {
                    aggregates.push(self.align_to_aggr(duration, func)?);
                }
                Rule::Group { groups, func } => aggregates.push(self.group_to_aggr(groups, func)?),
                Rule::Bucket {
                    groups,
                    duration,
                    func,
                } => aggregates.push(self.bucket_to_aggr(groups, duration, func)?),
                Rule::Extern(extend_parts) => extends.append(&mut self.parse_extend(extend_parts)?),
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
        &self,
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
        let op = self.stdlib.compute_fn(&f).unwrap().clone();

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
        for SyntaxRule { node, rule } in rules {
            match rule {
                Rule::As(_) | Rule::IfDef { .. } | Rule::Sample(_) | Rule::Filter(_) => {
                    return Err(ParseError::RuleNotSupportedAfterCompute { span: node.span() });
                }
                Rule::Map(func) => aggregates.push(self.map_to_aggr(func)?),
                Rule::Align { duration, func } => {
                    aggregates.push(self.align_to_aggr(duration, func)?)
                }
                Rule::Group { groups, func } => aggregates.push(self.group_to_aggr(groups, func)?),
                Rule::Bucket {
                    groups,
                    duration,
                    func,
                } => aggregates.push(self.bucket_to_aggr(groups, duration, func)?),
                Rule::Extern(extend_parts) => extends.append(&mut self.parse_extend(extend_parts)?),
            }
        }

        Ok(crate::Query::Compute {
            left,
            right,
            name: Metric::new(&name).unwrap(),
            op,
            aggregates,
            extends,
            directives: self.directives.clone(),
            params: self.params.clone(),
        })
    }
    fn bucket_to_aggr(
        &self,
        groups: Vec<ast::Ident>,
        duration: Option<ast::Duration>,
        func: FunctionCall,
    ) -> Result<Aggregate> {
        let span = func.node.span();
        let f = call_to_function(&func)?;
        let function = self
            .stdlib
            .bucket_function(f.name.name())
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
        let tags = groups
            .into_iter()
            .map(|g| g.into_string())
            .collect::<Vec<_>>();
        let spec = func
            .args
            .into_iter()
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
                    n: n + 1,
                }),
                ast::Expr::Const(TagValue::Float(f)) => Ok(BucketSpec::Percentile(f)),
                ast::Expr::Const(TagValue::Int(1)) => Ok(BucketSpec::Percentile(1.0)),
                ast::Expr::Const(TagValue::Int(0)) => Ok(BucketSpec::Percentile(0.0)),
                ast::Expr::Const(c) => Err(ParseError::InvalidArgumentType {
                    function: f.name.name().to_string(),
                    expected: TagType::Float,
                    actual: c.tpe(),
                    span: node.span(),
                    n: n + 1,
                }),
                ast::Expr::String(_) => Err(ParseError::InvalidArgumentType {
                    function: f.name.name().to_string(),
                    expected: TagType::Float,
                    actual: TagType::String,
                    span: node.span(),
                    n: n + 1,
                }),
                ast::Expr::Array(_) => Err(ParseError::InvalidArgumentType {
                    function: f.name.name().to_string(),
                    expected: TagType::Float,
                    actual: TagType::Array,
                    span: node.span(),
                    n: n + 1,
                }),
                ast::Expr::Var(_) => Err(ParseError::VariablesNotSupported { span: node.span() }),
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Aggregate::Bucket(BucketBy {
            span,
            function,
            time,
            tags,
            spec,
        }))
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

    fn group_to_aggr(&self, groups: Vec<ast::Ident>, func: FunctionCall) -> Result<Aggregate> {
        let span = func.node.span();
        let f = call_to_function(&func)?;
        let function = self
            .stdlib
            .group_fn(&f)
            .ok_or(ParseError::UnknownFunction {
                name: f.name.name().to_string(),
                span: func.node.span(),
            })?
            .clone();
        let tags = groups
            .into_iter()
            .map(|g| g.to_string())
            .collect::<Vec<_>>();
        Ok(Aggregate::GroupBy(GroupBy {
            span,
            function,
            tags,
        }))
    }

    fn align_to_aggr(
        &self,
        duration: Option<ast::Duration>,
        func: FunctionCall,
    ) -> Result<Aggregate> {
        let f = call_to_function(&func)?;
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

        Ok(Aggregate::Align(Align { function, time }))
    }

    fn map_to_aggr(&self, func: FunctionCall) -> Result<Aggregate> {
        let f = call_to_function(&func)?;
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
                ast::Expr::Const(TagValue::Int(f)) => Some(*f as f64),
                ast::Expr::Const(TagValue::Float(f)) => Some(*f as f64),
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

    fn parse_extend(&self, extend_parts: Vec<ast::ExtendPart>) -> Result<Vec<TagExtend>> {
        extend_parts
            .into_iter()
            .map(|p| {
                Ok(TagExtend {
                    tag: p.name.to_string(),
                    value: self.parse_expr(p.value)?,
                })
            })
            .collect()
    }

    fn parse_expr(&self, SyntaxExpr { node, expr }: SyntaxExpr) -> Result<query::Expr> {
        match expr {
            ast::Expr::Ident(ident) => Ok(query::Expr::Tag(ident.to_string())),
            ast::Expr::String(string_parts) => Ok(query::Expr::String(
                string_parts
                    .into_iter()
                    .map(|p| match p {
                        ast::StringPart::Const(s) => Ok(query::StringFragment::Text(s)),
                        ast::StringPart::Expr(e) => {
                            Ok(query::StringFragment::Expr(self.parse_expr(e)?))
                        }
                    })
                    .collect::<Result<Vec<query::StringFragment>>>()?,
            )),
            ast::Expr::Var(variable) => Ok(query::Expr::Param {
                span: node.span(),
                param: self.get_param_decl(&variable)?,
            }),
            ast::Expr::Const(tag_value) => Ok(query::Expr::Const(tag_value)),
            ast::Expr::Array(syntax_exprs) => Ok(query::Expr::Array(
                syntax_exprs
                    .into_iter()
                    .map(|e| self.parse_expr(e))
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
    fn filter_cmp(&self, f: FilterCmp) -> Result<Filter> {
        let r = match f {
            FilterCmp::Eq { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Eq(self.parse_expr(rhs)?),
            },
            FilterCmp::Neq { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Ne(self.parse_expr(rhs)?),
            },
            FilterCmp::Lt { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Lt(self.parse_expr(rhs)?),
            },
            FilterCmp::Gt { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Gt(self.parse_expr(rhs)?),
            },
            FilterCmp::Lte { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Le(self.parse_expr(rhs)?),
            },
            FilterCmp::Gte { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Ge(self.parse_expr(rhs)?),
            },
            FilterCmp::In { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::In(self.parse_expr(rhs)?),
            },
            FilterCmp::EqRe { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::RegEx(self.parse_expr(rhs)?),
            },
            FilterCmp::NeqRe { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::RegExNot(self.parse_expr(rhs)?),
            },
            FilterCmp::Is { lhs, rhs } => Filter::Cmp {
                field: lhs.into_string(),
                rhs: Cmp::Is(self.parse_expr(rhs)?),
            },
        };
        Ok(r)
    }
}

fn call_to_function(f: &FunctionCall) -> Result<Function> {
    let Some((name, module_path)) = f.name.split_last() else {
        return Err(ParseError::UnknownFunction {
            span: f.node.span(),
            name: "".to_string(),
        });
    };
    let name = FunctionId::new(name);
    let module_path = module_path.iter().map(|p| ModuleId::new(p)).collect();
    Ok(Function { module_path, name })
}
