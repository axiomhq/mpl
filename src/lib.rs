//! The `MPL` query language
#![deny(
    warnings,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::large_futures,
    missing_docs
)]
#![allow(clippy::missing_errors_doc)]
#![allow(unused_assignments)] // We need this for the type error

mod parser;
pub use parser::{MPLParser, Rule};

pub mod enc_regex;
pub mod errors;
pub mod linker;
pub mod query;
mod stdlib;
pub mod tags;
pub mod time;
pub mod types;
pub mod visitor;

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};

pub use errors::ParseError;
use miette::{Diagnostic, SourceOffset, SourceSpan};
use pest::Parser as _;
pub use query::Query;

pub use stdlib::STDLIB;

use crate::{
    query::{Cmp, Expr, Filter, ParamDeclaration, ParamType, TagType, TerminalParamType, Warnings},
    types::{Dataset, Parameterized},
    visitor::{QueryVisitor, QueryWalker, VisitRes},
};

/// Compile error
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum CompileError {
    /// Parse error
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(#[from] ParseError),
    /// Typecheck error
    #[error(transparent)]
    #[diagnostic(transparent)]
    Type(#[from] TypeError),
    /// Groupcheck error
    #[error(transparent)]
    #[diagnostic(transparent)]
    Group(#[from] GroupError),

    /// Option error
    #[error(transparent)]
    #[diagnostic(transparent)]
    Ifdef(#[from] IfdefError),
}

/// Parses and typechecks an MPL query into a Query object.
#[allow(clippy::result_large_err)]
pub fn compile<S: BuildHasher>(
    query: &str,
    system_params: HashMap<String, ParamType, S>,
) -> Result<(Query, Warnings), CompileError> {
    // stage 1: parse
    let mut parse = MPLParser::parse(Rule::file, query).map_err(ParseError::from)?;
    let (mut query, warnings) = parser::Parser::default().parse_query(&mut parse, system_params)?;
    // stage 2: typecheck
    let mut visitor = ParamTypecheckVisitor {};
    visitor.walk(&mut query)?;
    // stage 3: group check
    let mut visitor = GroupCheckVisitor::default();
    visitor.walk(&mut query)?;

    let mut visitor = OptionCheckVisitor::default();
    visitor.walk(&mut query)?;

    Ok((query, warnings))
}
/// Type error
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum GroupError {
    /// groups are not a subset of the previous groups
    #[error("invalid groups: {next_groups:?} is not a subset of {prev_groups:?}")]
    InvalidGroups {
        /// the previous groups
        next_groups: HashSet<String>,
        /// the location of the next groups
        next_span: Box<SourceSpan>,
        /// the current groups
        prev_groups: HashSet<String>,
        /// the location of the previous groups
        prev_span: Box<SourceSpan>,
    },
}

#[derive(Default)]
struct OptionCheckVisitor {
    ifdef_param: Option<ParamDeclaration>,
    seen_param: Option<ParamDeclaration>,
}

/// Ifdef error
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum IfdefError {
    /// Usage of optional parameter outside of ifdef
    #[error("{} is optional and used outside of ifdef", param.name)]
    OptionalOutsideOfIfdef {
        /// The source location
        #[label("{}", param.name)]
        span: SourceSpan,
        /// The param declaration
        param: ParamDeclaration,
    },
    /// Usage of optional parameter when it's not referenced
    #[error("{} is used in a ifdef guard but not referenced inside of it", param.name)]
    OptionalNotUsed {
        /// The source location
        #[label("{}", param.name)]
        span: SourceSpan,
        /// The param declaration
        param: ParamDeclaration,
    },
}

impl QueryVisitor for OptionCheckVisitor {
    type Error = IfdefError;
    fn visit_ifdef(
        &mut self,
        param: &mut ParamDeclaration,
        _filter: &mut Filter,
        _else_filter: &mut Option<Filter>,
    ) -> Result<VisitRes, Self::Error> {
        self.ifdef_param = Some(param.clone());
        self.seen_param = None;
        Ok(VisitRes::Walk)
    }
    fn leave_ifdef(
        &mut self,
        param: &mut ParamDeclaration,
        _filter: &mut Filter,
        _else_filter: &mut Option<Filter>,
    ) -> Result<(), Self::Error> {
        if self.ifdef_param != self.seen_param {
            return Err(IfdefError::OptionalNotUsed {
                span: param.span,
                param: param.clone(),
            });
        }
        self.ifdef_param = None;
        Ok(())
    }
    fn visit_expr(&mut self, value: &mut Expr) -> Result<VisitRes, Self::Error> {
        if let Expr::Param { span, param } = value
            && param.is_optional()
        {
            self.seen_param = Some(param.clone());
            if self.seen_param != self.ifdef_param {
                return Err(IfdefError::OptionalOutsideOfIfdef {
                    span: *span,
                    param: param.clone(),
                });
            }
        }
        Ok(VisitRes::Walk)
    }
    fn visit_parameterized_regex(
        &mut self,
        regex: &mut Parameterized<enc_regex::EncodableRegex>,
    ) -> Result<VisitRes, Self::Error> {
        if let Parameterized::Param { span, param } = regex
            && param.is_optional()
        {
            self.seen_param = Some(param.clone());
            if self.seen_param != self.ifdef_param {
                return Err(IfdefError::OptionalOutsideOfIfdef {
                    span: *span,
                    param: param.clone(),
                });
            }
        }
        Ok(VisitRes::Walk)
    }
}

impl QueryWalker for OptionCheckVisitor {}

struct GroupCheckVisitor {
    groups: Option<HashSet<String>>,
    span: SourceSpan,
    stack: Vec<(SourceSpan, Option<HashSet<String>>)>,
}

impl Default for GroupCheckVisitor {
    fn default() -> Self {
        Self {
            groups: None,
            span: SourceSpan::new(SourceOffset::from_location("", 0, 0), 0),
            stack: Vec::new(),
        }
    }
}
impl GroupCheckVisitor {
    fn check_group_by(
        &mut self,
        tags: &[String],
        span: SourceSpan,
    ) -> Result<VisitRes, GroupError> {
        let next_groups: HashSet<String> = tags.iter().cloned().collect();
        let Some(prev_groups) = self.groups.take() else {
            self.groups = Some(next_groups);
            self.span = span;
            return Ok(VisitRes::Walk);
        };
        if !next_groups.is_subset(&prev_groups) {
            return Err(GroupError::InvalidGroups {
                next_groups,
                next_span: Box::new(span),
                prev_groups,
                prev_span: Box::new(self.span),
            });
        }
        self.groups = Some(next_groups);
        self.span = span;
        Ok(VisitRes::Walk)
    }
}

impl QueryVisitor for GroupCheckVisitor {
    type Error = GroupError;
    fn visit(&mut self, _: &mut Query) -> Result<VisitRes, Self::Error> {
        self.stack.push((self.span, self.groups.take()));
        Ok(VisitRes::Walk)
    }
    fn leave(&mut self, _: &mut Query) -> Result<(), Self::Error> {
        let Some((span, groups)) = self.stack.pop() else {
            return Ok(());
        };
        self.span = span;
        self.groups = groups;
        Ok(())
    }
    fn visit_group_by(&mut self, group_by: &mut query::GroupBy) -> Result<VisitRes, Self::Error> {
        self.check_group_by(&group_by.tags, group_by.span)
    }
    fn visit_bucket_by(
        &mut self,
        bucket_by: &mut query::BucketBy,
    ) -> Result<VisitRes, Self::Error> {
        self.check_group_by(&bucket_by.tags, bucket_by.span)
    }
}
impl QueryWalker for GroupCheckVisitor {}

/// Type error
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum TypeError {
    /// Type mismatch
    #[error(
        "The param ${param_name} has type {actual}, but was used in context that expects one of: {}",
        expected.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    )]
    #[diagnostic(code(mpl_lang::typemismatch))]
    #[allow(unused_assignments)]
    TypeMismatch {
        /// The location of the param used
        #[label("param")]
        use_span: SourceSpan,
        /// The location where the param was declared
        #[label("param declaration")]
        declaration_span: SourceSpan,
        /// The param name
        param_name: String,
        /// The expected type(s)
        expected: Vec<TerminalParamType>,
        /// The actual type
        actual: TerminalParamType,
    },
}

struct ParamTypecheckVisitor {}

impl ParamTypecheckVisitor {
    fn assert_param_type(
        param: &ParamDeclaration,
        use_span: SourceSpan,
        expected: Vec<TerminalParamType>,
    ) -> Result<(), TypeError> {
        if !expected.contains(&param.typ()) {
            return Err(TypeError::TypeMismatch {
                use_span,
                declaration_span: param.span,
                param_name: param.name.clone(),
                expected,
                actual: param.typ(),
            });
        }

        Ok(())
    }

    fn assert_param<T>(
        value: &Parameterized<T>,
        expected: Vec<TerminalParamType>,
    ) -> Result<(), TypeError> {
        let Parameterized::Param { span, param } = value else {
            return Ok(());
        };
        Self::assert_param_type(param, *span, expected)
    }
}

impl QueryVisitor for ParamTypecheckVisitor {
    type Error = TypeError;

    fn visit_dataset(
        &mut self,
        dataset: &mut Parameterized<Dataset>,
    ) -> Result<VisitRes, Self::Error> {
        Self::assert_param(dataset, vec![TerminalParamType::Dataset]).map(|()| VisitRes::Walk)
    }

    fn visit_align(&mut self, align: &mut query::Align) -> Result<VisitRes, Self::Error> {
        if let Some(time) = &align.time {
            Self::assert_param(time, vec![TerminalParamType::Duration]).map(|()| VisitRes::Walk)
        } else {
            Ok(VisitRes::Walk)
        }
    }

    fn visit_bucket_by(
        &mut self,
        bucket_by: &mut query::BucketBy,
    ) -> Result<VisitRes, Self::Error> {
        if let Some(time) = &bucket_by.time {
            Self::assert_param(time, vec![TerminalParamType::Duration]).map(|()| VisitRes::Walk)
        } else {
            Ok(VisitRes::Walk)
        }
    }

    fn visit_cmp(&mut self, _field: &mut String, cmp: &mut Cmp) -> Result<VisitRes, Self::Error> {
        let tag_value_param_types = vec![
            TerminalParamType::Tag(TagType::String),
            TerminalParamType::Tag(TagType::Int),
            TerminalParamType::Tag(TagType::Float),
            TerminalParamType::Tag(TagType::Bool),
        ];

        match cmp {
            Cmp::Is(_)
            | Cmp::Eq(Expr::Const(_) | Expr::String(_) | Expr::Tag(_))
            | Cmp::Ne(Expr::Const(_) | Expr::String(_) | Expr::Tag(_))
            | Cmp::Gt(Expr::Const(_) | Expr::String(_) | Expr::Tag(_))
            | Cmp::Ge(Expr::Const(_) | Expr::String(_) | Expr::Tag(_))
            | Cmp::Lt(Expr::Const(_) | Expr::String(_) | Expr::Tag(_))
            | Cmp::Le(Expr::Const(_) | Expr::String(_) | Expr::Tag(_)) => Ok(VisitRes::Walk),
            Cmp::Eq(Expr::Param { span, param }) => {
                if param.typ() == TerminalParamType::Regex {
                    // we have a regex param in an eq
                    // this happens because we cannot detect this in pest
                    //
                    // this is | filter foo == #/bar/ vs | filter foo == $bar_re
                    *cmp = Cmp::RegEx(Parameterized::Param {
                        span: *span,
                        param: param.clone(),
                    });
                    return Ok(VisitRes::Walk);
                }

                Self::assert_param_type(param, *span, tag_value_param_types)
                    .map(|()| VisitRes::Walk)
            }
            Cmp::Ne(Expr::Param { span, param }) => {
                if param.typ() == TerminalParamType::Regex {
                    // we have a regex param in ne
                    // this happens because we cannot detect this in pest
                    //
                    // this is | filter foo != #/bar/ vs | filter foo != $bar_re
                    *cmp = Cmp::RegExNot(Parameterized::Param {
                        span: *span,
                        param: param.clone(),
                    });
                    return Ok(VisitRes::Walk);
                }

                Self::assert_param_type(param, *span, tag_value_param_types)
                    .map(|()| VisitRes::Walk)
            }
            Cmp::Gt(Expr::Param { span, param })
            | Cmp::Ge(Expr::Param { span, param })
            | Cmp::Lt(Expr::Param { span, param })
            | Cmp::Le(Expr::Param { span, param }) => {
                Self::assert_param_type(param, *span, tag_value_param_types)
                    .map(|()| VisitRes::Walk)
            }

            Cmp::RegEx(value) | Cmp::RegExNot(value) => {
                Self::assert_param(value, vec![TerminalParamType::Regex]).map(|()| VisitRes::Walk)
            }
        }
    }
}

impl QueryWalker for ParamTypecheckVisitor {}

#[cfg(feature = "examples")]
pub mod examples {
    //! Examples used in tests and documentation

    macro_rules! example {
        ($name:expr) => {
            (
                concat!($name),
                include_str!(concat!("../tests/examples/", $name, ".mpl")),
            )
        };
    }

    /// Language specification
    pub const SPEC: &str = include_str!("../spec.md");

    /// MPL examples used in tests and documentation
    pub const MPL: [(&str, &str); 22] = [
        example!("align-rate"),
        example!("as"),
        example!("enrich"),
        example!("extend"),
        example!("filtered-histogram"),
        example!("group-by"),
        example!("histogram"),
        example!("histogram_rate"),
        example!("ifdef"),
        example!("ifdef-else"),
        example!("inf"),
        example!("map-gt"),
        example!("map-mul"),
        example!("nested-enrich"),
        example!("parser-error"),
        example!("rate"),
        example!("replace_labels"),
        example!("set"),
        example!("slo"),
        example!("slo-histogram"),
        example!("slo-ingest-rate"),
        example!("sum_rate"),
    ];
}
