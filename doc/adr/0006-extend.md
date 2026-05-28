# 6. extend

Date: 2026-05-28

## Status

Accepted

## Context

Sometimes it is helpful to add tags to a query to enrich the results with additional metadata. `extend` is meant to add new fields to the results.

## Decision

We add a `extend` clause to the query that lets the user add new tags. It is a hard requirement that the selected tag is net-new (no series in the query that already has the tag), otherwise the query will fail.

This hard requirement is to avoid complexity of now conflicintg series's and the need to speicfy a merge rule for conflicts.

extend clauses will always come after the aggregation phase, this ensures that the opperation is not making the query more expensive.

extends takes the following form:

```
| extend my_tag = "my value", my_other_tag = "my other value"
```

The initial implementation will only support simple constant values to avoid pulling in the complexity of a expression language into this step. Expressions will be supported in a future version and discussed seperately as they do also affect `filter`, `where` and `declare` and potentally `map`. 

## Consequences

We will be able to add new tags to the results without any conflicts, this comes with limitied utility as only constants will be usable in this itteration.
