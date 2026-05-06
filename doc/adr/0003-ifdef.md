# 3. ifdef

Date: 2026-05-05

## Status

Accepted

## Context

There is a reasonable situation where a user might want to use parameters to conditionally disable a filter and not filter on a tag.

As an example, in a dashboard the following query could be used to show average cpu usage for either a region or the entire deployment.

```
param $interval: Duration;
param $region: string;

prod:cpu_usage
| where region == $region
| align to $interval using avg
| group by using avg
```

As of today this is not possible, if `region` is not provided the query errors.

The same pattern of optional values emerges when we consider parameterisation or handling of absent tags. A parameterized range filter could for example take a lower and upper bound that are optional and only apply them if passed to it.

We have explored a number of options and discarded them, here a non exhaustive list:

`?` - that ignores a pipe when a value isn't set, too confusing when `and` or `or` is used in a where clause (`where region == $region? and provider == "aws"` — it is unclear if the second part of the where `provider == "aws"` applies)

`maybe` - introducing a maybe block `maybe { where region == $region? }` the whole block is discarded if a `?` fails - too complex and too many new concepts at once.

`$region == ""` - encoding the 'we do not care' in the type space, impossible for many types, moving variables to the left clashes with tag variables.

`isset($region)` - nearly identical as above, moving variables will clash with types, denies us future development in parameterisation and custom functions.

`with($region) {...}` - executes the block only if the (or all) variables are set. with is not expressive enough making it unintuitive to understand and creates a weird unique expression. Other verbiage (`with_if`, `where_with`, `scoped`, `with_tag`) were discarded for similar reasons

## Decision

The core of the decision is to move 'is a parameter set' into a wrapper type and **not** in the type space, allowing it to work with arbitrary types - not just strings.

We introduce optional parameters as an `Option` type and the `ifdef(..) {}` block.

An optional parameter is declared as:

```
param $region: Option<string>;
```

As part of the query they can only be used in ifdef blocks to scope their actual value inside the block:

```
...
| ifdef($region) { where region == $region }
... 
```

What this is not in this first iteration:

- ifdef only takes a single variable: `ifdef($a, $b)` is not supported
- ifdef for tags: `ifdef(tag)` - only variables are supported 
- ifdef else: `ifdef(...) {...} else {...}`  - we will not implement a else block yet
- complex expressions in ifdef blocks (for now we only allow where) - we will only allow a single `where` inside the if-block for the time being


## Consequences

Optional parameters become a first class language concept. Consumers of the AST are responsible to handle the removal of ifdef blocks when they do param resolution.
