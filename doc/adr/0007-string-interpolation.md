# 7. string-interpolation

Date: 2026-06-03

## Status

Accepted

## Context

Following up to the [extend ADR](0006-extend.md) we need a way to construct new tag values from existing ones. The minimal useful subset of this is string interpolation. 

The more complex alternatives like full blown expressions `a + "/" + b`, or function calls `concat(a, "/", b)` are equally useful and perhaps more powerful but they have a much large surface.

String interpolation on the flip side is one simple addition to MPL that remains useful with future expansions without opening the proverbial box of pandora.

## Decision

String interpolation is available everywhere where string values are expected. The syntax follows the pattern "... ${<interpolation>} ...".

Inside the `${<interpolation>}` expression, any constant value, parameter or tag can be used. All data is converted to it's string representation.

The expression fails if it references a non-existent tag or parameter.

As interpolations are mormal expressions, nested interpolations are supported even if they're not very useful at the moment.

## Consequences

The language surface grows to string interpolation, the changes pulled in the situation wher enow tags can be compared to other tags (`| where my_tag == other_tag`) as it is a logical expectation once `| where my_tag == "${other_tag}"` works - limiting the compairison to string values only would make no sense and create inconsistent behavior.
