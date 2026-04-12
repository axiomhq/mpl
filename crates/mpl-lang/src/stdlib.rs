//! Standard library module.
use std::{collections::HashMap, sync::LazyLock};

use crate::{
    linker::{FunctionId, Module, ModuleId},
    types::{BucketType, ComputeType, ConversionMethod, MapType, TagsType, TimeType},
};
/// `MPL` standard Library
pub static STDLIB: LazyLock<Module> = LazyLock::new(|| {
    let mut mapping_functions = HashMap::new();
    mapping_functions.insert(FunctionId::new("rate"), MapType::Rate.into());
    mapping_functions.insert(FunctionId::new("increase"), MapType::Increase.into());
    mapping_functions.insert(FunctionId::new("max"), MapType::Max.into());
    mapping_functions.insert(FunctionId::new("min"), MapType::Min.into());
    mapping_functions.insert(FunctionId::new("abs"), MapType::Abs.into());
    // eval functions
    mapping_functions.insert(FunctionId::new("+"), MapType::Add.into());
    mapping_functions.insert(FunctionId::new("-"), MapType::Sub.into());
    mapping_functions.insert(FunctionId::new("*"), MapType::Mul.into());
    mapping_functions.insert(FunctionId::new("/"), MapType::Div.into());
    let mut align_functions = HashMap::new();
    align_functions.insert(FunctionId::new("min"), TimeType::Min.into());
    align_functions.insert(FunctionId::new("max"), TimeType::Max.into());
    align_functions.insert(FunctionId::new("avg"), TimeType::Avg.into());
    align_functions.insert(FunctionId::new("sum"), TimeType::Sum.into());
    align_functions.insert(FunctionId::new("count"), TimeType::Count.into());
    align_functions.insert(FunctionId::new("last"), TimeType::Last.into());
    let mut group_functions = HashMap::new();
    group_functions.insert(FunctionId::new("min"), TagsType::Min.into());
    group_functions.insert(FunctionId::new("max"), TagsType::Max.into());
    group_functions.insert(FunctionId::new("avg"), TagsType::Avg.into());
    group_functions.insert(FunctionId::new("sum"), TagsType::Sum.into());
    group_functions.insert(FunctionId::new("count"), TagsType::Count.into());
    let mut bucket_functions = HashMap::new();
    bucket_functions.insert(FunctionId::new("histogram"), BucketType::Histogram);
    bucket_functions.insert(
        FunctionId::new("interpolate_delta_histogram"),
        BucketType::InterpolateDeltaHistogram,
    );
    bucket_functions.insert(
        FunctionId::new("interpolate_cumulative_histogram"),
        // Docs will print both rate and increase
        BucketType::InterpolateCumulativeHistogram(ConversionMethod::Rate),
    );
    let mut compute_functions = HashMap::new();
    compute_functions.insert(FunctionId::new("min"), ComputeType::Min.into());
    compute_functions.insert(FunctionId::new("max"), ComputeType::Max.into());
    compute_functions.insert(FunctionId::new("avg"), ComputeType::Avg.into());
    compute_functions.insert(FunctionId::new("/"), ComputeType::Div.into());
    compute_functions.insert(FunctionId::new("*"), ComputeType::Mul.into());
    compute_functions.insert(FunctionId::new("+"), ComputeType::Add.into());
    compute_functions.insert(FunctionId::new("-"), ComputeType::Sub.into());

    let mut prom_align_functions = HashMap::new();
    prom_align_functions.insert(FunctionId::new("rate"), TimeType::Rate.into());
    let prom = Module {
        name: ModuleId::new("prom"),
        doc: "Prometheus compatibility functions",
        submodules: HashMap::new(),
        align_functions: prom_align_functions,
        mapping_functions: HashMap::new(),
        group_functions: HashMap::new(),
        compute_functions: HashMap::new(),
        bucket_functions: HashMap::new(),
    };

    let mut interpolate_map_functions = HashMap::new();
    interpolate_map_functions.insert(FunctionId::new("linear"), MapType::InterpolateLinear.into());
    let interpolate = Module {
        name: ModuleId::new("interpolate"),
        doc: "Interpolation functions",
        submodules: HashMap::new(),
        align_functions: HashMap::new(),
        mapping_functions: interpolate_map_functions,
        group_functions: HashMap::new(),
        compute_functions: HashMap::new(),
        bucket_functions: HashMap::new(),
    };

    let mut interpolate_filter_functions = HashMap::new();
    interpolate_filter_functions.insert(FunctionId::new("lt"), MapType::FilterLt.into());
    interpolate_filter_functions.insert(FunctionId::new("gt"), MapType::FilterGt.into());
    interpolate_filter_functions.insert(FunctionId::new("lte"), MapType::FilterLe.into());
    interpolate_filter_functions.insert(FunctionId::new("gte"), MapType::FilterGe.into());
    interpolate_filter_functions.insert(FunctionId::new("eq"), MapType::FilterEq.into());
    interpolate_filter_functions.insert(FunctionId::new("neq"), MapType::FilterNe.into());

    let filter = Module {
        name: ModuleId::new("filter"),
        doc: "Filter functions, remove non matching datapoints from a time series",
        submodules: HashMap::new(),
        align_functions: HashMap::new(),
        mapping_functions: interpolate_filter_functions,
        group_functions: HashMap::new(),
        compute_functions: HashMap::new(),
        bucket_functions: HashMap::new(),
    };

    let mut interpolate_is_functions = HashMap::new();
    interpolate_is_functions.insert(FunctionId::new("lt"), MapType::IsLt.into());
    interpolate_is_functions.insert(FunctionId::new("gt"), MapType::IsGt.into());
    interpolate_is_functions.insert(FunctionId::new("lte"), MapType::IsLe.into());
    interpolate_is_functions.insert(FunctionId::new("gte"), MapType::IsGe.into());
    interpolate_is_functions.insert(FunctionId::new("eq"), MapType::IsEq.into());
    interpolate_is_functions.insert(FunctionId::new("neq"), MapType::IsNe.into());

    let is = Module {
        name: ModuleId::new("is"),
        doc: "Test functions, set the value of matching datapoints to 1, and non matching datapoints to 0",
        submodules: HashMap::new(),
        align_functions: HashMap::new(),
        mapping_functions: interpolate_is_functions,
        group_functions: HashMap::new(),
        compute_functions: HashMap::new(),
        bucket_functions: HashMap::new(),
    };

    let mut fill_map_functions = HashMap::new();
    fill_map_functions.insert(FunctionId::new("prev"), MapType::FillPrev.into());
    fill_map_functions.insert(FunctionId::new("const"), MapType::FillConst.into());
    let fill = Module {
        name: ModuleId::new("fill"),
        doc: "Functions to fill not set datapoints with values",
        submodules: HashMap::new(),
        align_functions: HashMap::new(),
        mapping_functions: fill_map_functions,
        group_functions: HashMap::new(),
        compute_functions: HashMap::new(),
        bucket_functions: HashMap::new(),
    };

    let mut submodules = HashMap::new();
    submodules.insert(ModuleId::new("prom"), prom);
    submodules.insert(ModuleId::new("interpolate"), interpolate);
    submodules.insert(ModuleId::new("filter"), filter);
    submodules.insert(ModuleId::new("is"), is);
    submodules.insert(ModuleId::new("fill"), fill);

    Module {
        name: ModuleId::new("std"),
        doc: "mpl standard library",
        submodules,
        align_functions,
        mapping_functions,
        group_functions,
        compute_functions,
        bucket_functions,
    }
});
