# MPL Language Specification

```mpl
// examples, not always a full query for brevity, but a part of the syntax
```

# Structure of a Query

A query consists of three parts:

1) [the source](#source) of the data
2) [filtering](#filtering) the data
3) [transformation](#transformation) of the data (aggregation and grouping)

Filters must appear before transformations in the pipeline.

## Source

The simple way to define the source of the data is to give it a [dataset](#dataset), a [metric](#metric), and a [time range](#time-range).
In MPL, the syntax for this is as follows:

```mpl
<dataset>:<metric>[<time range>][ as <alias>]
```

- `dataset`: a [ident](#identifiers) that represents the [axiom metrics dataset](#dataset) the data is pulled from
- `metric`: a [ident](#identifiers) that represents the [metric](#metric) name to be queried
- `time range`: The range of data we query. The range can be a [time range](#time-range) or left out to pass the range via the API.
- `alias`: an optional [ident](#identifiers) that represents the alias for the metric. The alias renames the metric within the query.

For example:

```mpl
// query the metric `cpu_usage` in the dataset k8s-metrics-dev over the last hour
`k8s-metrics-dev`:cpu_usage[1h..]

// query the metric `cpu_usage` in the dataset k8s-metrics-dev with a time range supplied via the API
`k8s-metrics-dev`:cpu_usage

// query the metric `cpu_usage` in the dataset k8s-metrics-dev starting from 2 hours ago to 5 minutes ago
`k8s-metrics-dev`:cpu_usage[2h..5m]

// query the metric `cpu_usage` in the dataset k8s-metrics-dev starting from a specific unix timestamp
`k8s-metrics-dev`:cpu_usage[1747077736..]

// query the metric `cpu_usage` in the dataset k8s-metrics-dev starting from a specific ISO 8601 date to one hour later
`k8s-metrics-dev`:cpu_usage[2025-03-01T13:00:00Z..+1h]
```

## Sampling

Before the filter it is possible to sample the data. This can be helpful when designing more complex queries or get a
rough idea about data. Sampling is done on **all** series in the metric before filters are applied and is a percentage value.

Sampling by 0.9 means that 90% of the series will be kept. The syntax is:

```mpl
| sample 0.9
```

The sampling operator is only valid right after the source.

## Filtering

A [metric](#metric) will likely contain multiple series; filtering the series based on a set of tags is common. This is done by using the `where` clause. Filters, like aggregation and grouping, are chained to the data source using the `|` operator. Multiple filter expressions can be chained, equivalent to an `and`. The syntax for the filter is as follows:

```mpl
| where <filter-expr>
```

`filter` is a deprecated alias for `where`. Using `filter` produces a warning.

The filter expression is a boolean expression that has the form:

- `<tag> <operator> <value>` - a [single tag filter](#filter-expression)
- `<tag> is <type>` - a a [single type check](#type-check) 
- `<filter-expr> and <filter-expr>` - a conjunction of two [filter expressions](#filter-expression) using logical `and`
- `<filter-expr> or <filter-expr>` - a disjunction of two [filter expressions](#filter-expression) using logical `or`
- `not <filter-expr>` - a negation of a filter expression
- `(<filter-expr>)` - a [filter expressions](#filter-expression) in parentheses to control the order of evaluation

### Filter Expression

The elements of a single filter expression are as follows:

- `tag` is an [ident](#identifiers)
- `operator` is one of the following:
  - `==` for equality
  - `!=` for inequality (syntactic sugar for `not ... =`)
  - `>` for greater than
  - `<` for less than
  - `>=` for greater than or equal to
  - `<=` for less than or equal to
- `value` one of
  - `"this string"` - a [string](#string)
  - `42` - an [integer](#integer)
  - `42.0` - a [float](#float)
  - `true` - a [bool](#bool)
  - `#/.* regexp $/` - [regular expression](#regex)

```mpl
`k8s-metrics-dev`:cpu_usage[1h..]
| where project == #/.*metrics.*/
| where namespace == "cloud-dev"
| where `service.name` == "query"
```
### Type check

Type checks can be used as a filter to ensure a tag has a certain field. The expression is true only if the:

a) the tag exists in the series
b) the tag has the type specified

Types that are currently supported include:

- `string` - a string value
- `int` - an integer value
- `float` - a float value
- `bool` - a boolean value

the syntax is as follows `| filter the_tag is string`

This allows building expressions for metrics where tags are lacking or of an inconsistent type. For example the following query would allow to filter 200 replies  that are stored as integers or incorrectly in prometheus style as strings.

```
production:http_codes
| filter (code is int and code == 200) or (code is string and code == "200")
| map rate
| align to 5m using avg
```

## Transformation

Once we have filtered the data and only have the series we are interested in, we can transform the
data to extract the information we are interested in.

We distinguish between a number of different transformations:

1) [mapping](#mapping) - A transformation that maps the data to a new value.
2) [aggregation](#aggregation) - A transformation that aggregates the data over a given time to a single value.
3) [grouping](#grouping) - A transformation that groups the data by a set of tags, combining overlapping values.
4) [bucketing](#bucketing) - A two-dimensional transformation that combines the time and tag dimension; this is for histograms.
5) [renaming](#renaming) - Rename the metric within the pipeline.

### Mapping

Mappings are the simplest transformation possible; it is used to alter a series in place without
changing its shape.

Data transformations are done using the `map` operator. The syntax for the map function is as follows:

```mpl
// transforms the data into its rate per second (delta_value/delta_time)
| map rate

// adds 5 to each value
| map + 5

// fills empty values with the previous value
| map fill::prev

// fills empty values with zeros
| map fill::const(0)

// removes datapoints where the value is not less than 0.4
| map filter::lt(0.4)
```


### Aggregation

Aggregation is a transformation that aggregates the data over a given time to a single value. This is done using the `align` operator. The operator also aligns the data to the given window.

![Diagram illustrating the align operator in MPL. The diagram shows a timeline with data points distributed over time. Vertical dashed lines mark intervals; each interval is labeled with an aggregation function such as avg or sum. Arrows indicate how data points within each interval are grouped and processed by the aggregation function. The environment is clean and technical, focusing on explaining how the align operator aggregates data over specified time windows. No emotional tone is present. No additional text is present in the image.](img/align.png)

The syntax for the align is as follows:

```mpl
// calculates the average over 5 minutes
| align to 5m using avg

// counts the datapoints in the last hour
| align to 1h using count
```

### Grouping

Groupings are transformations that group several series using one or more tags as a
new identifier. A grouping will reduce the tags in a metric to the tags used in the grouping.

The `group` operator performs grouping. Using `by <tags>,...` allows grouping by one or more tags.

![Diagram showing the group operator in MPL. Multiple time series lines are displayed, each with different tag values. Colored arrows indicate how series with matching tag values are grouped together into a single aggregated series. The environment is clean and technical, focusing on explaining how grouping by tags combines related series. No emotional tone is present. No additional text is present in the image.](img/group-by.png)

The syntax for the group is as follows:

```mpl
// groups the data by the tags `project` and `namespace`
| group by project, namespace using sum
```

If the `by` part is left out, the grouping combines all series together.

```mpl
// number of series in the last hour
| group using count

// total number of nodes in the last hour
| group using sum
```

### Bucketing

Bucketing is a special case of grouping that combines the time and tag dimensions. This is used for
histograms.

![Diagram illustrating the bucketing operator in MPL. Multiple time series lines are shown, each representing data with different tag values. Vertical lines divide the timeline into intervals, and within each interval, data points are grouped and aggregated into buckets based on both time and tag dimensions. The environment is clean and technical, focusing on explaining how bucketing organizes and summarizes data for histograms. No emotional tone is present. No additional text is present in the image.](img/bucket.png)

The syntax for the bucket is as follows:

```mpl
// Buckets over project and namespace using histogram(count)
| bucket by project, namespace to 5m using histogram(count)
```

### Renaming

The `as` operator renames the metric within the pipeline. It can appear both on the source and as a pipe operator.

```mpl
// renames the metric to `cpu_usage_rate` in a pipe
| as cpu_usage_rate
```

## Computation

The `compute` operator queries two metrics and combines the results into a new metric. The underlying data structure ensures joins are non-overlapping. The `compute` operator supports nested computations.

```mpl
// error rate over the past 5 minutes
(
 `k8s-metrics-dev`:http_requests_total
 | where code >= 400
 | group by method, path using sum,
 `k8s-metrics-dev`:http_requests_total
 | group by method, path using sum;
)
| compute error_rate using /
| align to 5m using avg
```

# Specification

## Identifiers

Identifiers are used to represent fields, metrics, datasets, function names, and other named entities.

An identifier consists of an ASCII letter or `_` followed by zero or more ASCII letters, digits, or `_`.

Identifiers are case sensitive and can be any length.

Identifiers that do not fit the above definition can be escaped using the backtick character `` ` ``.

```mpl
// unescaped identifiers
this_is_1_valid_atom
// escaping an identifier
`this-is also a very valid atom :D`
```

## Time Range

Time ranges are defined by a start and end time. The start time is inclusive, and the end time is exclusive. The syntax is:
`[<start>..<end>?]`

The end time is optional and can be left out. If the end time is left out, it is set to the current time.

Times can be defined as:

- a Unix timestamp in seconds
- an RFC3339 date
- relative times (e.g. `1h`, `2d`, `3w`, `4M`, `5y`) where the time is relative to the current time. The time unit can be one of the following:
  - `ms` for milliseconds (will be rounded to seconds)
  - `s` for seconds
  - `m` for minutes
  - `h` for hours
  - `d` for days
  - `w` for weeks
  - `M` for months
  - `y` for years
- One of the times can be defined as a modifier of `+` or `-` followed by a relative time. This is then added or subtracted from the other time.

```mpl
// one hour ago until now
[1h..]

// The hour following a given Unix timestamp
[1747077736..+1h]

// The hour prior to a given Unix timestamp
[-1h..1747077736]

// The hour before a given RFC3339 date
[-1h..2025-03-01T13:00:00Z]
```

## String

Strings begin with a double quote `"` and end with a double quote `"`. Strings can contain any ASCII
character except for the double quote `"` and the backslash `\`. The backslash is used to escape
special characters.

## Integer

Integers are a sequence of digits. They can be positive or negative.

## Float

Floats are a sequence of digits with a decimal point. They can be positive or negative.
They support exponent notation using `e` or `E` followed by an optional sign and a sequence of
digits.

## Bool

Bools are `true` or `false`. They are case sensitive.

## Regex

Regular expressions are defined using the `#/` prefix followed by the pattern and a closing `/`. They can contain any ASCII character
except for the `/` character. The `\` character is used to escape special characters, only the
`/` , `\`,  `\n`, `\t`, `\r`, `\b`, and `\f` character need to be escaped. `\` can otherwise be used in regular expressions without
escaping. For example, the regex `#/\.*\n/` is a valid regex that matches any number of dots
followed by a newline.

# Concepts

## Dataset

An axiom metrics dataset is a collection of metrics that are stored together.

## Metric

The name of the metric, for example, `cpu_usage` or `http_requests_total`. The metric name is used to
identify the metric and all its encompassing series.

## Tag

A tag is a key-value pair that is used to identify a series. For example, `method="PUT"` is a tag
that identifies the HTTP request method. Tags can be used in filters.

Tag values can be a string, integer, float, or bool.

## Series

A series combines a metric name and a set of tags that identify the time series for a single
measurement entity. For example, `http_requests_total{method="PUT", path="/v1/metrics", code=200}` (in promql text syntax) is a series that represents the number of HTTP requests to the `/v1/metrics` endpoint with a `PUT` method and a `200` response code.

## Data Types

MPL supports distinct types for tag values: string, integer, float, and bool. This enables typed comparisons in filter expressions.

```mpl
// error rate over the past 5 minutes
(
`k8s-metrics-dev`:http_requests_total
 | where code < 400
 | group by code, method, path using sum,
 `k8s-metrics-dev`:http_requests_total as failure
 | where code >= 400
 | group by code, method, path using sum;
)
| compute error_rate using / 
| align to 5m using avg
```

# Draft / Future

This appendix describes features that are specified but not yet available.

## Set Values in Filters

The `in` operator accepts a set of values. Sets are defined using the `[` and `]` characters and can contain any value type.

```mpl
| where code in [200, 201, 202, 203, 204, 205]
```

## Sliding Windows

The `align` operator supports sliding windows, where the window size differs from the step size.

```mpl
// calculates the running average every minute with a sliding window of 5 minutes
| align to 1m over 5m using avg
```

## Tag Renaming

Transformations of the series can modify its tags:

```mpl
// Renames the tag load to cpu_load
| replace load=cpu_load

// replaces the value of hostname using a regular expression, removing the domain
| replace hostname ~ #s/(.*?)\..*/$1/

// create the new tag core from gpu_core and add 1 to it:
| replace core = gpu_core + 1

// remove the tag gpu_core
| remove gpu_core
```

## Format

The `format` operator produces a formatted string from metric information. It is only allowed as the last operator in a query. Tags cannot be returned as table field names, only as strings; the `format` operator addresses this.

The format operator takes a format string and expands it with the metric information.

- `{{metric}}` - the metric name
- `{{resolution}}` - the resolution of the metric in seconds
- `{{.<tag>}}` - the value of the tag

```mpl
// formats the metric as a string for HTTP requests to something like "GET[200]: /api/v1/metrics"
| format "{{.method}}[{.code}}: {{.path}}"
```

## Enriching

The `join` operator pulls tags from another series without reading its data.

```mpl
 test:kube_pod_status_ready
 | group by pod using sum
 | join created_by_kind from test:kube_pod_info by pod
```

## Variables

Variables can be imported from query parameters (see [Query Parameters](#query-parameters)).
They are referenced with a `$` prefix.

```mpl
// a running average over a one-hour window, emitting every $step
| align to $step over 1h using avg
```

## Meta Content

Modifiers supply meta information to the query. They are single-line comments starting with a bang and must appear at the start of the query.

### Strictness (not implemented)

The `strict` modifier enables strict validation of the query.

```mpl
set strict;
```

### Warnings (not implemented)

The `warn` modifier enables additional diagnostic warnings.

```mpl
// warns for possible type conflicts
set warn=types;

// warns for possible performance issues
set warn=performance;

// warns for idiomatic issues (i.e, a built-in linter)
set warn=ideomatic;
```

### Behavioral (not implemented)

The `behaviour` modifier alters query semantics. For example, `regexp-string-conversion` allows regexp to match non-string data by first converting it to a string.

```mpl
set behaviour=regexp-string-conversion;

#...

// This now works for matching codes 100-399
| where code < 400
```

### Editions (not implemented

Editions allow the language to evolve while preserving backward compatibility with existing queries.

```
set edition=2025;
```

### WASM (not implemented)

Users can load custom WASM modules to extend the query language with user-defined functions.

```mpl
// Load wasm module that was uploaded to Axiom in their org
#! wasm=ai_module:do-ai-stuff.wasm
#! wasm=other_module:do-non-ai-stuff.wasm
```

WASM functions can be used in `map`, `align`, `group`, and `bucket` operators:

```mpl
| map wasm::my_module::my_function(1, 2, 3)
| align to 30s using wasm::my_module::my_function
| group by project, namespace using wasm::my_module::my_function(1, 2, 3)
| bucket by project, namespace to 5m using wasm::ai::where_is_waldo
```

### Query Parameters

HTTP query parameters can be passed into the query.

```mpl
// expose step query parameter with the type: duration
param $__interval: duration;
param $ds: dataset;
param $re: regex;
param $str: string;
```

The passed query parameters need to be valid MPL atomics and are prefixed with `param__`.

The prior example a valid query string would be:

```
?param____interval=42s&param__ds=%60my-dataset%60&param__re=%23%2F.*%2F&param__str=%22string+goes+here%22
```
