# 2. language-level-types

Date: 2026-04-30

## Status

Accepted

## Context

We sopport two kind of types:

1. otel types such as `int`, `float`, `bool`, `string`
2. language level types such as `Dataset`, `Duration`, `Regex` and eventually others as the language grows


It is helpful to users to make this difference obvious.

## Decision

We apply the guideline that otel types are treated core types using lower case naming such as `int` while language level types use upper case naming `Dataset`.

## Consequences

A clear distinction can be made between language level and data level types.
