use std::fmt::Display;

use crate::{
    Query,
    linker::MapFunction,
    query::{
        Aggregate, Align, As, BucketBy, Cmp, Filter, GroupBy, Mapping, MetricId, RelativeTime,
        Source, Time, TimeRange, TimeUnit,
    },
    types::{BucketType, MapType, Parameterized},
};

fn escape_ident(f: &mut std::fmt::Formatter<'_>, ident: &str) -> std::fmt::Result {
    write!(f, "`{}`", ident.replace('\\', "\\\\").replace('`', "\\`"))
}

impl Display for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for param in self.params() {
            writeln!(f, "param ${}: {};", param.name, param.typ)?;
        }

        match self {
            Query::Simple {
                sample,
                source,
                filters,
                aggregates,
                directives: _,
                params: _,
            } => {
                writeln!(f, "{source}")?;
                if let Some(sample) = sample {
                    writeln!(f, "| sample {sample}")?;
                }
                for filter in filters {
                    writeln!(f, "| where {filter}")?;
                }
                for aggregate in aggregates {
                    writeln!(f, " {aggregate}")?;
                }
            }
            Query::Compute {
                left,
                right,
                name,
                op,
                aggregates,
                directives: _,
                params: _,
            } => {
                writeln!(f, "( {left}, {right} )")?;
                writeln!(f, "| compute {name} using {op}")?;
                for aggregate in aggregates {
                    writeln!(f, " {aggregate}")?;
                }
            }
        }

        Ok(())
    }
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Source {
            metric_id: MetricId { dataset, metric },
            time,
        } = self;
        match dataset {
            Parameterized::Concrete(dataset) => escape_ident(f, dataset)?,
            Parameterized::Param { span: _, param } => {
                write!(f, "$")?;
                escape_ident(f, param.name.as_str())?;
            }
        }
        write!(f, ":")?;
        escape_ident(f, metric)?;
        if let Some(time) = time {
            write!(f, "{time}")?;
        }
        Ok(())
    }
}

impl Display for TimeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}..", self.start)?;
        if let Some(end) = &self.end {
            write!(f, "{end}]")?;
        } else {
            write!(f, "]")?;
        }
        Ok(())
    }
}

impl Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Time::Relative(relative_time) => write!(f, "{relative_time}"),
            Time::Timestamp(t) => write!(f, "{t}"),
            Time::RFC3339(date_time) => write!(f, "{date_time}"),
            Time::Modifier(m) => write!(f, "{m}"),
        }
    }
}

impl Display for RelativeTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.value, self.unit)
    }
}

impl Display for TimeUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeUnit::Millisecond => write!(f, "ms"),
            TimeUnit::Second => write!(f, "s"),
            TimeUnit::Minute => write!(f, "m"),
            TimeUnit::Hour => write!(f, "h"),
            TimeUnit::Day => write!(f, "d"),
            TimeUnit::Week => write!(f, "w"),
            TimeUnit::Month => write!(f, "M"),
            TimeUnit::Year => write!(f, "y"),
        }
    }
}

impl Display for As {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let As { name } = self;
        write!(f, "as {name}")
    }
}

impl Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Filter::And(filters) => {
                if let Some((first, rest)) = filters.split_first() {
                    write!(f, "({first}")?;
                    for filter in rest {
                        write!(f, " and {filter}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Filter::Or(filters) => {
                if let Some((first, rest)) = filters.split_first() {
                    write!(f, "({first}")?;
                    for filter in rest {
                        write!(f, " or {filter}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Filter::Not(filter) => {
                write!(f, "not {filter}")
            }
            Filter::Cmp { field, rhs } => {
                escape_ident(f, field)?;
                write!(f, " {rhs}")
            }
        }
    }
}

impl Display for Cmp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cmp::Eq(v) => write!(f, "== {v}"),
            Cmp::Ne(v) => write!(f, "!= {v}"),
            Cmp::Gt(v) => write!(f, "> {v}"),
            Cmp::Ge(v) => write!(f, ">= {v}"),
            Cmp::Lt(v) => write!(f, "< {v}"),
            Cmp::Le(v) => write!(f, "<= {v}"),
            Cmp::Is(v) => write!(f, "is {v}"),
            Cmp::RegEx(r) => match r {
                Parameterized::Concrete(r) => write!(f, "== {}", r.as_ref()),
                Parameterized::Param { span: _, param } => write!(f, "== ${}", param.name),
            },
            Cmp::RegExNot(r) => match r {
                Parameterized::Concrete(r) => write!(f, "!= {}", r.as_ref()),
                Parameterized::Param { span: _, param } => write!(f, "!= ${}", param.name),
            },
        }
    }
}

