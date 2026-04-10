//! The query structures
use std::{
    collections::{HashMap, HashSet},
    num::TryFromIntError,
};

#[cfg(feature = "clock")]
use chrono::Utc;
use chrono::{DateTime, Duration, FixedOffset};
use miette::SourceSpan;
use pest::Parser as _;
use strumbra::SharedString;

use crate::{
    ParseError,
    enc_regex::EncodableRegex,
    linker::{AlignFunction, ComputeFunction, GroupFunction, MapFunction},
    parser::{self, MPLParser, ParseParamError, Rule},
    tags::TagValue,
    time::{Resolution, ResolutionError},
    types::{BucketSpec, BucketType, Dataset, Metric, Parameterized},
};

mod fmt;
#[cfg(test)]
mod tests;

/// Metric identifier
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct MetricId {
    /// The dataset identifier or param
    pub dataset: Parameterized<Dataset>,
    /// The metric identifier
    pub metric: Metric,
}

/// Time unit
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum TimeUnit {
    /// Millisecond
    Millisecond,
    /// Second
    Second,
    /// Minute
    Minute,
    /// Hour
    Hour,
    /// Day
    Day,
    /// Week
    Week,
    /// Month
    Month,
    /// Year
    Year,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Relative time (1h)
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct RelativeTime {
    /// Value
    pub value: u64,
    /// Unit
    pub unit: TimeUnit,
}

/// A point in time
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum Time {
    /// A time relative to now
    Relative(RelativeTime),
    /// A timestamp
    Timestamp(i64),
    /// A RFC3339 timestamp
    RFC3339(#[cfg_attr(feature = "wasm", tsify(type = "string"))] DateTime<FixedOffset>),
    /// A time modifier
    Modifier(String),
}

/// A timerange between two times
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct TimeRange {
    /// Start time of the range
    pub start: Time,
    /// End time of the range or None for 'now'
    pub end: Option<Time>,
}

/// The source for a query
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Source {
    /// The metric
    pub metric_id: MetricId,
    /// The time range
    pub time: Option<TimeRange>,
}

///An error relkated to value parsing
#[derive(Debug, thiserror::Error)]
pub enum ValueError {
    /// Invalid float value
    #[error("Invalid Float")]
    BadFloat,
}

/// A comparison operator for filtering based on a value
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum Cmp {
    /// Equal to the given value
    Eq(Parameterized<TagValue>),
    /// Not equal to the given value
    Ne(Parameterized<TagValue>),
    /// Greater than the given value
    Gt(Parameterized<TagValue>),
    /// Greater than or equal to the given value
    Ge(Parameterized<TagValue>),
    /// Less than the given value
    Lt(Parameterized<TagValue>),
    /// Less than or equal to the given value
    Le(Parameterized<TagValue>),
    /// Matches the given regular expression
    RegEx(Parameterized<EncodableRegex>),
    /// Does not match the given regular expression
    RegExNot(Parameterized<EncodableRegex>),
    /// Is the given tag type
    Is(TagType),
}

/// Rename the output as a new metric
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct As {
    /// The new name for the metric
    pub name: Metric,
}

/// Filter the series
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum Filter {
    /// Logical AND of the given filters
    And(Vec<Filter>),
    /// Logical OR of the given filters
    Or(Vec<Filter>),
    /// Logical NOT of the given filters
    Not(Box<Filter>),
    /// Filter based on a filed
    Cmp {
        /// The field to filter on
        field: String,
        /// The compaison to perform
        rhs: Cmp,
    },
}

/// A Mapping function
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Mapping {
    /// The function to apply
    pub function: MapFunction,
    /// The optional argument to pass to the function
    pub arg: Option<f64>,
}

/// An Alignment function
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Align {
    /// The function to apply
    pub function: AlignFunction,
    /// The time to align to
    pub time: Parameterized<RelativeTime>,
}

/// A Grouping function
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct GroupBy {
    /// The location of the group by clause
    #[cfg_attr(feature = "wasm", tsify(type = "{ offset: number, length: number }"))]
    pub span: SourceSpan,
    /// The function to apply
    pub function: GroupFunction,
    /// The tags to group by
    pub tags: Vec<String>,
}

/// A Bucketing function, applying both tag and time based aggregation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct BucketBy {
    /// The location of the group by clause
    #[cfg_attr(feature = "wasm", tsify(type = "{ offset: number, length: number }"))]
    pub span: SourceSpan,
    /// The function to apply
    pub function: BucketType,
    /// The time to align to
    pub time: Parameterized<RelativeTime>,
    /// The tags to group by
    pub tags: Vec<String>,
    /// The buckets to produce
    pub spec: Vec<BucketSpec>,
}

/// Possible aggregate functions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum Aggregate {
    /// Map a function over each value
    Map(Mapping),
    /// Align the data to a time interval
    Align(Align),
    /// Group the data by tags
    GroupBy(GroupBy),
    /// Bucket the data by time and tags
    Bucket(BucketBy),
    /// Rename the metric
    As(As),
}

