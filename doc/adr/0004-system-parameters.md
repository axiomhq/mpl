# 4. system parameters

Date: 2026-05-08

## Status

Accepted

## Context

We want to be able to provide system parameters without them conflicting with
user-provided parameters.

For example, Axiom currently is automatically providing the param
`$__interval: Duration` to queries in the UI.
We might want to move this logic into the query engine and provide it as a
system parameter.

## Decision

Prevent customers from declaring parameters that start with `__` and instead
reserve them for system parameters.
These system parameters have to be declared types and passed at parse-time.

## Consequences

User-declared parameters that start with `__` will result in a parse warnings. They later will become erros after a transition period
