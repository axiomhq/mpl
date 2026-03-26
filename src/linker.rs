//! Builtin and custom function linking for `MPL`
use std::{
    borrow::Borrow,
    collections::HashMap,
    fmt::{Display, Write as _},
};

use serde::{
    Serialize,
    ser::{SerializeMap, SerializeStruct as _},
};

use crate::types::{BucketType, ComputeType, MapType, TagsType, TimeType};

#[derive(Debug, Clone, serde::Serialize)]
/// A function argument
pub enum ArgType {
    /// A floating point argument
    Float,
    /// A enum argument, the value can be any of the values
    Enum(&'static [&'static str]),
    /// A repeated argument
    Repeated {
        /// Type of the repeated argument
        typ: Box<ArgType>,
        /// Minimum number of repetitions
        min: usize,
        /// Maximum number of repetitions
        max: Option<usize>,
    },
    /// The argument can be one of the following types
    OneOf(Vec<ArgType>),
    /// Optional argument
    Optional(Box<ArgType>),
}
impl Display for ArgType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgType::Float => write!(f, "float"),
            ArgType::Enum(values) => write!(f, "enum({})", values.join(", ")),
            ArgType::Repeated { typ, min, max } => {
                write!(f, "repeated({typ}")?;
                if *min > 0 {
                    write!(f, ", min={min}")?;
                }
                if let Some(max) = max {
                    write!(f, ", max={max}")?;
                }
                write!(f, ")")
            }
            ArgType::OneOf(types) => write!(
                f,
                "one_of({})",
                types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            ArgType::Optional(typ) => write!(f, "[{typ}]"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
/// A argument to a function
pub struct Arg {
    /// Name of the argument
    pub name: &'static str,
    /// Type of the argument
    pub typ: ArgType,
}
impl Display for Arg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.typ)
    }
}
impl Arg {
    /// Creates a new argument
    #[must_use]
    pub const fn new(name: &'static str, typ: ArgType) -> Self {
        Self { name, typ }
    }
}

/// Trait for functions
pub trait FunctionTrait {
    /// Documentation of the function
    fn doc(&self) -> &str;
    /// Arguments to the function
    fn args(&self) -> Vec<Arg>;
    /// Creates the description for the function
    fn documentation(&self, name: &FunctionId) -> String {
        let args = self
            .args()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let doc = self.doc();
        let e = if name.0.contains('*') { "__" } else { "**" };
        if args.is_empty() {
            format!(
                r"{e}{name}{e}:
{doc}"
            )
        } else {
            format!(
                r"{e}{name}{e}({args}):

{doc}"
            )
        }
    }
}

/// Module identifier
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub(crate) struct ModuleId(pub(crate) String);

impl std::fmt::Display for ModuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ModuleId {
    pub(crate) fn new(name: &str) -> Self {
        ModuleId(name.to_string())
    }
}

impl Borrow<str> for ModuleId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Function identifier
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct FunctionId(pub(crate) String);

impl std::fmt::Display for FunctionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl FunctionId {
    pub(crate) fn new(name: &str) -> Self {
        FunctionId(name.to_string())
    }
}

impl Borrow<str> for FunctionId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

pub(crate) struct Function {
    pub(crate) module_path: Vec<ModuleId>,
    pub(crate) name: FunctionId,
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for m in &self.module_path {
            write!(f, "{m}::")?;
        }
        write!(f, "{}", self.name)
    }
}

/// Module definition
pub struct Module {
    pub(crate) name: ModuleId,
    pub(crate) doc: &'static str,
    pub(crate) align_functions: HashMap<FunctionId, AlignFunction>,
    pub(crate) mapping_functions: HashMap<FunctionId, MapFunction>,
    pub(crate) group_functions: HashMap<FunctionId, GroupFunction>,
    pub(crate) bucket_functions: HashMap<FunctionId, BucketType>,
    pub(crate) compute_functions: HashMap<FunctionId, ComputeFunction>,
    pub(crate) submodules: HashMap<ModuleId, Module>,
}

impl std::fmt::Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Module: {}", self.name)
    }
}

struct FunctionSerializer<'a, F: FunctionTrait>(&'a F);

