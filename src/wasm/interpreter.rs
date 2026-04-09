//! Playground interpreter.
//!
//! Evaluates [`PipeStep`]s against in-memory datasets.

use std::collections::HashMap;

use derive_more::Deref;
use eyre::{Result, bail, ensure, eyre};
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::{
    linker::{AlignFunction, GroupFunction, MapFunction},
    query::{
        Aggregate, Align, As, BucketBy, Cmp, Filter, GroupBy, Mapping, RelativeTime, Source,
        TagType, TimeUnit,
    },
    tags::TagValue,
    types::{BucketSpec, MapType, Parameterized, TagsType, TimeType},
    wasm::steps::{PipeStep, StepNode, parse_steps},
};

/// Top-level list of datasets.
pub type Datasets = Vec<DatasetEntry>;

/// A named dataset containing metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetEntry {
    /// Dataset name.
    pub name: String,
    /// Metrics in this dataset.
    pub metrics: Vec<MetricEntry>,
}

/// A named metric containing series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEntry {
    /// Metric name.
    pub name: String,
    /// Series for this metric.
    pub series: Vec<Series>,
}

/// A time series with named tags, timestamps, and values.
#[derive(Debug, Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Series {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Tag key-value pairs.
    pub tags: HashMap<String, String>,
    /// Unix timestamps (seconds).
    pub timestamps: Vec<f64>,
    /// Data values (one per timestamp).
    pub values: Vec<f64>,
}

/// An interpreter error.
#[derive(Debug, Clone, Deref, Serialize, Tsify)]
#[serde(transparent)]
#[tsify(into_wasm_abi)]
pub struct StepError(pub String);

/// Result for a single pipeline step.
#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub enum StepResult {
    /// Successful step with series data.
    Ok(Vec<Series>),
    /// Failed step with error message.
    Err(StepError),
}

/// Combined parse + interpret result for a single step.
#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct StepOutput {
    /// Display text for this step.
    pub text: String,
    /// Interpretation result.
    pub result: StepResult,
}

/// Result of running a full pipeline.
#[derive(Debug, Clone, Deref, Serialize, Tsify)]
#[serde(transparent)]
#[tsify(into_wasm_abi)]
pub struct RunOutput(pub Vec<StepOutput>);

/// Playground interpreter with pre-loaded datasets.
#[wasm_bindgen]
pub struct Interpreter {
    datasets: Datasets,
}

#[wasm_bindgen]
impl Interpreter {
    /// Create an interpreter with the given datasets.
    #[wasm_bindgen(constructor)]
    pub fn new(datasets: JsValue) -> Result<Self, String> {
        Ok(Self {
            datasets: serde_wasm_bindgen::from_value(datasets).map_err(|e| e.to_string())?,
        })
    }

    /// Parse and interpret an MPL query.
    pub fn run(&self, code: &str) -> Result<RunOutput, String> {
        let steps = parse_steps(code)?;
        let results = interpret(&steps, &self.datasets);

        Ok(RunOutput(
            steps
                .iter()
                .zip(results)
                .map(|(step, result)| StepOutput {
                    text: step.canonical.clone(),
                    result: match result {
                        Ok(s) => StepResult::Ok(s),
                        Err(e) => StepResult::Err(StepError(format!("{e:#}"))),
                    },
                })
                .collect(),
        ))
    }
}

/// Interpret a sequence of pipeline steps against in-memory datasets.
pub fn interpret(pipe_steps: &[PipeStep], datasets: &Datasets) -> Vec<Result<Vec<Series>>> {
    let mut results = Vec::with_capacity(pipe_steps.len());
    let mut series: Vec<Series> = Vec::new();

    for step in pipe_steps {
        let outcome = match &step.node {
            StepNode::Source(src) => eval_source(src, datasets),
            StepNode::Filter(f) => apply_filter(&series, f),
            StepNode::Sample(rate) => apply_sample(&series, *rate),
            StepNode::Error(msg) => Err(eyre!("{msg}")),
            StepNode::Aggregate(agg) => apply_aggregate(&series, agg),
            StepNode::Compute { .. } => {
                Err(eyre!("Compute queries are not supported in the playground"))
            }
        };

        if let Ok(ref s) = outcome {
            series = s.clone();
        }
        results.push(outcome);
    }

    results
}