/// Values for directives
#[cfg_attr(feature = "wasm", tsify::declare)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum DirectiveValue {
    /// Directive with a ident value
    Ident(String),
    /// Directive with a literal value
    Int(i64),
    /// Directive with a float value
    Float(f64),
    /// Directive with a string value
    String(String),
    /// Directive with a boolean value
    Bool(bool),
    /// Directive with no value
    None,
}

impl DirectiveValue {
    /// Ident value
    #[must_use]
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            DirectiveValue::Ident(ident) => Some(ident),
            _ => None,
        }
    }
    /// Int value
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            DirectiveValue::Int(int) => Some(*int),
            _ => None,
        }
    }
    /// Float value
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            DirectiveValue::Float(float) => Some(*float),
            _ => None,
        }
    }
    /// String value
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            DirectiveValue::String(string) => Some(string),
            _ => None,
        }
    }
    /// Bool value
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DirectiveValue::Bool(bool) => Some(*bool),
            _ => None,
        }
    }
    /// Tests if value is None
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, DirectiveValue::None)
    }
    /// Tests if value is Some
    #[must_use]
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

/// Types for params.
#[cfg_attr(feature = "wasm", tsify::declare)]
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub enum ParamType {
    /// Duration (e.g. 25s)
    Duration,
    /// Dataset
    Dataset,
    /// Regex
    Regex,
    /// A tag value type
    Tag(TagType),
}
impl std::fmt::Display for ParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamType::Dataset => write!(f, "dataset"),
            ParamType::Duration => write!(f, "duration"),
            ParamType::Regex => write!(f, "regex"),
            ParamType::Tag(t) => t.fmt(f),
        }
    }
}

/// Types for params.
#[cfg_attr(feature = "wasm", tsify::declare)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
#[derive(Clone, Copy, Debug, Hash, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub enum TagType {
    /// String
    String,
    /// Int
    Int,
    /// Float
    Float,
    /// Bool
    Bool,
    /// None / Null value
    None,
}

impl std::fmt::Display for TagType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TagType::String => "string",
                TagType::Int => "int",
                TagType::Float => "float",
                TagType::Bool => "bool",
                TagType::None => "null",
            }
        )
    }
}

/// Directives given to adjust the behavior of the runtime
#[cfg_attr(feature = "wasm", tsify::declare)]
pub type Directives = HashMap<String, DirectiveValue>;

/// A param.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct Param {
    /// The location of the param
    #[cfg_attr(feature = "wasm", tsify(type = "{ offset: number, length: number }"))]
    pub span: SourceSpan,
    /// The name of the param
    pub name: String,
    /// The type of the param
    pub typ: ParamType,
}

/// A param value.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    /// Dataset
    Dataset(Dataset),
    /// Duration
    Duration(RelativeTime),
    /// String
    String(String),
    /// Int
    Int(i64),
    /// Float
    Float(f64),
    /// Bool
    Bool(bool),
    /// Regex
    Regex(EncodableRegex),
}

impl ParamValue {
    /// Get the type of the param value.
    #[must_use]
    pub fn typ(&self) -> ParamType {
        match self {
            ParamValue::Dataset(_) => ParamType::Dataset,
            ParamValue::Duration(_) => ParamType::Duration,
            ParamValue::Regex(_) => ParamType::Regex,
            ParamValue::String(_) => ParamType::Tag(TagType::String),
            ParamValue::Int(_) => ParamType::Tag(TagType::Int),
            ParamValue::Float(_) => ParamType::Tag(TagType::Float),
            ParamValue::Bool(_) => ParamType::Tag(TagType::Bool),
        }
    }
}

/// The param provided to the query.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvidedParam {
    /// The name of the param.
    pub name: String,
    /// The value.
    pub value: ParamValue,
}

