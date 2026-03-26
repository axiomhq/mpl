//! Generic datatypes for `MPL`
use std::ops::Deref;

use miette::SourceSpan;
use serde::{Deserialize, Serialize};
pub use strumbra::Error as StrumbraError;
use strumbra::SharedString;

use crate::{
    linker::{Arg, ArgType, FunctionTrait},
    query::Param,
};

/// A dataset identifier
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    // needed for the axum path extractor
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "bincode", derive(bincode::Decode, bincode::Encode))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Dataset(String);

impl From<&str> for Dataset {
    fn from(value: &str) -> Self {
        Dataset(value.to_string())
    }
}
impl PartialEq<str> for Dataset {
    fn eq(&self, other: &str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<&str> for Dataset {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}

impl Dataset {
    /// Creates a new dataset.
    #[must_use]
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl Deref for Dataset {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for Dataset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A type that can either be concrete or a param.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum Parameterized<T> {
    /// A concrete type
    Concrete(T),
    /// A parameter and the location where it's used (for type checking)
    Param {
        /// The location where the param is used
        #[cfg_attr(feature = "wasm", tsify(type = "{ offset: number, length: number }"))]
        span: SourceSpan,
        /// The param
        param: Param,
    },
}

impl<T> std::fmt::Display for Parameterized<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Parameterized::Concrete(inner) => write!(f, "{inner}"),
            Parameterized::Param { span: _, param } => write!(f, "${}", param.name),
        }
    }
}

impl<T> Parameterized<T> {
    /// Map the concrete value.
    pub fn map_concrete<O, F: Fn(T) -> O>(self, f: F) -> Parameterized<O> {
        match self {
            Parameterized::Concrete(inner) => Parameterized::Concrete(f(inner)),
            Parameterized::Param { span, param } => Parameterized::Param { span, param },
        }
    }

    /// Try to map the concrete value.
    pub fn try_map_concrete<O, E, F: Fn(T) -> Result<O, E>>(
        self,
        f: F,
    ) -> Result<Parameterized<O>, E> {
        Ok(match self {
            Parameterized::Concrete(inner) => Parameterized::Concrete(f(inner)?),
            Parameterized::Param { span, param } => Parameterized::Param { span, param },
        })
    }

    /// Returns true if the type is a param.
    pub fn is_param(&self) -> bool {
        matches!(self, Parameterized::Param { .. })
    }

    /// Returns true if the type is a concrete value.
    pub fn is_concrete(&self) -> bool {
        matches!(self, Parameterized::Concrete(_))
    }
}

/// A metric identifier
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Metric(
    #[cfg_attr(feature = "wasm", tsify(type = "String"))]
    #[cfg_attr(feature = "bincode", bincode(with_serde))]
    SharedString,
);

impl std::fmt::Display for Metric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq<str> for Metric {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}

impl Deref for Metric {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Metric {
    /// Creates a new metric.
    pub fn new(name: &str) -> Result<Self, StrumbraError> {
        Ok(Self(SharedString::try_from(name)?))
    }
}

impl TryFrom<String> for Metric {
    type Error = StrumbraError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Ok(Metric(SharedString::try_from(s)?))
    }
}

impl TryFrom<&'_ str> for Metric {
    type Error = StrumbraError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Ok(Metric(SharedString::try_from(s)?))
    }
}

impl PartialEq<&str> for Metric {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}

/// The method for cumulative-to-delta conversion
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum ConversionMethod {
    /// Use rate to convert cumulative to delta
    #[default]
    Rate,
    /// Use increase to convert cumulative to delta
    Increase,
}

impl std::fmt::Display for ConversionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversionMethod::Rate => write!(f, "rate"),
            ConversionMethod::Increase => write!(f, "increase"),
        }
    }
}

/// Aggregation type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum BucketType {
    /// Histogram
    Histogram,
    /// Interpolate delta histogram based on histogram data in the store
    InterpolateDeltaHistogram,
    /// Interpolate cumulative histogram with a specified conversion mode
    InterpolateCumulativeHistogram(ConversionMethod),
}