fn get_param<T>(p: &Parameterized<T>) -> Result<&T> {
    match p {
        Parameterized::Concrete(v) => Ok(v),
        Parameterized::Param { .. } => {
            bail!("Parameterized values are not supported in the playground")
        }
    }
}

fn raw_tag(v: &TagValue) -> String {
    match v {
        TagValue::None => String::new(),
        TagValue::Bool(b) => b.to_string(),
        TagValue::Int(i) => i.to_string(),
        TagValue::Float(f) => f.to_string(),
        TagValue::String(s) => s.to_string(),
    }
}

fn series_name(tags: &HashMap<String, String>) -> String {
    if tags.is_empty() {
        return "(all)".into();
    }
    let mut pairs: Vec<_> = tags.iter().collect();
    pairs.sort_by_key(|(k, _)| *k);
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn require_tag(tags: &HashMap<String, String>, key: &str) -> Result<String> {
    tags.get(key)
        .cloned()
        .ok_or_else(|| eyre!("Missing required tag: {key}"))
}

fn eval_source(src: &Source, datasets: &Datasets) -> Result<Vec<Series>> {
    let dataset = get_param(&src.metric_id.dataset)?.to_string();
    let metric = src.metric_id.metric.to_string();
    Ok(datasets
        .iter()
        .find(|ds| ds.name == dataset)
        .and_then(|ds| ds.metrics.iter().find(|m| m.name == metric))
        .map(|m| m.series.clone())
        .unwrap_or_default())
}

fn evaluate_cmp(tag_val: &str, rhs: &Cmp) -> Result<bool> {
    match rhs {
        Cmp::Eq(p) => Ok(tag_val == raw_tag(get_param(p)?)),
        Cmp::Ne(p) => Ok(tag_val != raw_tag(get_param(p)?)),
        Cmp::Gt(p) => {
            let lhs: f64 = tag_val.parse().unwrap_or(f64::NAN);
            let rhs_f: f64 = raw_tag(get_param(p)?).parse().unwrap_or(f64::NAN);
            Ok(lhs > rhs_f)
        }
        Cmp::Ge(p) => {
            let lhs: f64 = tag_val.parse().unwrap_or(f64::NAN);
            let rhs_f: f64 = raw_tag(get_param(p)?).parse().unwrap_or(f64::NAN);
            Ok(lhs >= rhs_f)
        }
        Cmp::Lt(p) => {
            let lhs: f64 = tag_val.parse().unwrap_or(f64::NAN);
            let rhs_f: f64 = raw_tag(get_param(p)?).parse().unwrap_or(f64::NAN);
            Ok(lhs < rhs_f)
        }
        Cmp::Le(p) => {
            let lhs: f64 = tag_val.parse().unwrap_or(f64::NAN);
            let rhs_f: f64 = raw_tag(get_param(p)?).parse().unwrap_or(f64::NAN);
            Ok(lhs <= rhs_f)
        }
        Cmp::RegEx(p) => {
            let re = get_param(p)?;
            Ok(re.is_match(tag_val))
        }
        Cmp::RegExNot(p) => {
            let re = get_param(p)?;
            Ok(!re.is_match(tag_val))
        }
        Cmp::Is(tag_type) => Ok(match tag_type {
            TagType::None => tag_val.is_empty(),
            TagType::Bool => tag_val == "true" || tag_val == "false",
            TagType::Int => tag_val.parse::<i64>().is_ok(),
            TagType::Float => tag_val.parse::<f64>().is_ok(),
            TagType::String => true,
        }),
    }
}

fn evaluate_filter(s: &Series, filter: &Filter) -> Result<bool> {
    match filter {
        Filter::Cmp { field, rhs } => match s.tags.get(field.as_str()) {
            Some(tag_val) => evaluate_cmp(tag_val, rhs),
            None => Ok(false),
        },
        Filter::And(filters) => {
            for f in filters {
                if !evaluate_filter(s, f)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Filter::Or(filters) => {
            for f in filters {
                if evaluate_filter(s, f)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Filter::Not(f) => Ok(!evaluate_filter(s, f)?),
    }
}

fn filter_fields(filter: &Filter) -> Vec<&str> {
    match filter {
        Filter::Cmp { field, .. } => vec![field.as_str()],
        Filter::And(fs) | Filter::Or(fs) => fs.iter().flat_map(filter_fields).collect(),
        Filter::Not(f) => filter_fields(f),
    }
}

fn apply_filter(series: &[Series], filter: &Filter) -> Result<Vec<Series>> {
    for field in filter_fields(filter) {
        ensure!(
            series.iter().any(|s| s.tags.contains_key(field)),
            "Unknown tag: {field}"
        );
    }
    let mut result = Vec::new();
    for s in series {
        if evaluate_filter(s, filter)? {
            result.push(s.clone());
        }
    }
    Ok(result)
}

fn apply_sample(series: &[Series], rate: f64) -> Result<Vec<Series>> {
    let mut rng = rand::rng();
    Ok(series
        .iter()
        .filter(|_| rng.random::<f64>() < rate)
        .cloned()
        .collect())
}

fn reduce_tags_pts(pts: &[f64], func: TagsType) -> f64 {
    match func {
        TagsType::Sum => pts.iter().sum(),
        TagsType::Avg => pts.iter().sum::<f64>() / pts.len() as f64,
        TagsType::Min => pts.iter().copied().fold(f64::INFINITY, f64::min),
        TagsType::Max => pts.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        TagsType::Count => pts.len() as f64,
    }
}

fn aggregate_columns(values: &[&[f64]], func: TagsType) -> Result<Vec<f64>> {
    ensure!(!values.is_empty(), "Cannot aggregate empty series");
    let len = values[0].len();
    let mut result = Vec::with_capacity(len);
    for i in 0..len {
        let pts: Vec<f64> = values
            .iter()
            .filter_map(|v| {
                let x = v[i];
                if x.is_nan() { None } else { Some(x) }
            })
            .collect();
        ensure!(
            !pts.is_empty(),
            "No valid points at column {i} for {func:?}"
        );
        result.push(reduce_tags_pts(&pts, func));
    }
    Ok(result)
}

fn apply_aggregate(series: &[Series], agg: &Aggregate) -> Result<Vec<Series>> {
    match agg {
        Aggregate::Map(m) => apply_map(series, m),
        Aggregate::Align(a) => apply_align(series, a),
        Aggregate::GroupBy(g) => apply_group(series, g),
        Aggregate::Bucket(b) => apply_bucket(series, b),
        Aggregate::As(a) => apply_as(series, a),
    }
}

fn require_arg(arg: Option<f64>, name: &str) -> Result<f64> {
    arg.ok_or_else(|| eyre!("Map function {name} requires an argument"))
}

fn apply_map(series: &[Series], mapping: &Mapping) -> Result<Vec<Series>> {
    let map_type = match &mapping.function {
        MapFunction::Builtin(t) => *t,
        MapFunction::UserDefined(_) => {
            bail!("User-defined map functions are not supported");
        }
    };
    let arg = mapping.arg;

    series
        .iter()
        .map(|s| {
            let v: Vec<f64> = match map_type {
                MapType::IsLt => {
                    let a = require_arg(arg, "IsLt")?;
                    s.values.iter().map(|&x| if x < a { 1.0 } else { 0.0 }).collect()
                }
                MapType::IsGt => {
                    let a = require_arg(arg, "IsGt")?;
                    s.values.iter().map(|&x| if x > a { 1.0 } else { 0.0 }).collect()
                }
                MapType::IsEq => {
                    let a = require_arg(arg, "IsEq")?;
                    s.values.iter().map(|&x| if x == a { 1.0 } else { 0.0 }).collect()
                }
                MapType::IsNe => {
                    let a = require_arg(arg, "IsNe")?;
                    s.values.iter().map(|&x| if x != a { 1.0 } else { 0.0 }).collect()
                }
                MapType::IsGe => {
                    let a = require_arg(arg, "IsGe")?;
                    s.values.iter().map(|&x| if x >= a { 1.0 } else { 0.0 }).collect()
                }
                MapType::IsLe => {
                    let a = require_arg(arg, "IsLe")?;
                    s.values.iter().map(|&x| if x <= a { 1.0 } else { 0.0 }).collect()
                }
                MapType::FilterLt => {
                    let a = require_arg(arg, "FilterLt")?;
                    s.values.iter().map(|&x| if x < a { x } else { f64::NAN }).collect()
                }
                MapType::FilterGt => {
                    let a = require_arg(arg, "FilterGt")?;
                    s.values.iter().map(|&x| if x > a { x } else { f64::NAN }).collect()
                }
                MapType::FilterEq => {
                    let a = require_arg(arg, "FilterEq")?;
                    s.values.iter().map(|&x| if x == a { x } else { f64::NAN }).collect()
                }
                MapType::FilterNe => {
                    let a = require_arg(arg, "FilterNe")?;
                    s.values.iter().map(|&x| if x != a { x } else { f64::NAN }).collect()
                }
                MapType::FilterGe => {
                    let a = require_arg(arg, "FilterGe")?;
                    s.values.iter().map(|&x| if x >= a { x } else { f64::NAN }).collect()
                }
                MapType::FilterLe => {
                    let a = require_arg(arg, "FilterLe")?;
                    s.values.iter().map(|&x| if x <= a { x } else { f64::NAN }).collect()
                }
                MapType::Rate => {
                    let mut v = Vec::with_capacity(s.values.len());
                    for i in 0..s.values.len() {
                        if i == 0 {
                            v.push(f64::NAN);
                        } else {
                            let dt = s.timestamps[i] - s.timestamps[i - 1];
                            if dt <= 0.0 {
                                bail!("Non-positive time delta at index {i}");
                            }
                            v.push((s.values[i] - s.values[i - 1]) / dt);
                        }
                    }
                    v
                }
                MapType::Increase => {
                    let mut v = Vec::with_capacity(s.values.len());
                    for i in 0..s.values.len() {
                        if i == 0 {
                            v.push(f64::NAN);
                        } else {
                            v.push(f64::max(0.0, s.values[i] - s.values[i - 1]));
                        }
                    }
                    v
                }
                MapType::Abs => s.values.iter().map(|x| x.abs()).collect(),
                MapType::FillConst => {
                    let a = require_arg(arg, "FillConst")?;
                    s.values.iter().map(|&x| if x.is_nan() { a } else { x }).collect()
                }
                MapType::FillPrev => {
                    let mut prev: Option<f64> = None;
                    let mut v = Vec::with_capacity(s.values.len());
                    for &x in &s.values {
                        if !x.is_nan() {
                            prev = Some(x);
                            v.push(x);
                        } else {
                            match prev {
                                Some(p) => v.push(p),
                                None => bail!("FillPrev: no previous value available"),
                            }
                        }
                    }
                    v
                }
                MapType::Add => {
                    let a = require_arg(arg, "Add")?;
                    s.values.iter().map(|&x| x + a).collect()
                }
                MapType::Sub => {
                    let a = require_arg(arg, "Sub")?;
                    s.values.iter().map(|&x| x - a).collect()
                }
                MapType::Mul => {
                    let a = require_arg(arg, "Mul")?;
                    s.values.iter().map(|&x| x * a).collect()
                }
                MapType::Div => {
                    let a = require_arg(arg, "Div")?;
                    if a == 0.0 {
                        bail!("Division by zero");
                    }
                    s.values.iter().map(|&x| x / a).collect()
                }
                MapType::Min => {
                    let a = require_arg(arg, "Min")?;
                    s.values.iter().map(|&x| x.min(a)).collect()
                }
                MapType::Max => {
                    let a = require_arg(arg, "Max")?;
                    s.values.iter().map(|&x| x.max(a)).collect()
                }
                MapType::InterpolateLinear => {
                    let mut v = s.values.clone();
                    for i in 0..v.len() {
                        if !v[i].is_nan() {
                            continue;
                        }
                        let mut left = i as isize - 1;
                        while left >= 0 && v[left as usize].is_nan() {
                            left -= 1;
                        }
                        let mut right = i + 1;
                        while right < v.len() && v[right].is_nan() {
                            right += 1;
                        }
                        if left >= 0 && right < v.len() {
                            let li = left as usize;
                            let denom = s.timestamps[right] - s.timestamps[li];
                            if denom == 0.0 {
                                bail!("InterpolateLinear: duplicate timestamps");
                            }
                            let frac = (s.timestamps[i] - s.timestamps[li]) / denom;
                            v[i] = v[li] + frac * (v[right] - v[li]);
                        } else {
                            bail!("InterpolateLinear: cannot interpolate at index {i}, no bounding values");
                        }
                    }
                    v
                }
            };
            Ok(Series {
                name: s.name.clone(),
                tags: s.tags.clone(),
                timestamps: s.timestamps.clone(),
                values: v,
            })
        })
        .collect()
}

fn window_pts(s: &Series, t: f64, window_sec: f64) -> Vec<f64> {
    let end = t + window_sec;
    s.timestamps
        .iter()
        .zip(&s.values)
        .filter_map(|(&ts, &val)| {
            if ts >= t && ts < end && !val.is_nan() {
                Some(val)
            } else {
                None
            }
        })
        .collect()
}

fn reduce_time_pts(pts: &[f64], func: TimeType) -> f64 {
    match func {
        TimeType::Sum => pts.iter().sum(),
        TimeType::Avg => pts.iter().sum::<f64>() / pts.len() as f64,
        TimeType::Min => pts.iter().copied().fold(f64::INFINITY, f64::min),
        TimeType::Max => pts.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        TimeType::Count => pts.len() as f64,
        TimeType::Last => *pts.last().unwrap_or(&f64::NAN),
        TimeType::Rate => unreachable!("Rate handled separately"),
    }
}

fn apply_align(series: &[Series], align: &Align) -> Result<Vec<Series>> {
    let func = match &align.function {
        AlignFunction::Builtin(t) => *t,
        AlignFunction::UserDefined(_) => {
            bail!("User-defined align functions are not supported");
        }
    };
    let window_sec = time_to_seconds(get_param(&align.time)?)?;

    series
        .iter()
        .map(|s| {
            if s.timestamps.is_empty() {
                bail!("Cannot align series with no timestamps");
            }
            let start = s.timestamps[0];
            let end = *s.timestamps.last().unwrap();
            let mut new_ts = Vec::new();
            let mut new_vals = Vec::new();

            let mut t = start;
            while t <= end {
                new_ts.push(t);
                let pts = window_pts(s, t, window_sec);

                if func == TimeType::Rate {
                    if pts.len() < 2 {
                        new_vals.push(f64::NAN);
                    } else {
                        let mut increase = 0.0;
                        for j in 1..pts.len() {
                            let delta = pts[j] - pts[j - 1];
                            increase += if delta < 0.0 { pts[j] } else { delta };
                        }
                        new_vals.push(increase / window_sec);
                    }
                } else if pts.is_empty() {
                    new_vals.push(f64::NAN);
                } else {
                    new_vals.push(reduce_time_pts(&pts, func));
                }

                t += window_sec;
            }

            Ok(Series {
                name: s.name.clone(),
                tags: s.tags.clone(),
                timestamps: new_ts,
                values: new_vals,
            })
        })
        .collect()
}

fn apply_group(series: &[Series], group_by: &GroupBy) -> Result<Vec<Series>> {
    if series.is_empty() {
        bail!("Cannot group empty series");
    }
    let func = match &group_by.function {
        GroupFunction::Builtin(t) => *t,
        GroupFunction::UserDefined(_) => {
            bail!("User-defined group functions are not supported");
        }
    };
    let tags = &group_by.tags;

    if tags.is_empty() {
        let values_refs: Vec<&[f64]> = series.iter().map(|s| s.values.as_slice()).collect();
        let vals = aggregate_columns(&values_refs, func)?;
        return Ok(vec![Series {
            tags: HashMap::new(),
            name: format!("{func:?}(all)"),
            timestamps: series[0].timestamps.clone(),
            values: vals,
        }]);
    }

    let mut groups: HashMap<String, Vec<&Series>> = HashMap::new();
    for s in series {
        let key = tags
            .iter()
            .map(|t| require_tag(&s.tags, t))
            .collect::<Result<Vec<_>, _>>()?
            .join("|");
        groups.entry(key).or_default().push(s);
    }

    let mut result = Vec::new();
    for group in groups.values() {
        let values_refs: Vec<&[f64]> = group.iter().map(|s| s.values.as_slice()).collect();
        let vals = aggregate_columns(&values_refs, func)?;
        let ts = group[0].timestamps.clone();
        let mut new_tags = HashMap::new();
        for t in tags {
            new_tags.insert(t.clone(), require_tag(&group[0].tags, t)?);
        }
        result.push(Series {
            name: series_name(&new_tags),
            tags: new_tags,
            timestamps: ts,
            values: vals,
        });
    }
    Ok(result)
}

fn compute_percentile(bucket_vals: &[(f64, f64)], p: f64) -> Result<f64> {
    if bucket_vals.is_empty() {
        bail!("Cannot compute percentile with no buckets");
    }
    let total = bucket_vals.last().unwrap().1;
    if total == 0.0 {
        bail!("Cannot compute percentile: total count is zero");
    }
    let target = p * total;

    for (i, &(le, val)) in bucket_vals.iter().enumerate() {
        if val >= target {
            let prev_val = if i > 0 { bucket_vals[i - 1].1 } else { 0.0 };
            let prev_le = if i > 0 { bucket_vals[i - 1].0 } else { 0.0 };
            if val == prev_val {
                bail!("Unexpected equal adjacent bucket values");
            }
            let fraction = (target - prev_val) / (val - prev_val);
            return Ok(prev_le + fraction * (le - prev_le));
        }
    }
    bail!("Percentile computation failed: target exceeds all buckets")
}

fn compute_bucket_spec(
    spec: &BucketSpec,
    group_series: &[&Series],
    by_le: &[(f64, &Series)],
    t: f64,
    window_sec: f64,
) -> f64 {
    match spec {
        BucketSpec::Percentile(p) => {
            let bucket_vals: Vec<(f64, f64)> = by_le
                .iter()
                .map(|(le, s)| {
                    let pts = window_pts(s, t, window_sec);
                    let val = if pts.is_empty() {
                        0.0
                    } else {
                        pts.iter().sum::<f64>() / pts.len() as f64
                    };
                    (*le, val)
                })
                .collect();
            if bucket_vals.iter().all(|(_, v)| *v == 0.0) {
                return f64::NAN;
            }
            compute_percentile(&bucket_vals, *p).unwrap_or(f64::NAN)
        }
        _ => {
            let all_pts: Vec<f64> = group_series
                .iter()
                .flat_map(|s| window_pts(s, t, window_sec))
                .collect();
            if all_pts.is_empty() {
                return f64::NAN;
            }
            match spec {
                BucketSpec::Count => all_pts.len() as f64,
                BucketSpec::Avg => all_pts.iter().sum::<f64>() / all_pts.len() as f64,
                BucketSpec::Sum => all_pts.iter().sum(),
                BucketSpec::Min => all_pts.iter().copied().fold(f64::INFINITY, f64::min),
                BucketSpec::Max => all_pts.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                BucketSpec::Percentile(_) => unreachable!(),
            }
        }
    }
}

fn spec_name(spec: &BucketSpec) -> String {
    match spec {
        BucketSpec::Percentile(p) => format!("p{}", p * 100.0),
        BucketSpec::Count => "Count".into(),
        BucketSpec::Avg => "Avg".into(),
        BucketSpec::Sum => "Sum".into(),
        BucketSpec::Min => "Min".into(),
        BucketSpec::Max => "Max".into(),
    }
}

fn time_to_seconds(rt: &RelativeTime) -> Result<f64> {
    let v = rt.value as f64;
    match rt.unit {
        TimeUnit::Millisecond => Ok(v / 1000.0),
        TimeUnit::Second => Ok(v),
        TimeUnit::Minute => Ok(v * 60.0),
        TimeUnit::Hour => Ok(v * 3600.0),
        TimeUnit::Day => Ok(v * 86400.0),
        TimeUnit::Week => Ok(v * 604_800.0),
        TimeUnit::Month => bail!("Month time unit is not supported"),
        TimeUnit::Year => bail!("Year time unit is not supported"),
    }
}

fn apply_bucket(series: &[Series], bucket: &BucketBy) -> Result<Vec<Series>> {
    if series.is_empty() {
        bail!("Cannot bucket empty series");
    }
    let window_sec = time_to_seconds(get_param(&bucket.time)?)?;
    let group_tags = &bucket.tags;

    let mut groups: HashMap<String, Vec<&Series>> = HashMap::new();
    for s in series {
        let key = group_tags
            .iter()
            .map(|t| require_tag(&s.tags, t))
            .collect::<Result<Vec<_>, _>>()?
            .join("|");
        groups.entry(key).or_default().push(s);
    }

    let mut result = Vec::new();

    for group_series in groups.values() {
        let mut by_le: Vec<(f64, &Series)> = Vec::new();
        for s in group_series {
            if let Some(le_str) = s.tags.get("le") {
                let le: f64 = le_str
                    .parse()
                    .map_err(|_| eyre!("Invalid 'le' tag value: {le_str}"))?;
                by_le.push((le, s));
            }
        }
        by_le.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        if group_series[0].timestamps.is_empty() {
            bail!("Cannot bucket series with no timestamps");
        }
        let start = group_series[0].timestamps[0];
        let end = *group_series[0].timestamps.last().unwrap();
        let mut new_ts = Vec::new();
        let mut t = start;
        while t <= end {
            new_ts.push(t);
            t += window_sec;
        }

        let mut group_tag_values = HashMap::new();
        for tag in group_tags {
            group_tag_values.insert(tag.clone(), require_tag(&group_series[0].tags, tag)?);
        }

        for spec in &bucket.spec {
            let values: Vec<f64> = new_ts
                .iter()
                .map(|&t| compute_bucket_spec(spec, group_series, &by_le, t, window_sec))
                .collect();

            let sn = spec_name(spec);
            let mut tags = group_tag_values.clone();
            tags.insert("spec".into(), sn.clone());

            result.push(Series {
                name: format!("{}, {sn}", series_name(&group_tag_values)),
                tags,
                timestamps: new_ts.clone(),
                values,
            });
        }
    }

    if result.is_empty() {
        bail!("Bucket produced no output series");
    }
    Ok(result)
}

fn apply_as(series: &[Series], as_: &As) -> Result<Vec<Series>> {
    let name = as_.name.to_string();
    Ok(series
        .iter()
        .map(|s| Series {
            name: name.clone(),
            tags: s.tags.clone(),
            timestamps: s.timestamps.clone(),
            values: s.values.clone(),
        })
        .collect())
}

#[cfg(test)]
mod tests;