impl<F> Serialize for FunctionSerializer<'_, F>
where
    F: FunctionTrait,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Function", 2)?;
        state.serialize_field("doc", self.0.doc())?;
        state.serialize_field("args", &self.0.args())?;
        state.end()
    }
}

struct FunctionMapSerializer<'a, F: FunctionTrait>(&'a HashMap<FunctionId, F>);

impl<F> Serialize for FunctionMapSerializer<'_, F>
where
    F: FunctionTrait,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_map(Some(self.0.len()))?;
        for (id, func) in self.0 {
            state.serialize_entry(&id.to_string(), &FunctionSerializer(func))?;
        }
        state.end()
    }
}

impl Serialize for Module {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Module", 8)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("doc", &self.doc)?;
        state.serialize_field(
            "align_functions",
            &FunctionMapSerializer(&self.align_functions),
        )?;
        state.serialize_field(
            "mapping_functions",
            &FunctionMapSerializer(&self.mapping_functions),
        )?;
        state.serialize_field(
            "group_functions",
            &FunctionMapSerializer(&self.group_functions),
        )?;
        state.serialize_field(
            "compute_functions",
            &FunctionMapSerializer(&self.compute_functions),
        )?;
        state.serialize_field(
            "bucket_functions",
            &FunctionMapSerializer(&self.bucket_functions),
        )?;
        state.serialize_field("submodules", &self.submodules)?;
        state.end()
    }
}

impl Module {
    /// Generates tye markdown style documentation for hte module
    pub fn documentation(&self, level: usize) -> Result<String, std::fmt::Error> {
        let header = "#".repeat(level + 1);
        let mut functions = String::new();
        let mut align_functions: Vec<_> = self.align_functions.iter().collect();

        align_functions.sort_by_key(|(i, _)| *i);
        if !align_functions.is_empty() {
            writeln!(&mut functions, "#{header} Align Functions")?;
        }
        for (n, f) in &align_functions {
            writeln!(&mut functions, "{}", f.documentation(n))?;
            writeln!(&mut functions)?;
        }
        let mut mapping_functions: Vec<_> = self.mapping_functions.iter().collect();
        mapping_functions.sort_by_key(|(i, _)| *i);
        if !mapping_functions.is_empty() {
            writeln!(&mut functions, "#{header} Map Functions")?;
        }
        for (n, f) in &mapping_functions {
            writeln!(&mut functions, "{}", f.documentation(n))?;
            writeln!(&mut functions)?;
        }
        let mut group_functions: Vec<_> = self.group_functions.iter().collect();
        group_functions.sort_by_key(|(i, _)| *i);
        if !group_functions.is_empty() {
            writeln!(&mut functions, "#{header} Group Functions")?;
        }
        for (n, f) in &group_functions {
            writeln!(&mut functions, "{}", f.documentation(n))?;
            writeln!(&mut functions)?;
        }
        let mut compute_functions: Vec<_> = self.compute_functions.iter().collect();
        compute_functions.sort_by_key(|(i, _)| *i);
        if !compute_functions.is_empty() {
            writeln!(&mut functions, "#{header} Compute Functions")?;
        }
        for (n, f) in &compute_functions {
            writeln!(&mut functions, "{}", f.documentation(n))?;
            writeln!(&mut functions)?;
        }
        let mut bucket_functions: Vec<_> = self.bucket_functions.iter().collect();
        bucket_functions.sort_by_key(|(i, _)| *i);
        if !bucket_functions.is_empty() {
            writeln!(&mut functions, "#{header} Bucket Functions")?;
        }
        for (n, f) in &bucket_functions {
            writeln!(&mut functions, "{}", f.documentation(n))?;
            writeln!(&mut functions)?;
        }

        let mut submodule_list: Vec<_> = self.submodules.iter().collect();
        submodule_list.sort_by_key(|(i, _)| *i);
        let mut submodules = String::new();
        for (_, m) in submodule_list {
            writeln!(&mut submodules, "{}", m.documentation(level + 1)?)?;
        }
        Ok(format!(
            r"{header} {name}
{doc}
{functions}{submodules}",
            name = self.name,
            doc = self.doc,
        ))
    }
    pub(crate) fn map_fn(&self, id: &Function) -> Option<&MapFunction> {
        self.map_fn_(&id.module_path, &id.name)
    }
    fn map_fn_(&self, modules: &[ModuleId], id: &FunctionId) -> Option<&MapFunction> {
        if let Some((first, rest)) = modules.split_first() {
            self.submodules.get(first)?.map_fn_(rest, id)
        } else {
            self.mapping_functions.get(id)
        }
    }