impl FunctionTrait for BucketType {
    fn doc(&self) -> &str {
        match self {
            BucketType::Histogram => {
                "Aggregates non-histogram input series using the provided bucket specs (including histogram quantile calculation)."
            }
            BucketType::InterpolateDeltaHistogram => {
                "Aggregates delta-temporality histogram input series using the provided bucket specs (including histogram quantile calculation via interpolation)."
            }
            BucketType::InterpolateCumulativeHistogram(_) => {
                "Aggregates cumulative-temporality histogram input series after converting to delta using a conversion mode (including histogram quantile calculation via interpolation)."
            }
        }
    }

    fn args(&self) -> Vec<Arg> {
        match self {
            BucketType::InterpolateCumulativeHistogram(_) => vec![
                Arg::new("mode", ArgType::Enum(&["rate", "increase"])),
                Arg::new(
                    "specs",
                    ArgType::Repeated {
                        typ: Box::new(ArgType::OneOf(vec![
                            // cumulative does not support min/max as bucket spec
                            ArgType::Enum(&["count", "avg", "sum"]),
                            ArgType::Float,
                        ])),
                        min: 1,
                        max: None,
                    },
                ),
            ],
            BucketType::Histogram | BucketType::InterpolateDeltaHistogram => vec![Arg::new(
                "specs",
                ArgType::Repeated {
                    typ: Box::new(ArgType::OneOf(vec![
                        ArgType::Enum(&["count", "avg", "sum", "min", "max"]),
                        ArgType::Float,
                    ])),
                    min: 1,
                    max: None,
                },
            )],
        }
    }
}

impl std::fmt::Display for BucketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BucketType::Histogram => write!(f, "histogram"),
            BucketType::InterpolateDeltaHistogram => write!(f, "interpolate_delta_histogram"),
            BucketType::InterpolateCumulativeHistogram(_) => {
                write!(f, "interpolate_cumulative_histogram")
            }
        }
    }
}

/// Aggregation type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum MapType {
    /// Min
    Min,
    /// Max
    Max,
    /// Rate
    Rate,
    /// Add
    Add,
    /// Sub
    Sub,
    /// Mul
    Mul,
    /// Div
    Div,
    /// Abs
    Abs,
    /// fill with constant
    FillConst,
    /// fill with previous value
    FillPrev,
    /// Increase between points
    Increase,
    /// lesser than
    FilterLt,
    /// greater than
    FilterGt,
    /// equal to
    FilterEq,
    /// not equal to
    FilterNe,
    /// greater than or equal to
    FilterGe,
    /// less than or equal to
    FilterLe,
    /// lesser than
    IsLt,
    /// greater than
    IsGt,
    /// equal to
    IsEq,
    /// not equal to
    IsNe,
    /// greater than or equal to
    IsGe,
    /// less than or equal to
    IsLe,
    /// Linear interpolation
    InterpolateLinear,
}

