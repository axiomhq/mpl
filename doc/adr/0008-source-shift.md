# 8. Source offset

Date: 2026-09-24

## Status

Proposed

## Context

Comparing a metric across two different time ranges is... pretty routine. For instance, PromQL supports this through the `offset` operator. We invert its logic: PromQL's negative offsets move forward in time; ours move backward. Positive offsets will move forward. AKA: the sign matches the direction.

## Decision

Add a `offset` operator immediately after the source, before sample, filters and aggregations.

```mpl
metrics:cpu | offset -1h
```

- The API continues to supply the query time range. A statement like `offset -1h` for a query with the time range 10:00am – 11:00am will read the metric values from 9:00 – 10:00am.
- A single source gets a single offset. Each source will decide the time period to read before anything else runs. Allowing repeated offsets would open up a bunch of unneccessary complexity. For instance, do they stack up or replace one another?
- Without an offset, read the API's time range.
- Require `-` before the duration: negative means earlier. Keep the existing duration syntax and units. Forward offsets (`1h` or `+1h`) can come later.
- Thanks to Heinz for the idea: `Source` stores how far back to read. We give that number its own type, `Offset`, so Rust catches timestamp errors (the same idea as [ifdef](0003-ifdef.md)).
  - For instance, a offset only works if the window can move back that far: offseting `10..20` back by `11` would start at `-1`. `Timerange::offset` does the subtraction and catches that error, so every caller uses the same check.

## Consequences

A `offset` changes which value belongs at each time in the result. With `offset -1h` at 10:15, `result at 10:15 = source value at 9:15`. So in spite of how confusing it might feel:
- the result’s time is 10:15
- the value comes from 9:15

Reading older data is only half the work -> we must also add the offset to the returned timestamps so the comparison pairs the right samples together.

Consumers of the `Source` struct must handle a new `offset` field. Unsure if it's just the metrics repo or what.

Keep an omitted `offset` distinct from an explicit `offset -0s` when formatting
