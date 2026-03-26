//! Visitor pattern for query

use crate::{
    Query,
    linker::ComputeFunction,
    query::{
        Aggregate, Align, As, BucketBy, Cmp, DirectiveValue, Directives, Filter, GroupBy, Mapping,
        Param, Source,
    },
    types::{Dataset, Metric, Parameterized},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum VisitRes {
    Walk,
    Stop,
}

pub(crate) trait QueryVisitor {
    type Error: std::error::Error;

    fn visit(&mut self, query: &mut Query) -> Result<VisitRes, Self::Error> {
        let _ = query;
        Ok(VisitRes::Walk)
    }

    fn leave(&mut self, query: &mut Query) -> Result<(), Self::Error> {
        let _ = query;
        Ok(())
    }

    fn visit_source(&mut self, source: &mut Source) -> Result<VisitRes, Self::Error> {
        let _ = source;
        Ok(VisitRes::Walk)
    }

    fn leave_source(&mut self, source: &mut Source) -> Result<(), Self::Error> {
        let _ = source;
        Ok(())
    }

    fn visit_dataset(
        &mut self,
        dataset: &mut Parameterized<Dataset>,
    ) -> Result<VisitRes, Self::Error> {
        let _ = dataset;
        Ok(VisitRes::Walk)
    }

    fn leave_dataset(&mut self, dataset: &mut Parameterized<Dataset>) -> Result<(), Self::Error> {
        let _ = dataset;
        Ok(())
    }

    fn visit_metric(&mut self, metric: &mut Metric) -> Result<VisitRes, Self::Error> {
        let _ = metric;
        Ok(VisitRes::Walk)
    }

    fn leave_metric(&mut self, metric: &mut Metric) -> Result<(), Self::Error> {
        let _ = metric;
        Ok(())
    }

    fn visit_sample(&mut self, sample: &mut Option<f64>) -> Result<VisitRes, Self::Error> {
        let _ = sample;
        Ok(VisitRes::Walk)
    }

    fn leave_sample(&mut self, sample: &mut Option<f64>) -> Result<(), Self::Error> {
        let _ = sample;
        Ok(())
    }

    fn visit_filters(&mut self, filters: &mut Vec<Filter>) -> Result<VisitRes, Self::Error> {
        let _ = filters;
        Ok(VisitRes::Walk)
    }

    fn leave_filters(&mut self, filters: &mut Vec<Filter>) -> Result<(), Self::Error> {
        let _ = filters;
        Ok(())
    }

    fn visit_filter(&mut self, filter: &mut Filter) -> Result<VisitRes, Self::Error> {
        let _ = filter;
        Ok(VisitRes::Walk)
    }

    fn leave_filter(&mut self, filter: &mut Filter) -> Result<(), Self::Error> {
        let _ = filter;
        Ok(())
    }

    fn visit_cmp(&mut self, field: &mut String, cmp: &mut Cmp) -> Result<VisitRes, Self::Error> {
        let _ = field;
        let _ = cmp;
        Ok(VisitRes::Walk)
    }

    fn leave_cmp(&mut self, field: &mut String, cmp: &mut Cmp) -> Result<(), Self::Error> {
        let _ = field;
        let _ = cmp;
        Ok(())
    }

    fn visit_op(&mut self, op: &mut ComputeFunction) -> Result<VisitRes, Self::Error> {
        let _ = op;
        Ok(VisitRes::Walk)
    }

    fn leave_op(&mut self, op: &mut ComputeFunction) -> Result<(), Self::Error> {
        let _ = op;
        Ok(())
    }

    fn visit_aggregates(
        &mut self,
        aggregates: &mut Vec<Aggregate>,
    ) -> Result<VisitRes, Self::Error> {
        let _ = aggregates;
        Ok(VisitRes::Walk)
    }

    fn leave_aggregates(&mut self, aggregates: &mut Vec<Aggregate>) -> Result<(), Self::Error> {
        let _ = aggregates;
        Ok(())
    }

    fn visit_aggregate(&mut self, aggregate: &mut Aggregate) -> Result<VisitRes, Self::Error> {
        let _ = aggregate;
        Ok(VisitRes::Walk)
    }

    fn leave_aggregate(&mut self, aggregate: &mut Aggregate) -> Result<(), Self::Error> {
        let _ = aggregate;
        Ok(())
    }

    fn visit_mapping(&mut self, mapping: &mut Mapping) -> Result<VisitRes, Self::Error> {
        let _ = mapping;
        Ok(VisitRes::Walk)
    }

    fn leave_mapping(&mut self, mapping: &mut Mapping) -> Result<(), Self::Error> {
        let _ = mapping;
        Ok(())
    }

    fn visit_align(&mut self, align: &mut Align) -> Result<VisitRes, Self::Error> {
        let _ = align;
        Ok(VisitRes::Walk)
    }

    fn leave_align(&mut self, align: &mut Align) -> Result<(), Self::Error> {
        let _ = align;
        Ok(())
    }

    fn visit_group_by(&mut self, group_by: &mut GroupBy) -> Result<VisitRes, Self::Error> {
        let _ = group_by;
        Ok(VisitRes::Walk)
    }

    fn leave_group_by(&mut self, group_by: &mut GroupBy) -> Result<(), Self::Error> {
        let _ = group_by;
        Ok(())
    }

    fn visit_bucket_by(&mut self, bucket_by: &mut BucketBy) -> Result<VisitRes, Self::Error> {
        let _ = bucket_by;
        Ok(VisitRes::Walk)
    }

    fn leave_bucket_by(&mut self, bucket_by: &mut BucketBy) -> Result<(), Self::Error> {
        let _ = bucket_by;
        Ok(())
    }

    fn visit_as(&mut self, as_: &mut As) -> Result<VisitRes, Self::Error> {
        let _ = as_;
        Ok(VisitRes::Walk)
    }

    fn leave_as(&mut self, as_: &mut As) -> Result<(), Self::Error> {
        let _ = as_;
        Ok(())
    }

    fn visit_directives(&mut self, directives: &mut Directives) -> Result<VisitRes, Self::Error> {
        let _ = directives;
        Ok(VisitRes::Walk)
    }

    fn leave_directives(&mut self, directives: &mut Directives) -> Result<(), Self::Error> {
        let _ = directives;
        Ok(())
    }

    fn visit_directive(
        &mut self,
        name: &String,
        value: &mut DirectiveValue,
    ) -> Result<VisitRes, Self::Error> {
        let _ = name;
        let _ = value;
        Ok(VisitRes::Walk)
    }

    fn leave_directive(
        &mut self,
        name: &String,
        value: &mut DirectiveValue,
    ) -> Result<(), Self::Error> {
        let _ = name;
        let _ = value;
        Ok(())
    }

    fn visit_params(&mut self, params: &mut Vec<Param>) -> Result<VisitRes, Self::Error> {
        let _ = params;
        Ok(VisitRes::Walk)
    }

    fn leave_params(&mut self, params: &mut Vec<Param>) -> Result<(), Self::Error> {
        let _ = params;
        Ok(())
    }

    fn visit_param(&mut self, param: &mut Param) -> Result<VisitRes, Self::Error> {
        let _ = param;
        Ok(VisitRes::Walk)
    }

    fn leave_param(&mut self, param: &mut Param) -> Result<(), Self::Error> {
        let _ = param;
        Ok(())
    }
}

macro_rules! stop {
    ($e:expr, $leave_fn:expr) => {
        if $e? == VisitRes::Stop {
            return $leave_fn;
        }
    };
}

pub(crate) trait QueryWalker: QueryVisitor {
    fn walk(&mut self, query: &mut Query) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit(self, query),
            QueryVisitor::leave(self, query)
        );
        match query {
            Query::Simple {
                sample,
                source,
                filters,
                aggregates,
                directives,
                params,
            } => {
                QueryWalker::walk_source(self, source)?;
                QueryWalker::walk_sample(self, sample)?;
                QueryWalker::walk_filters(self, filters)?;
                QueryWalker::walk_aggregates(self, aggregates)?;
                QueryWalker::walk_directives(self, directives)?;
                QueryWalker::walk_params(self, params)?;
            }
            Query::Compute {
                left,
                right,
                name,
                op,
                aggregates,
                directives,
                params,
            } => {
                QueryWalker::walk(self, left)?;
                QueryWalker::walk(self, right)?;
                QueryWalker::walk_metric(self, name)?;
                QueryWalker::walk_op(self, op)?;
                QueryWalker::walk_aggregates(self, aggregates)?;
                QueryWalker::walk_directives(self, directives)?;
                QueryWalker::walk_params(self, params)?;
            }
        }

        QueryVisitor::leave(self, query)
    }

    fn walk_source(&mut self, source: &mut Source) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit_source(self, source),
            QueryVisitor::leave_source(self, source)
        );
        QueryWalker::walk_dataset(self, &mut source.metric_id.dataset)?;
        QueryWalker::walk_metric(self, &mut source.metric_id.metric)?;
        QueryVisitor::leave_source(self, source)?;
        Ok(())
    }

    fn walk_dataset(&mut self, dataset: &mut Parameterized<Dataset>) -> Result<(), Self::Error> {
        QueryVisitor::visit_dataset(self, dataset)?;
        QueryVisitor::leave_dataset(self, dataset)?;
        Ok(())
    }

    fn walk_metric(&mut self, metric: &mut Metric) -> Result<(), Self::Error> {
        QueryVisitor::visit_metric(self, metric)?;
        QueryVisitor::leave_metric(self, metric)?;
        Ok(())
    }
    fn walk_sample(&mut self, sample: &mut Option<f64>) -> Result<(), Self::Error> {
        QueryVisitor::visit_sample(self, sample)?;
        QueryVisitor::leave_sample(self, sample)?;
        Ok(())
    }

    fn walk_filters(&mut self, filters: &mut Vec<Filter>) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit_filters(self, filters),
            QueryVisitor::leave_filters(self, filters)
        );
        for filter in filters.iter_mut() {
            QueryWalker::walk_filter(self, filter)?;
        }
        QueryVisitor::leave_filters(self, filters)?;
        Ok(())
    }

    fn walk_filter(&mut self, filter: &mut Filter) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit_filter(self, filter),
            QueryVisitor::leave_filter(self, filter)
        );
        match filter {
            Filter::And(filters) | Filter::Or(filters) => {
                for filter in filters {
                    QueryWalker::walk_filter(self, filter)?;
                }
            }
            Filter::Not(filter) => QueryWalker::walk_filter(self, filter)?,
            Filter::Cmp { field, rhs } => QueryWalker::walk_cmp(self, field, rhs)?,
        }
        QueryVisitor::leave_filter(self, filter)?;
        Ok(())
    }

    fn walk_cmp(&mut self, field: &mut String, rhs: &mut Cmp) -> Result<(), Self::Error> {
        QueryVisitor::visit_cmp(self, field, rhs)?;
        QueryVisitor::leave_cmp(self, field, rhs)?;
        Ok(())
    }

    fn walk_op(&mut self, op: &mut ComputeFunction) -> Result<(), Self::Error> {
        QueryVisitor::visit_op(self, op)?;
        QueryVisitor::leave_op(self, op)?;
        Ok(())
    }

    fn walk_aggregates(&mut self, aggregates: &mut Vec<Aggregate>) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit_aggregates(self, aggregates),
            QueryVisitor::leave_aggregates(self, aggregates)
        );
        for aggregate in aggregates.iter_mut() {
            QueryWalker::walk_aggregate(self, aggregate)?;
        }
        QueryVisitor::leave_aggregates(self, aggregates)?;
        Ok(())
    }

    fn walk_aggregate(&mut self, aggregate: &mut Aggregate) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit_aggregate(self, aggregate),
            QueryVisitor::leave_aggregate(self, aggregate)
        );
        match aggregate {
            Aggregate::Map(mapping) => QueryWalker::walk_mapping(self, mapping)?,
            Aggregate::Align(align) => QueryWalker::walk_align(self, align)?,
            Aggregate::GroupBy(group_by) => QueryWalker::walk_group_by(self, group_by)?,
            Aggregate::Bucket(bucket_by) => QueryWalker::walk_bucket_by(self, bucket_by)?,
            Aggregate::As(as_) => QueryWalker::walk_as(self, as_)?,
        }
        QueryVisitor::leave_aggregate(self, aggregate)?;
        Ok(())
    }

    fn walk_mapping(&mut self, mapping: &mut Mapping) -> Result<(), Self::Error> {
        QueryVisitor::visit_mapping(self, mapping)?;
        QueryVisitor::leave_mapping(self, mapping)?;
        Ok(())
    }

    fn walk_align(&mut self, align: &mut Align) -> Result<(), Self::Error> {
        QueryVisitor::visit_align(self, align)?;
        QueryVisitor::leave_align(self, align)?;
        Ok(())
    }

    fn walk_group_by(&mut self, group_by: &mut GroupBy) -> Result<(), Self::Error> {
        QueryVisitor::visit_group_by(self, group_by)?;
        QueryVisitor::leave_group_by(self, group_by)?;
        Ok(())
    }

    fn walk_bucket_by(&mut self, bucket_by: &mut BucketBy) -> Result<(), Self::Error> {
        QueryVisitor::visit_bucket_by(self, bucket_by)?;
        QueryVisitor::leave_bucket_by(self, bucket_by)?;
        Ok(())
    }

    fn walk_as(&mut self, as_: &mut As) -> Result<(), Self::Error> {
        QueryVisitor::visit_as(self, as_)?;
        QueryVisitor::visit_metric(self, &mut as_.name)?;
        QueryVisitor::leave_as(self, as_)?;
        Ok(())
    }

    fn walk_directives(&mut self, directives: &mut Directives) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit_directives(self, directives),
            QueryVisitor::leave_directives(self, directives)
        );
        for (name, value) in directives.iter_mut() {
            QueryWalker::walk_directive(self, name, value)?;
        }
        QueryVisitor::leave_directives(self, directives)?;
        Ok(())
    }

    fn walk_directive(
        &mut self,
        name: &String,
        value: &mut DirectiveValue,
    ) -> Result<(), Self::Error> {
        QueryVisitor::visit_directive(self, name, value)?;
        QueryVisitor::leave_directive(self, name, value)?;
        Ok(())
    }

    fn walk_params(&mut self, params: &mut Vec<Param>) -> Result<(), Self::Error> {
        stop!(
            QueryVisitor::visit_params(self, params),
            QueryVisitor::leave_params(self, params)
        );
        for param in params.iter_mut() {
            QueryWalker::walk_param(self, param)?;
        }
        QueryVisitor::leave_params(self, params)?;
        Ok(())
    }

    fn walk_param(&mut self, param: &mut Param) -> Result<(), Self::Error> {
        QueryVisitor::visit_param(self, param)?;
        QueryVisitor::leave_param(self, param)?;
        Ok(())
    }
}