impl FunctionTrait for MapType {
    fn doc(&self) -> &str {
        match self {
            MapType::Min => "Minimum between the argument and the datapoint",
            MapType::Max => "Maximum between the argument and the datapoint",
            MapType::Add => "Adds the argument to the datapoint",
            MapType::Sub => "Subtracts the argument from the datapoint",
            MapType::Mul => "Multiplies the argument with the datapoint",
            MapType::Div => "Divides the datapoint by the argument",
            MapType::Abs => "Absolute value of the datapoint",
            MapType::Rate => {
                "Per second rate of change between the datapoint and the previous datapoint"
            }
            MapType::FillConst => "Fills unset datapoints with the given constant",
            MapType::FillPrev => "Fills unset datapoints with the previous datapoint",
            MapType::Increase => {
                "Calculates the increase between the datapoint and the previous datapoint"
            }
            MapType::FilterLt => {
                "Filters for datapoints that are less than the argument all datapoints not less than the argument are removed"
            }
            MapType::FilterGt => {
                "Filters for datapoints that are greater than the argument all datapoints not greater than the argument are removed"
            }
            MapType::FilterEq => {
                "Filters for datapoints that are equal to the argument all datapoints not equal to the argument are removed"
            }
            MapType::FilterNe => {
                "Filters for datapoints that are not equal to the argument all datapoints equal to the argument are removed"
            }
            MapType::FilterGe => {
                "Filters for datapoints that are greater than or equal to the argument all datapoints not greater than or equal to the argument are removed"
            }
            MapType::FilterLe => {
                "Filters for datapoints that are less than or equal to the argument all datapoints not less than or equal to the argument are removed"
            }
            MapType::IsLt => {
                "Sets the datapoint to 1.0 if the datapoint is less than the argument otherwise sets it to 0.0"
            }
            MapType::IsGt => {
                "Sets the datapoint to 1.0 if the datapoint is greater than the argument otherwise sets it to 0.0"
            }
            MapType::IsEq => {
                "Sets the datapoint to 1.0 if the datapoint is equal to the argument otherwise sets it to 0.0"
            }
            MapType::IsNe => {
                "Sets the datapoint to 1.0 if the datapoint is not equal to the argument otherwise sets it to 0.0"
            }
            MapType::IsLe => {
                "Sets the datapoint to 1.0 if the datapoint is less than or equal to the argument otherwise sets it to 0.0"
            }
            MapType::IsGe => {
                "Sets the datapoint to 1.0 if the datapoint is greater than or equal to the argument otherwise sets it to 0.0"
            }
            MapType::InterpolateLinear => {
                "Performs linear interpolation between two datapoints filling unset values with the interpolated value"
            }
        }
    }
    fn args(&self) -> Vec<Arg> {
        match self {
            MapType::FilterLt
            | MapType::FilterGt
            | MapType::FilterEq
            | MapType::FilterNe
            | MapType::FilterGe
            | MapType::FilterLe
            | MapType::IsLt
            | MapType::IsGt
            | MapType::IsEq
            | MapType::IsNe
            | MapType::IsGe
            | MapType::IsLe
            | MapType::Add
            | MapType::Sub
            | MapType::Mul
            | MapType::Div
            | MapType::FillConst => vec![Arg::new("value", ArgType::Float)],
            MapType::Min => vec![Arg::new("min", ArgType::Float)],
            MapType::Max => vec![Arg::new("max", ArgType::Float)],
            MapType::Abs
            | MapType::Rate
            | MapType::FillPrev
            | MapType::Increase
            | MapType::InterpolateLinear => {
                vec![]
            }
        }
    }
}

impl std::fmt::Display for MapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapType::Min => write!(f, "min"),
            MapType::Max => write!(f, "max"),
            MapType::Rate => write!(f, "rate"),
            MapType::Add => write!(f, "+"),
            MapType::Sub => write!(f, "-"),
            MapType::Mul => write!(f, "*"),
            MapType::Div => write!(f, "/"),
            MapType::Abs => write!(f, "abs"),
            MapType::FillConst => write!(f, "fill::const"),
            MapType::FillPrev => write!(f, "fill::prev"),
            MapType::Increase => write!(f, "increase"),
            MapType::FilterLt => write!(f, "filter::lt"),
            MapType::FilterGt => write!(f, "filter::gt"),
            MapType::FilterEq => write!(f, "filter::eq"),
            MapType::FilterNe => write!(f, "filter::ne"),
            MapType::FilterGe => write!(f, "filter::ge"),
            MapType::FilterLe => write!(f, "filter::le"),
            MapType::IsLt => write!(f, "Is::lt"),
            MapType::IsGt => write!(f, "Is::gt"),
            MapType::IsEq => write!(f, "Is::eq"),
            MapType::IsNe => write!(f, "Is::ne"),
            MapType::IsGe => write!(f, "Is::ge"),
            MapType::IsLe => write!(f, "Is::le"),
            MapType::InterpolateLinear => write!(f, "linear"),
        }
    }
}

/// Aggregation type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum TimeType {
    /// Count
    Count,
    /// Sum
    Sum,
    /// Average
    Avg,
    /// Min
    Min,
    /// Max
    Max,
    /// average rate over time
    Rate,
    /// Last observed
    Last,
}
impl FunctionTrait for TimeType {
    fn doc(&self) -> &str {
        match self {
            TimeType::Count => "Count the number of elements",
            TimeType::Sum => "Sum the elements",
            TimeType::Avg => "Average the elements",
            TimeType::Min => "Minimum of the elements",
            TimeType::Max => "Maximum of the elements",
            TimeType::Rate => "Average per second rate over a time window",
            TimeType::Last => "Last observed value",
        }
    }

