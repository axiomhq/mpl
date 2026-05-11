# 5. whole window

Date: 2026-05-11

## Status

Accepted

## Context

There is a common situation where a user wants to produce exactly one value per series for the
entire queried time range.

This is especially useful for visualisations such as stat and pie charts, where a
single summarized value is usually more meaningful than a time series with many points.

For example, in a dashboard a user may want to show the total number of active alerts over the selected query range:

```
prod:alertmanager_alerts
| where region == $region
| align using sum
```

Before this decision, users had to approximate this behavior by manually choosing a large align
interval:

```
prod:alertmanager_alerts
| where region == $region
| align to 60d using sum
```

This has several drawbacks:

- the user must manually pass a value intended to match the query range
- `align to ...` does not actually guarantee a single output value per series
- it introduces a window-size concept even when the intent is not windowing but whole-range reduction

We explored following alternative and rejected:

`summarize`
: introducing a separate operator that guarantees a single value per series would work, but it would
  largely duplicate `align` and `bucket` semantics while adding another top-level concept to the
  language.

## Decision

Following the same pattern as `group using sum`, where omission of `by` means "operate over the
entire grouping dimension", we allow omission of `to` for time-reducing operators to mean "operate
over the entire queried time range".

This applies to `align`:

```
prod:alertmanager_alerts
| where region == $region
| align using avg
```

and to `bucket`:

```
prod:http_server_request_duration_seconds_bucket
| bucket by service.name using histogram(count, 0.95)
```

In this form:

- `align using <fn>` means: produce one value per series using the whole query window
- `bucket ... using <fn>(...)` without `to` means: aggregate each bucket over the whole query window

## Consequences

Users can express whole-range aggregation directly without passing a synthetic duration.

The language becomes more explicit about intent: omitting `to` means "whole query range", while
including `to` means "windowed aggregation".