impl Display for Aggregate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "| ")?;
        match self {
            Aggregate::As(As { name }) => write!(f, "as {name}"),
            Aggregate::Map(Mapping {
                function: MapFunction::Builtin(MapType::Rate),
                arg: None,
            }) => write!(f, "map rate"),
            Aggregate::Map(map) => write!(f, "map {map}"),
            Aggregate::Align(Align { function, time }) => {
                write!(f, "align to {time} using {function}")
            }
            Aggregate::GroupBy(GroupBy {
                span: _,
                function,
                tags: fields,
            }) => {
                if let Some((field, rest)) = fields.split_first() {
                    write!(f, "group by ")?;
                    escape_ident(f, field)?;
                    for field in rest {
                        write!(f, ", ")?;
                        escape_ident(f, field)?;
                    }
                } else {
                    write!(f, "group ")?;
                }
                write!(f, " using {function}")
            }
            Aggregate::Bucket(BucketBy {
                span: _,
                function,
                time,
                tags: fields,
                spec,
            }) => {
                if let Some((field, rest)) = fields.split_first() {
                    write!(f, "bucket by ")?;
                    escape_ident(f, field)?;
                    for field in rest {
                        write!(f, ", ")?;
                        escape_ident(f, field)?;
                    }
                } else {
                    write!(f, "bucket ")?;
                }
                write!(f, " to {time} using {function}")?;
                // For cumulative histogram, include the mode before bucket specs
                let mode_prefix = if let BucketType::InterpolateCumulativeHistogram(mode) = function
                {
                    Some(mode)
                } else {
                    None
                };
                if let Some((first, rest)) = spec.split_first() {
                    write!(f, "(")?;
                    if let Some(mode) = mode_prefix {
                        write!(f, "{mode}, ")?;
                    }
                    write!(f, "{first}")?;
                    for s in rest {
                        write!(f, ", {s}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
        }
    }
}
impl Display for Mapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mapping {
                function:
                    MapFunction::Builtin(
                        MapType::Mul
                        | MapType::Div
                        | MapType::Add
                        | MapType::Sub
                        | MapType::InterpolateLinear,
                    ),
                arg,
            } => {
                write!(f, " {}", self.function)?;
                if let Some(arg) = arg {
                    write!(f, " {arg}")?;
                }
            }
            Mapping {
                function:
                    MapFunction::Builtin(
                        MapType::Abs
                        | MapType::Max
                        | MapType::Min
                        | MapType::Rate
                        | MapType::FillConst
                        | MapType::FillPrev
                        | MapType::Increase
                        | MapType::FilterLt
                        | MapType::FilterGt
                        | MapType::FilterEq
                        | MapType::FilterNe
                        | MapType::FilterGe
                        | MapType::FilterLe
                        | MapType::IsLt
                        | MapType::IsGt
                        | MapType::IsEq
                        | MapType::IsNe
                        | MapType::IsGe
                        | MapType::IsLe,
                    ),
                arg,
            } => {
                write!(f, "{}", self.function)?;
                if let Some(arg) = arg {
                    write!(f, "({arg})")?;
                }
            }
            Mapping {
                function: MapFunction::UserDefined(func),
                arg,
            } => {
                write!(f, " {func}")?;
                if let Some(arg) = arg {
                    write!(f, " {arg}")?;
                }
            }
        }

        Ok(())
    }
}