impl ProvidedParam {
    /// Create a new `ProvidedParam`.
    pub fn new(name: impl Into<String>, value: ParamValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

/// A smol wrapper around `Vec<ProvidedParam>` for easier use.
#[derive(Debug, Clone, Default)]
pub struct ProvidedParams {
    inner: Vec<ProvidedParam>,
}

/// The error returned from `ProvidedParams::resolve`.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// Param not provided
    #[error("Param ${0} was not provided to the query")]
    ParamNotProvided(String),
    /// Invalid type
    #[error(
        "Param ${name} is defined as `{defined}`, but was used in a context that expected one of: {}",
        expected.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    )]
    InvalidType {
        /// Name of the param
        name: String,
        /// Type of the param
        defined: ParamType,
        /// The type that is valid in the context it was used
        expected: Vec<ParamType>,
    },
    /// Shared string error
    #[error("Shared string error: {0}")]
    SharedString(#[from] strumbra::Error),
}

/// The error returned from `ProvidedParams::parse`.
#[derive(Debug, thiserror::Error)]
pub enum ParseProvidedParamsError {
    /// Parse failed
    #[error("Failed to parse the value for ${param_name} as {expected_type}: {err}")]
    ParseParam {
        /// Param name
        param_name: String,
        /// Expected t ype
        expected_type: ParamType,
        /// Parse param error
        err: ParseParamError,
    },
    /// Params provided more than once
    #[error("These params were provided more than once: {}", .0.join(", "))]
    ParamsProvidedMoreThanOnce(Vec<String>),
    /// Params declared but not provided
    #[error("The following params were declared but not provided: {}", .0.join(", "))]
    ParamsDeclaredButNotProvided(Vec<String>),
    /// Too many params provided
    #[error("The number of params provided exceeds the upper limit of {0}")]
    TooManyParamsProvided(usize),
}

/// Warnings we want to surface to the user instead of failing the request.
#[derive(Debug, Default)]
pub struct Warnings {
    inner: Vec<String>,
}

impl Warnings {
    /// Create a new warnings structure.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new warning.
    pub fn push(&mut self, warning: impl Into<String>) {
        self.inner.push(warning.into());
    }

    /// Returns true if there are no warnings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the warnings as slice.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.inner
    }

    /// Turn into a vector.
    #[must_use]
    pub fn into_vec(self) -> Vec<String> {
        self.inner
    }
}

impl ProvidedParams {
    /// Create a new `ProvidedParams` struct.
    #[must_use]
    pub fn new(inner: Vec<ProvidedParam>) -> Self {
        Self { inner }
    }

    /// Parse params from a hashmap of query parameters.
    /// This will only look at params that start with `param__` and it'll use
    /// the parser definitions to extract the values.
    pub fn parse_and_validate(
        mpl_params: &Params,
        query_params: &[(String, String)],
    ) -> Result<(Self, Warnings), ParseProvidedParamsError> {
        const PREFIX: &str = "param__";
        const PARAM_COUNT_LIMIT: usize = 128;

        let mut warnings = Warnings::new();
        let mut defined_more_than_once = HashSet::new();
        let mut provided_but_not_declared = HashSet::new();
        let mut seen = HashSet::new();

        let params = query_params
            .iter()
            .filter_map(|(name, value)| {
                if !name.starts_with(PREFIX) {
                    return None;
                }
                let name = name.trim_start_matches(PREFIX);
                if name.is_empty() {
                    return None;
                }

                Some((name, value))
            })
            .take(PARAM_COUNT_LIMIT + 1)
            .collect::<Vec<(&str, &String)>>();

        // we don't support unlimited params
        if params.len() > PARAM_COUNT_LIMIT {
            return Err(ParseProvidedParamsError::TooManyParamsProvided(
                PARAM_COUNT_LIMIT,
            ));
        }

        let mut provided_params = Vec::new();
        for (name, value) in params {
            if seen.contains(name) {
                // uh oh, we've already seen this value
                defined_more_than_once.insert(name);
                continue;
            }
            seen.insert(name);

            // is the param even declared?
            let Some(mpl_param) = mpl_params.iter().find(|p| p.name == name) else {
                provided_but_not_declared.insert(name);
                continue;
            };

            // parse mpl
            let parsed = MPLParser::parse(Rule::param_value, value).map_err(|err| {
                ParseProvidedParamsError::ParseParam {
                    param_name: name.to_string(),
                    expected_type: mpl_param.typ,
                    err: ParseParamError::Parse(ParseError::from(err)),
                }
            })?;

            // parse as correct type
            let value = parser::parse_param_value(mpl_param, parsed).map_err(|err| {
                ParseProvidedParamsError::ParseParam {
                    param_name: name.to_string(),
                    expected_type: mpl_param.typ,
                    err,
                }
            })?;

            provided_params.push(ProvidedParam {
                name: name.to_string(),
                value,
            });
        }

        if !provided_but_not_declared.is_empty() {
            // sort for consistency
            let mut items = provided_but_not_declared
                .into_iter()
                .map(|p| format!("${p}"))
                .collect::<Vec<String>>();
            items.sort();

            // add to warnings, no need to error
            warnings.push(format!(
                "These params were provided but not declared: {}",
                items.join(", ")
            ));
        }

        if !defined_more_than_once.is_empty() {
            // sort for consistency
            let mut items = defined_more_than_once
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>();
            items.sort();

            return Err(ParseProvidedParamsError::ParamsProvidedMoreThanOnce(items));
        }

        let declared_param_names = mpl_params
            .iter()
            .map(|p| p.name.as_str())
            .collect::<HashSet<&str>>();
        let declared_but_not_provided = declared_param_names
            .difference(&seen)
            .collect::<Vec<&&str>>();
        if !declared_but_not_provided.is_empty() {
            // sort for consistency
            let mut items = declared_but_not_provided
                .into_iter()
                .map(|s| String::from(*s))
                .collect::<Vec<String>>();
            items.sort();

            return Err(ParseProvidedParamsError::ParamsDeclaredButNotProvided(
                items,
            ));
        }

        Ok((ProvidedParams::new(provided_params), warnings))
    }

