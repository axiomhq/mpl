type Span = { offset: number; length: number };

type Dataset = string;
type Metric = string;
type EncodableRegex = string;
type DirectiveValue = string | number | boolean | null;
type TagValue = string | number | boolean | null;

type Parameterized<T> =
  | { Concrete: T }
  | { Param: { span: Span; param: ParamDeclaration } };

type MetricId = {
  dataset: Parameterized<Dataset>;
  metric: Metric;
};

type TimeUnit =
  | "Millisecond"
  | "Second"
  | "Minute"
  | "Hour"
  | "Day"
  | "Week"
  | "Month"
  | "Year";

type RelativeTime = {
  value: number;
  unit: TimeUnit;
};

type Time =
  | { Relative: RelativeTime }
  | { Timestamp: number }
  | { RFC3339: string }
  | { Modifier: string };

type TimeRange = {
  start: Time;
  end: Time | null;
};

type Source = {
  metric_id: MetricId;
  time: TimeRange | null;
};

type TagType = "String" | "Int" | "Float" | "Bool" | "Null";

type Cmp =
  | { Eq: Parameterized<TagValue> }
  | { Ne: Parameterized<TagValue> }
  | { Gt: Parameterized<TagValue> }
  | { Ge: Parameterized<TagValue> }
  | { Lt: Parameterized<TagValue> }
  | { Le: Parameterized<TagValue> }
  | { RegEx: Parameterized<EncodableRegex> }
  | { RegExNot: Parameterized<EncodableRegex> }
  | { Is: TagType };

type As = {
  name: Metric;
};

type Filter =
  | { And: Filter[] }
  | { Or: Filter[] }
  | { Not: Filter }
  | { Cmp: { field: string; rhs: Cmp } };

type FilterOrIfDef =
  | { Filter: Filter }
  | {
      Ifdef: {
        param: ParamDeclaration;
        filter: Filter;
        else_filter: Filter | null;
      };
    };

type MapType =
  | "Min"
  | "Max"
  | "Rate"
  | "Add"
  | "Sub"
  | "Mul"
  | "Div"
  | "Abs"
  | "FillConst"
  | "FillPrev"
  | "Increase"
  | "FilterLt"
  | "FilterGt"
  | "FilterEq"
  | "FilterNe"
  | "FilterGe"
  | "FilterLe"
  | "IsLt"
  | "IsGt"
  | "IsEq"
  | "IsNe"
  | "IsGe"
  | "IsLe"
  | "InterpolateLinear";

type TimeType = "Count" | "Sum" | "Avg" | "Min" | "Max" | "Rate" | "Last";

type TagsType = "Count" | "Sum" | "Avg" | "Min" | "Max";

type ComputeType = "Avg" | "Min" | "Max" | "Add" | "Sub" | "Mul" | "Div";

type ConversionMethod = "Rate" | "Increase";

type BucketType =
  | "Histogram"
  | "InterpolateDeltaHistogram"
  | { InterpolateCumulativeHistogram: ConversionMethod };

type BucketSpec =
  | "Count"
  | "Avg"
  | "Sum"
  | "Min"
  | "Max"
  | { Percentile: number };

type MapFunction = { Builtin: MapType };
type AlignFunction = { Builtin: TimeType };
type GroupFunction = { Builtin: TagsType };
type ComputeFunction = { Builtin: ComputeType };

type Mapping = {
  function: MapFunction;
  arg: number | null;
};

type Align = {
  function: AlignFunction;
  time: Parameterized<RelativeTime> | null;
};

type GroupBy = {
  span: Span;
  function: GroupFunction;
  tags: string[];
};

type BucketBy = {
  span: Span;
  function: BucketType;
  time: Parameterized<RelativeTime> | null;
  tags: string[];
  spec: BucketSpec[];
};

type Aggregate =
  | { Map: Mapping }
  | { Align: Align }
  | { GroupBy: GroupBy }
  | { Bucket: BucketBy }
  | { As: As };

type TagExtend = {
  tag: string;
  value: Parameterized<TagValue>;
};

type TerminalParamType = "Duration" | "Dataset" | "Regex" | { Tag: TagType };

type ParamType =
  | { Terminal: TerminalParamType }
  | { Optional: TerminalParamType };

type ParamDeclaration = {
  span: Span;
  name: string;
  typ: ParamType;
};

export type Query =
  | {
      Simple: {
        source: Source;
        filters: FilterOrIfDef[];
        aggregates: Aggregate[];
        directives: Record<string, DirectiveValue>;
        params: ParamDeclaration[];
        extends?: TagExtend[];
        sample: number | null;
      };
    }
  | {
      Compute: {
        left: Query;
        right: Query;
        name: Metric;
        op: ComputeFunction;
        aggregates: Aggregate[];
        extends?: TagExtend[];
        directives: Record<string, DirectiveValue>;
        params: ParamDeclaration[];
      };
    };

export function parse_wasm(query: string): Query;