    fn args(&self) -> Vec<Arg> {
        vec![]
    }
}

impl std::fmt::Display for TimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeType::Count => write!(f, "count"),
            TimeType::Sum => write!(f, "sum"),
            TimeType::Avg => write!(f, "avg"),
            TimeType::Min => write!(f, "min"),
            TimeType::Max => write!(f, "max"),
            TimeType::Rate => write!(f, "rate"),
            TimeType::Last => write!(f, "last"),
        }
    }
}

/// Aggregation type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum TagsType {
    /// Count
    Count,
    /// Sum
    Sum,
    /// Average
    Avg,
    /// Min
    Min,
    /// Max
    Max,
}

impl FunctionTrait for TagsType {
    fn doc(&self) -> &str {
        match self {
            TagsType::Count => "Counts the number of set values",
            TagsType::Sum => "Sums the datapoints",
            TagsType::Avg => "Averages the datapoints",
            TagsType::Min => "The minimum value",
            TagsType::Max => "The maximum value",
        }
    }
    fn args(&self) -> Vec<Arg> {
        match self {
            TagsType::Count | TagsType::Sum | TagsType::Avg | TagsType::Min | TagsType::Max => {
                vec![]
            }
        }
    }
}

impl std::fmt::Display for TagsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagsType::Count => write!(f, "count"),
            TagsType::Sum => write!(f, "sum"),
            TagsType::Avg => write!(f, "avg"),
            TagsType::Min => write!(f, "min"),
            TagsType::Max => write!(f, "max"),
        }
    }
}

/// Aggregation type used in Compute
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum ComputeType {
    /// Average
    Avg,
    /// Min
    Min,
    /// Max
    Max,
    /// Add
    Add,
    /// Subtraction
    Sub,
    /// Multiplication
    Mul,
    /// Division
    Div,
}

impl FunctionTrait for ComputeType {
    fn doc(&self) -> &str {
        match self {
            ComputeType::Add => "Sums the datapoints",
            ComputeType::Avg => "Averages the datapoints",
            ComputeType::Min => "The minimum value",
            ComputeType::Max => "The maximum value",
            ComputeType::Div => "Divides the datapoints, removes datapoints if the divisor is zero",
            ComputeType::Mul => "Multiplies the datapoints",
            ComputeType::Sub => "Subtracts the datapoints",
        }
    }
    fn args(&self) -> Vec<Arg> {
        match self {
            ComputeType::Add
            | ComputeType::Avg
            | ComputeType::Min
            | ComputeType::Max
            | ComputeType::Div
            | ComputeType::Mul
            | ComputeType::Sub => vec![],
        }
    }
}

impl std::fmt::Display for ComputeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComputeType::Avg => write!(f, "avg"),
            ComputeType::Min => write!(f, "min"),
            ComputeType::Max => write!(f, "max"),
            ComputeType::Div => write!(f, "/"),
            ComputeType::Mul => write!(f, "*"),
            ComputeType::Add => write!(f, "+"),
            ComputeType::Sub => write!(f, "-"),
        }
    }
}

/// Bucket aggregation Histogram bucket sepcification
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum BucketSpec {
    /// Number of elements
    #[default]
    Count,
    /// Average of elemetns
    Avg,
    /// Sum of all elements
    Sum,
    /// Minimal value
    Min,
    /// Maximal value
    Max,
    /// Percentile
    Percentile(f64),
}

impl std::fmt::Display for BucketSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BucketSpec::Count => write!(f, "count"),
            BucketSpec::Avg => write!(f, "avg"),
            BucketSpec::Sum => write!(f, "sum"),
            BucketSpec::Min => write!(f, "min"),
            BucketSpec::Max => write!(f, "max"),
            BucketSpec::Percentile(p) => write!(f, "{p}"),
        }
    }
}