    /// Return a ref to the inner value.
    #[must_use]
    pub fn as_slice(&self) -> &[ProvidedParam] {
        self.inner.as_slice()
    }

    fn get_param(&self, name: &str) -> Result<&ProvidedParam, ResolveError> {
        self.inner
            .iter()
            .find(|p| p.name == name)
            .ok_or(ResolveError::ParamNotProvided(name.to_string()))
    }

    /// Resolve a `TagValue`.
    pub fn resolve_tag_value(&self, pv: Parameterized<TagValue>) -> Result<TagValue, ResolveError> {
        let param = match pv {
            Parameterized::Concrete(val) => return Ok(val), // no need to resolve
            Parameterized::Param { span: _, param } => param,
        };

        let provided_param = self.get_param(&param.name)?;
        match &provided_param.value {
            ParamValue::String(val) => Ok(TagValue::String(SharedString::try_from(val)?)),
            ParamValue::Int(val) => Ok(TagValue::Int(*val)),
            ParamValue::Float(val) => Ok(TagValue::Float(*val)),
            ParamValue::Bool(val) => Ok(TagValue::Bool(*val)),
            val => Err(ResolveError::InvalidType {
                name: param.name,
                defined: val.typ(),
                expected: vec![
                    ParamType::Tag(TagType::String),
                    ParamType::Tag(TagType::Int),
                    ParamType::Tag(TagType::Float),
                    ParamType::Tag(TagType::Bool),
                ],
            }),
        }
    }

    /// Resolve a `Dataset`.
    pub fn resolve_dataset(&self, pv: Parameterized<Dataset>) -> Result<Dataset, ResolveError> {
        let param = match pv {
            Parameterized::Concrete(val) => return Ok(val), // no need to resolve
            Parameterized::Param { span: _, param } => param,
        };

        let provided_param = self.get_param(&param.name)?;
        match &provided_param.value {
            ParamValue::Dataset(dataset) => Ok(dataset.clone()),
            val => Err(ResolveError::InvalidType {
                name: param.name,
                defined: val.typ(),
                expected: vec![ParamType::Dataset],
            }),
        }
    }

    /// Resolve a `RelativeTime`, aka duration.
    pub fn resolve_relative_time(
        &self,
        pv: Parameterized<RelativeTime>,
    ) -> Result<RelativeTime, ResolveError> {
        let param = match pv {
            Parameterized::Concrete(val) => return Ok(val), // no need to resolve
            Parameterized::Param { span: _, param } => param,
        };

        let provided_param = self.get_param(&param.name)?;
        match &provided_param.value {
            ParamValue::Duration(relative_time) => Ok(relative_time.clone()),
            val => Err(ResolveError::InvalidType {
                name: param.name,
                defined: val.typ(),
                expected: vec![ParamType::Duration],
            }),
        }
    }

    /// Resolve a regex.
    pub fn resolve_regex(
        &self,
        pv: Parameterized<EncodableRegex>,
    ) -> Result<EncodableRegex, ResolveError> {
        let param = match pv {
            Parameterized::Concrete(val) => return Ok(val), // no need to resolve
            Parameterized::Param { span: _, param } => param,
        };

        let provided_param = self.get_param(&param.name)?;
        match &provided_param.value {
            ParamValue::Regex(re) => Ok(re.clone()),
            val => Err(ResolveError::InvalidType {
                name: param.name,
                defined: val.typ(),
                expected: vec![ParamType::Regex],
            }),
        }
    }
}

/// Parameters that will be set externally.
#[cfg_attr(feature = "wasm", tsify::declare)]
pub type Params = Vec<Param>;