    pub(crate) fn align_fn(&self, id: &Function) -> Option<&AlignFunction> {
        self.align_fn_(&id.module_path, &id.name)
    }
    fn align_fn_(&self, modules: &[ModuleId], id: &FunctionId) -> Option<&AlignFunction> {
        if let Some((first, rest)) = modules.split_first() {
            self.submodules.get(first)?.align_fn_(rest, id)
        } else {
            self.align_functions.get(id)
        }
    }
    pub(crate) fn group_fn(&self, id: &Function) -> Option<&GroupFunction> {
        self.group_fn_(&id.module_path, &id.name)
    }
    fn group_fn_(&self, modules: &[ModuleId], id: &FunctionId) -> Option<&GroupFunction> {
        if let Some((first, rest)) = modules.split_first() {
            self.submodules.get(first)?.group_fn_(rest, id)
        } else {
            self.group_functions.get(id)
        }
    }

    pub(crate) fn compute_fn(&self, id: &Function) -> Option<&ComputeFunction> {
        self.compute_fn_(&id.module_path, &id.name)
    }
    fn compute_fn_(&self, modules: &[ModuleId], id: &FunctionId) -> Option<&ComputeFunction> {
        if let Some((first, rest)) = modules.split_first() {
            self.submodules.get(first)?.compute_fn_(rest, id)
        } else {
            self.compute_functions.get(id)
        }
    }
}

/// User supplied Mapping function
pub trait MapFunctionTrait:
    Send + Sync + std::fmt::Debug + std::fmt::Display + FunctionTrait
{
    /// calls the function
    #[must_use]
    fn call(&self, input: &str) -> String;
    /// Creates a boxed clone of the function object (clone doesn't work with boxed traites)
    #[must_use]
    fn box_clone(&self) -> Box<dyn MapFunctionTrait>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
/// A map functio wrapper
pub enum MapFunction {
    /// A builtin function
    Builtin(MapType),
    #[serde(skip)]
    /// A use defined function
    UserDefined(Box<dyn MapFunctionTrait>),
}

impl FunctionTrait for MapFunction {
    fn doc(&self) -> &str {
        match self {
            MapFunction::Builtin(t) => t.doc(),
            MapFunction::UserDefined(func) => func.doc(),
        }
    }
    fn args(&self) -> Vec<Arg> {
        match self {
            MapFunction::Builtin(t) => t.args(),
            MapFunction::UserDefined(func) => func.args(),
        }
    }
}

impl From<MapType> for MapFunction {
    fn from(t: MapType) -> Self {
        MapFunction::Builtin(t)
    }
}
impl std::fmt::Display for MapFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapFunction::Builtin(t) => write!(f, "{t}"),
            MapFunction::UserDefined(func) => write!(f, "{func}"),
        }
    }
}
impl Clone for MapFunction {
    fn clone(&self) -> Self {
        match self {
            MapFunction::Builtin(t) => MapFunction::Builtin(*t),
            MapFunction::UserDefined(func) => MapFunction::UserDefined(func.box_clone()),
        }
    }
}

/// User supplied Mapping function
pub trait AlignFunctionTrait:
    Send + Sync + std::fmt::Debug + std::fmt::Display + FunctionTrait
{
    /// calls the function
    #[must_use]
    fn call(&self, input: &str) -> String;
    /// Creates a boxed clone of the function object (clone doesn't work with boxed traites)
    #[must_use]
    fn box_clone(&self) -> Box<dyn AlignFunctionTrait>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
/// A align function wrapper
pub enum AlignFunction {
    /// A builtin function
    Builtin(TimeType),
    #[serde(skip)]
    /// A use defined function
    UserDefined(Box<dyn AlignFunctionTrait>),
}
impl FunctionTrait for AlignFunction {
    fn doc(&self) -> &str {
        match self {
            AlignFunction::Builtin(t) => t.doc(),
            AlignFunction::UserDefined(func) => func.doc(),
        }
    }
    fn args(&self) -> Vec<Arg> {
        match self {
            AlignFunction::Builtin(t) => t.args(),
            AlignFunction::UserDefined(func) => func.args(),
        }
    }
}
impl From<TimeType> for AlignFunction {
    fn from(t: TimeType) -> Self {
        AlignFunction::Builtin(t)
    }
}
impl std::fmt::Display for AlignFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlignFunction::Builtin(t) => write!(f, "{t}"),
            AlignFunction::UserDefined(func) => write!(f, "{func}"),
        }
    }
}
impl Clone for AlignFunction {
    fn clone(&self) -> Self {
        match self {
            AlignFunction::Builtin(t) => AlignFunction::Builtin(*t),
            AlignFunction::UserDefined(func) => AlignFunction::UserDefined(func.box_clone()),
        }
    }
}

