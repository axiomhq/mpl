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

#[allow(missing_docs)]
pub mod parser;

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

#[cfg(feature = "wasm")]
pub mod wasm;

use std::collections::HashSet;

pub use errors::ParseError;
use miette::{Diagnostic, SourceOffset, SourceSpan};
use parser::{MPLParser, Rule};
use pest::Parser as _;
pub use query::Query;

pub use stdlib::STDLIB;

use crate::{
    query::{Cmp, ParamType, TagType},
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
}

/// Parses and typechecks an MPL query into a Query object.
#[allow(clippy::result_large_err)]
pub fn compile(query: &str) -> Result<Query, CompileError> {
    // stage 1: parse
    let mut parse = MPLParser::parse(Rule::file, query).map_err(ParseError::from)?;
    let mut query = parser::Parser::default().parse_query(&mut parse)?;
    // stage 2: typecheck
    let mut visitor = ParamTypecheckVisitor {};
    visitor.walk(&mut query)?;
    // stage 3: group check
    let mut visitor = GroupCheckVisitor::default();
    visitor.walk(&mut query)?;

    Ok(query)
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
        expected: Vec<ParamType>,
        /// The actual type
        actual: ParamType,
    },
}

struct ParamTypecheckVisitor {}

impl ParamTypecheckVisitor {
    fn assert_param_type<T>(
        value: &Parameterized<T>,
        expected: Vec<ParamType>,
    ) -> Result<(), TypeError> {
        if let Parameterized::Param { span, param } = value
            && !expected.contains(&param.typ)
        {
            return Err(TypeError::TypeMismatch {
                use_span: *span,
                declaration_span: param.span,
                param_name: param.name.clone(),
                expected,
                actual: param.typ,
            });
        }

        Ok(())
    }
}

impl QueryVisitor for ParamTypecheckVisitor {
    type Error = TypeError;

    fn visit_dataset(
        &mut self,
        dataset: &mut Parameterized<Dataset>,
    ) -> Result<VisitRes, Self::Error> {
        Self::assert_param_type(dataset, vec![ParamType::Dataset]).map(|()| VisitRes::Walk)
    }

    fn visit_align(&mut self, align: &mut query::Align) -> Result<VisitRes, Self::Error> {
        Self::assert_param_type(&align.time, vec![ParamType::Duration]).map(|()| VisitRes::Walk)
    }

    fn visit_bucket_by(
        &mut self,
        bucket_by: &mut query::BucketBy,
    ) -> Result<VisitRes, Self::Error> {
        Self::assert_param_type(&bucket_by.time, vec![ParamType::Duration]).map(|()| VisitRes::Walk)
    }

    fn visit_cmp(&mut self, _field: &mut String, cmp: &mut Cmp) -> Result<VisitRes, Self::Error> {
        let tag_value_param_types = vec![
            ParamType::Tag(TagType::String),
            ParamType::Tag(TagType::Int),
            ParamType::Tag(TagType::Float),
            ParamType::Tag(TagType::Bool),
        ];

        match cmp {
            Cmp::Is(_) => Ok(VisitRes::Walk),
            Cmp::Eq(value) => {
                if let Parameterized::Param { span, param } = value
                    && param.typ == ParamType::Regex
                {
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

                Self::assert_param_type(value, tag_value_param_types).map(|()| VisitRes::Walk)
            }
            Cmp::Ne(value) => {
                if let Parameterized::Param { span, param } = value
                    && param.typ == ParamType::Regex
                {
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

                Self::assert_param_type(value, tag_value_param_types).map(|()| VisitRes::Walk)
            }
            Cmp::Gt(value) | Cmp::Ge(value) | Cmp::Lt(value) | Cmp::Le(value) => {
                Self::assert_param_type(value, tag_value_param_types).map(|()| VisitRes::Walk)
            }
            Cmp::RegEx(value) | Cmp::RegExNot(value) => {
                Self::assert_param_type(value, vec![ParamType::Regex]).map(|()| VisitRes::Walk)
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
    pub const MPL: [(&str, &str); 17] = [
        example!("align-rate"),
        example!("as"),
        example!("enrich"),
        example!("filtered-histogram"),
        example!("histogram_rate"),
        example!("histogram"),
        example!("map-gt"),
        example!("map-mul"),
        example!("nested-enrich"),
        example!("parser-error"),
        example!("rate"),
        example!("replace_labels"),
        example!("set"),
        example!("slo-histogram"),
        example!("slo-ingest-rate"),
        example!("slo"),
        example!("sum_rate"),
    ];
}