/// A Query AST representing a query in the `MPL` language
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum Query {
    /// A simple query that will produce a result
    Simple {
        /// The source of the data
        source: Source,
        /// The filters to apply to the data
        filters: Vec<Filter>,
        /// The aggregates to apply to the data
        aggregates: Vec<Aggregate>,
        /// The directives
        directives: Directives,
        /// The params
        params: Params,
        /// How to sample series
        sample: Option<f64>,
    },
    /// A compute query taking the input of two queries and producing a by computing combined values
    Compute {
        /// The left hand side query to compute
        left: Box<Query>,
        /// The right hand side query to compute
        right: Box<Query>,
        /// The name of the metric to produce
        name: Metric,
        /// The compute operation used to combine the left and right queries
        op: ComputeFunction,
        /// The aggregates to apply to the combined data
        aggregates: Vec<Aggregate>,
        /// The directives
        directives: Directives,
        /// The params
        params: Params,
    },
}

impl Query {
    /// Get a ref to the params of the query.
    #[must_use]
    pub fn params(&self) -> &Params {
        match self {
            Query::Simple { params, .. } | Query::Compute { params, .. } => params,
        }
    }
    /// Get a ref to the directives of the query.
    #[must_use]
    pub fn directives(&self) -> &Directives {
        match self {
            Query::Simple { directives, .. } | Query::Compute { directives, .. } => directives,
        }
    }
}

impl RelativeTime {
    /// Converts a relative time to a `Duration`
    pub fn to_duration(&self) -> Result<Duration, TimeError> {
        let v = i64::try_from(self.value).map_err(TimeError::InvalidDuration)?;
        Ok(match self.unit {
            TimeUnit::Millisecond => Duration::milliseconds(v),
            TimeUnit::Second => Duration::seconds(v),
            TimeUnit::Minute => Duration::minutes(v),
            TimeUnit::Hour => Duration::hours(v),
            TimeUnit::Day => Duration::days(v),
            TimeUnit::Week => Duration::weeks(v),
            TimeUnit::Month => Duration::days(v.saturating_mul(30)),
            TimeUnit::Year => Duration::days(v.saturating_mul(365)),
        })
    }

    /// Converts a relative time to a `Resolution`
    pub fn to_resolution(&self) -> Result<Resolution, ResolutionError> {
        match self.unit {
            TimeUnit::Millisecond => Resolution::secs(self.value / 1000),
            TimeUnit::Second => Resolution::secs(self.value),
            TimeUnit::Minute => Resolution::secs(self.value.saturating_mul(60)),
            TimeUnit::Hour => Resolution::secs(self.value.saturating_mul(60 * 60)),
            TimeUnit::Day => Resolution::secs(self.value.saturating_mul(60 * 60 * 24)),
            TimeUnit::Week => Resolution::secs(self.value.saturating_mul(60 * 60 * 24 * 7)),
            TimeUnit::Month => Resolution::secs(self.value.saturating_mul(60 * 60 * 24 * 30)),
            TimeUnit::Year => Resolution::secs(self.value.saturating_mul(60 * 60 * 24 * 365)),
        }
    }
}

/// An error that can occur when converting a time value.
#[derive(Debug, thiserror::Error)]
pub enum TimeError {
    /// Invalid timestamp could not be converted to a UTC datetime
    #[error("Invalid timestamp {0}, could not be converted to a UTC datetime")]
    InvalidTimestamp(i64),
    /// Invalid duration could not be converted to Duration as it exceeds the maximum i64
    #[error(
        "Invalid duration {0}, could not be converted to Duration as it exceeds the maximum i64"
    )]
    InvalidDuration(TryFromIntError),
}
#[cfg(feature = "clock")]
impl Time {
    fn to_datetime(&self) -> Result<DateTime<Utc>, TimeError> {
        Ok(match self {
            Time::Relative(t) => Utc::now() - t.to_duration()?,
            Time::Timestamp(ts) => {
                DateTime::<Utc>::from_timestamp(*ts, 0).ok_or(TimeError::InvalidTimestamp(*ts))?
            }
            Time::RFC3339(t) => t.with_timezone(&Utc),
            Time::Modifier(_) => todo!(),
        })
    }
}

#[cfg(feature = "clock")]
impl TimeRange {
    /// Converts a time range to a start and pair
    pub fn to_start_end(&self) -> Result<(DateTime<Utc>, DateTime<Utc>), TimeError> {
        let start = self.start.to_datetime()?;
        let end = self
            .end
            .as_ref()
            .map_or_else(|| Ok(Utc::now()), Time::to_datetime)?;
        Ok((start, end))
    }
}