/// User supplied Mapping function
pub trait GroupFunctionTrait:
    Send + Sync + std::fmt::Debug + std::fmt::Display + FunctionTrait
{
    /// calls the function
    #[must_use]
    fn call(&self, input: &str) -> String;
    /// Creates a boxed clone of the function object (clone doesn't work with boxed traites)
    #[must_use]
    fn box_clone(&self) -> Box<dyn GroupFunctionTrait>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
/// A group-by function wrapper
pub enum GroupFunction {
    /// A builtin function
    Builtin(TagsType),
    #[serde(skip)]
    /// A use defined function
    UserDefined(Box<dyn GroupFunctionTrait>),
}
impl FunctionTrait for GroupFunction {
    fn doc(&self) -> &str {
        match self {
            GroupFunction::Builtin(t) => t.doc(),
            GroupFunction::UserDefined(func) => func.doc(),
        }
    }
    fn args(&self) -> Vec<Arg> {
        match self {
            GroupFunction::Builtin(t) => t.args(),
            GroupFunction::UserDefined(func) => func.args(),
        }
    }
}
impl From<TagsType> for GroupFunction {
    fn from(t: TagsType) -> Self {
        GroupFunction::Builtin(t)
    }
}
impl std::fmt::Display for GroupFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupFunction::Builtin(t) => write!(f, "{t}"),
            GroupFunction::UserDefined(func) => write!(f, "{func}"),
        }
    }
}
impl Clone for GroupFunction {
    fn clone(&self) -> Self {
        match self {
            GroupFunction::Builtin(t) => GroupFunction::Builtin(*t),
            GroupFunction::UserDefined(func) => GroupFunction::UserDefined(func.box_clone()),
        }
    }
}

/// User supplied Compute function
pub trait ComputeFunctionTrait:
    Send + Sync + std::fmt::Debug + std::fmt::Display + FunctionTrait
{
    /// calls the function
    #[must_use]
    fn call(&self, input: &str) -> String;
    /// Creates a boxed clone of the function object (clone doesn't work with boxed traites)
    #[must_use]
    fn box_clone(&self) -> Box<dyn ComputeFunctionTrait>;
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
/// A compute function wrapper
pub enum ComputeFunction {
    /// A builtin function
    Builtin(ComputeType),
    #[serde(skip)]
    /// A use defined function
    UserDefined(Box<dyn ComputeFunctionTrait>),
}
impl FunctionTrait for ComputeFunction {
    fn doc(&self) -> &str {
        match self {
            ComputeFunction::Builtin(t) => t.doc(),
            ComputeFunction::UserDefined(func) => func.doc(),
        }
    }
    fn args(&self) -> Vec<Arg> {
        match self {
            ComputeFunction::Builtin(t) => t.args(),
            ComputeFunction::UserDefined(func) => func.args(),
        }
    }
}
impl From<ComputeType> for ComputeFunction {
    fn from(c: ComputeType) -> Self {
        ComputeFunction::Builtin(c)
    }
}
impl std::fmt::Display for ComputeFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComputeFunction::Builtin(t) => write!(f, "{t}"),
            ComputeFunction::UserDefined(func) => write!(f, "{func}"),
        }
    }
}
impl Clone for ComputeFunction {
    fn clone(&self) -> Self {
        match self {
            ComputeFunction::Builtin(c) => ComputeFunction::Builtin(*c),
            ComputeFunction::UserDefined(func) => ComputeFunction::UserDefined(func.box_clone()),
        }
    }
}
