//! Error types and diagnostics for `MPL` parsing.
use std::fmt::{self};

use miette::{Diagnostic, SourceSpan};

use crate::query::ParamDeclaration;

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
        help("Supported operators: ==, !=, >, >=, <, <=, in")
    )]
    UnsupportedTagComparison {
        /// The source location of the unsupported operator
        #[label("invalid operator")]
        span: SourceSpan,
        /// The unsupported operator
        op: String,
    },

    /// `in` used with a non-array right-hand side
    #[error("`in` requires an array on the right-hand side")]
    #[diagnostic(
        code(mpl_lang::in_requires_array),
        help("Use an array literal (e.g. `in [200, 201]`) or an array-typed param")
    )]
    InRequiresArray {
        /// The source location of the offending right-hand side
        #[label("expected an array here")]
        span: SourceSpan,
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

    // commented out until this becomes an error, for now it's a warning
    // /// Param is using the prefix reserved for system params
    // #[error("The param ${param} is using a prefix reserved for system params")]
    // #[diagnostic(
    //     code(mpl_lang::param_reserved_prefix),
    //     help("The prefix `__` is reserved for system parameters")
    // )]
    // ParamUsingSystemPrefix {
    //     /// The source location of the param
    //     #[label("invalid prefix")]
    //     span: SourceSpan,
    //     /// The param
    //     param: String,
    // },
    /// The system param is not using the prefix
    #[error("The system param ${param} is missing the system prefix")]
    #[diagnostic(
        code(mpl_lan::system_param_missing_prefix),
        help("The system param is missing the `__` prefix")
    )]
    SystemParamMissingPrefix {
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
    /// `ifdef()` was used on a parameter that wasn't declared optional
    #[error("The parameter {} is not declared as optional", param.name)]
    #[diagnostic(code(mpl_lang::ifdef_not_optional))]
    IfdefNotOptional {
        /// The source location of the param declaration
        #[label("param declaration")]
        span: miette::SourceSpan,
        /// The param type
        param: ParamDeclaration,
    },
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
