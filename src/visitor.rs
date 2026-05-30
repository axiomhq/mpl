//! Visitor pattern for query

use crate::{
    Query,
    enc_regex::EncodableRegex,
    linker::ComputeFunction,
    query::{
        Aggregate, Align, As, BucketBy, Cmp, DirectiveValue, Directives, Expr, Filter,
        FilterOrIfDef, GroupBy, Mapping, ParamDeclaration, Source, TagExtend,
    },
    types::{Dataset, Metric, Parameterized},
};

/// Result of a visit operation.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VisitRes {
    /// Continue after the visit hook, walking this node's children and leave hook.
    Walk,
    /// Stop after the visit hook, skipping this node's children and leave hook.
    Stop,
}

impl VisitRes {
    fn should_walk(self) -> bool {
        matches!(self, VisitRes::Walk)
    }
}

/// Visitor for all query components.
pub trait QueryVisitor {
    /// Error type for the visitor.
    type Error: std::error::Error;

    /// Visit a query.
    fn visit(&mut self, query: &mut Query) -> Result<VisitRes, Self::Error> {
        let _ = query;
        Ok(VisitRes::Walk)
    }

    /// Leave a query.
    fn leave(&mut self, query: &mut Query) -> Result<(), Self::Error> {
        let _ = query;
        Ok(())
    }

    /// Visit a source.
    fn visit_source(&mut self, source: &mut Source) -> Result<VisitRes, Self::Error> {
        let _ = source;
        Ok(VisitRes::Walk)
    }

    /// Leave a source.
    fn leave_source(&mut self, source: &mut Source) -> Result<(), Self::Error> {
        let _ = source;
        Ok(())
    }

    /// Visit a dataset.
    fn visit_dataset(
        &mut self,
        dataset: &mut Parameterized<Dataset>,
    ) -> Result<VisitRes, Self::Error> {
        let _ = dataset;
        Ok(VisitRes::Walk)
    }

    /// Leave a dataset.
    fn leave_dataset(&mut self, dataset: &mut Parameterized<Dataset>) -> Result<(), Self::Error> {
        let _ = dataset;
        Ok(())
    }

    /// Visit a metric.
    fn visit_metric(&mut self, metric: &mut Metric) -> Result<VisitRes, Self::Error> {
        let _ = metric;
        Ok(VisitRes::Walk)
    }

    /// Leave a metric.
    fn leave_metric(&mut self, metric: &mut Metric) -> Result<(), Self::Error> {
        let _ = metric;
        Ok(())
    }

    /// Visit a sample.
    fn visit_sample(&mut self, sample: &mut Option<f64>) -> Result<VisitRes, Self::Error> {
        let _ = sample;
        Ok(VisitRes::Walk)
    }

    /// Leave a sample.
    fn leave_sample(&mut self, sample: &mut Option<f64>) -> Result<(), Self::Error> {
        let _ = sample;
        Ok(())
    }

    /// Visit filters.
    fn visit_filters(&mut self, filters: &mut Vec<FilterOrIfDef>) -> Result<VisitRes, Self::Error> {
        let _ = filters;
        Ok(VisitRes::Walk)
    }

    /// Leave filters.
    fn leave_filters(&mut self, filters: &mut Vec<FilterOrIfDef>) -> Result<(), Self::Error> {
        let _ = filters;
        Ok(())
    }

    /// Visit a filter or ifdef.
    fn visit_filter_or_ifdef(
        &mut self,
        filter: &mut FilterOrIfDef,
    ) -> Result<VisitRes, Self::Error> {
        let _ = filter;
        Ok(VisitRes::Walk)
    }

    /// Leave a filter or ifdef.
    fn leave_filter_or_ifdef(&mut self, filter: &mut FilterOrIfDef) -> Result<(), Self::Error> {
        let _ = filter;
        Ok(())
    }

    /// Visit a filter.
    fn visit_filter(&mut self, filter: &mut Filter) -> Result<VisitRes, Self::Error> {
        let _ = filter;
        Ok(VisitRes::Walk)
    }

    /// Leave a filter.
    fn leave_filter(&mut self, filter: &mut Filter) -> Result<(), Self::Error> {
        let _ = filter;
        Ok(())
    }

    /// Visit an ifdef.
    fn visit_ifdef(
        &mut self,
        param: &mut ParamDeclaration,
        filter: &mut Filter,
        else_filter: &mut Option<Filter>,
    ) -> Result<VisitRes, Self::Error> {
        let _ = filter;
        let _ = param;
        let _ = else_filter;
        Ok(VisitRes::Walk)
    }

    /// Leave an ifdef.
    fn leave_ifdef(
        &mut self,
        param: &mut ParamDeclaration,
        filter: &mut Filter,
        else_filter: &mut Option<Filter>,
    ) -> Result<(), Self::Error> {
        let _ = filter;
        let _ = param;
        let _ = else_filter;
        Ok(())
    }

    /// Visit a comparison.
    fn visit_cmp(&mut self, field: &mut String, cmp: &mut Cmp) -> Result<VisitRes, Self::Error> {
        let _ = field;
        let _ = cmp;
        Ok(VisitRes::Walk)
    }

    /// Leave a comparison.
    fn leave_cmp(&mut self, field: &mut String, cmp: &mut Cmp) -> Result<(), Self::Error> {
        let _ = field;
        let _ = cmp;
        Ok(())
    }

    /// Visit a parameterized value.
    fn visit_expr(&mut self, value: &mut Expr) -> Result<VisitRes, Self::Error> {
        let _ = value;
        Ok(VisitRes::Walk)
    }

    /// Leave a parameterized value.
    fn leave_expr(&mut self, value: &mut Expr) -> Result<(), Self::Error> {
        let _ = value;
        Ok(())
    }

    /// Visit a parameterized regex.
    fn visit_parameterized_regex(
        &mut self,
        regex: &mut Parameterized<EncodableRegex>,
    ) -> Result<VisitRes, Self::Error> {
        let _ = regex;
        Ok(VisitRes::Walk)
    }

    /// Leave a parameterized regex.
    fn leave_parameterized_regex(
        &mut self,
        regex: &mut Parameterized<EncodableRegex>,
    ) -> Result<(), Self::Error> {
        let _ = regex;
        Ok(())
    }

    /// Visit an operation.
    fn visit_op(&mut self, op: &mut ComputeFunction) -> Result<VisitRes, Self::Error> {
        let _ = op;
        Ok(VisitRes::Walk)
    }

    /// Leave an operation.
    fn leave_op(&mut self, op: &mut ComputeFunction) -> Result<(), Self::Error> {
        let _ = op;
        Ok(())
    }

    /// Visit aggregates.
    fn visit_aggregates(
        &mut self,
        aggregates: &mut Vec<Aggregate>,
    ) -> Result<VisitRes, Self::Error> {
        let _ = aggregates;
        Ok(VisitRes::Walk)
    }

    /// Leave aggregates.
    fn leave_aggregates(&mut self, aggregates: &mut Vec<Aggregate>) -> Result<(), Self::Error> {
        let _ = aggregates;
        Ok(())
    }

    /// Visit an aggregate.
    fn visit_aggregate(&mut self, aggregate: &mut Aggregate) -> Result<VisitRes, Self::Error> {
        let _ = aggregate;
        Ok(VisitRes::Walk)
    }

    /// Leave an aggregate.
    fn leave_aggregate(&mut self, aggregate: &mut Aggregate) -> Result<(), Self::Error> {
        let _ = aggregate;
        Ok(())
    }

    /// Visit a mapping.
    fn visit_mapping(&mut self, mapping: &mut Mapping) -> Result<VisitRes, Self::Error> {
        let _ = mapping;
        Ok(VisitRes::Walk)
    }

    /// Leave a mapping.
    fn leave_mapping(&mut self, mapping: &mut Mapping) -> Result<(), Self::Error> {
        let _ = mapping;
        Ok(())
    }

    /// Visit an align.
    fn visit_align(&mut self, align: &mut Align) -> Result<VisitRes, Self::Error> {
        let _ = align;
        Ok(VisitRes::Walk)
    }

    /// Leave an align.
    fn leave_align(&mut self, align: &mut Align) -> Result<(), Self::Error> {
        let _ = align;
        Ok(())
    }

    /// Visit a group by.
    fn visit_group_by(&mut self, group_by: &mut GroupBy) -> Result<VisitRes, Self::Error> {
        let _ = group_by;
        Ok(VisitRes::Walk)
    }

    /// Leave a group by.
    fn leave_group_by(&mut self, group_by: &mut GroupBy) -> Result<(), Self::Error> {
        let _ = group_by;
        Ok(())
    }

    /// Visit a bucket by.
    fn visit_bucket_by(&mut self, bucket_by: &mut BucketBy) -> Result<VisitRes, Self::Error> {
        let _ = bucket_by;
        Ok(VisitRes::Walk)
    }

    /// Leave a bucket by.
    fn leave_bucket_by(&mut self, bucket_by: &mut BucketBy) -> Result<(), Self::Error> {
        let _ = bucket_by;
        Ok(())
    }

    /// Visit an as.
    fn visit_as(&mut self, as_: &mut As) -> Result<VisitRes, Self::Error> {
        let _ = as_;
        Ok(VisitRes::Walk)
    }

    /// Leave an as.
    fn leave_as(&mut self, as_: &mut As) -> Result<(), Self::Error> {
        let _ = as_;
        Ok(())
    }

    /// Visit directives.
    fn visit_directives(&mut self, directives: &mut Directives) -> Result<VisitRes, Self::Error> {
        let _ = directives;
        Ok(VisitRes::Walk)
    }

    /// Leave directives.
    fn leave_directives(&mut self, directives: &mut Directives) -> Result<(), Self::Error> {
        let _ = directives;
        Ok(())
    }

    /// Visit a directive.
    fn visit_directive(
        &mut self,
        name: &String,
        value: &mut DirectiveValue,
    ) -> Result<VisitRes, Self::Error> {
        let _ = name;
        let _ = value;
        Ok(VisitRes::Walk)
    }

    /// Leave a directive.
    fn leave_directive(
        &mut self,
        name: &String,
        value: &mut DirectiveValue,
    ) -> Result<(), Self::Error> {
        let _ = name;
        let _ = value;
        Ok(())
    }

    /// Visit params.
    fn visit_params(
        &mut self,
        params: &mut Vec<ParamDeclaration>,
    ) -> Result<VisitRes, Self::Error> {
        let _ = params;
        Ok(VisitRes::Walk)
    }

    /// Leave params.
    fn leave_params(&mut self, params: &mut Vec<ParamDeclaration>) -> Result<(), Self::Error> {
        let _ = params;
        Ok(())
    }

    /// Visit a param.
    fn visit_param(&mut self, param: &mut ParamDeclaration) -> Result<VisitRes, Self::Error> {
        let _ = param;
        Ok(VisitRes::Walk)
    }

    /// Leave a param.
    fn leave_param(&mut self, param: &mut ParamDeclaration) -> Result<(), Self::Error> {
        let _ = param;
        Ok(())
    }

    /// Visit params.
    fn visit_extends(&mut self, extends: &mut Vec<TagExtend>) -> Result<VisitRes, Self::Error> {
        let _ = extends;
        Ok(VisitRes::Walk)
    }

    /// Leave extends.
    fn leave_extends(&mut self, extends: &mut Vec<TagExtend>) -> Result<(), Self::Error> {
        let _ = extends;
        Ok(())
    }

    /// Visit a extend.
    fn visit_extend(&mut self, extend: &mut TagExtend) -> Result<VisitRes, Self::Error> {
        let _ = extend;
        Ok(VisitRes::Walk)
    }

    /// Leave a extend.
    fn leave_extend(&mut self, extend: &mut TagExtend) -> Result<(), Self::Error> {
        let _ = extend;
        Ok(())
    }
}

/// A trait for walking a query.
pub trait QueryWalker: QueryVisitor {
    /// Walk a query.
    fn walk(&mut self, query: &mut Query) -> Result<(), Self::Error> {
        if !QueryVisitor::visit(self, query)?.should_walk() {
            return Ok(());
        }
        match query {
            Query::Simple {
                sample,
                source,
                filters,
                aggregates,
                directives,
                params,
                extends,
            } => {
                QueryWalker::walk_source(self, source)?;
                QueryWalker::walk_sample(self, sample)?;
                QueryWalker::walk_filters(self, filters)?;
                QueryWalker::walk_aggregates(self, aggregates)?;
                QueryWalker::walk_directives(self, directives)?;
                QueryWalker::walk_params(self, params)?;
                QueryWalker::walk_extends(self, extends)?;
            }
            Query::Compute {
                left,
                right,
                name,
                op,
                aggregates,
                directives,
                params,
                extends,
            } => {
                QueryWalker::walk(self, left)?;
                QueryWalker::walk(self, right)?;
                QueryWalker::walk_metric(self, name)?;
                QueryWalker::walk_op(self, op)?;
                QueryWalker::walk_aggregates(self, aggregates)?;
                QueryWalker::walk_directives(self, directives)?;
                QueryWalker::walk_params(self, params)?;
                QueryWalker::walk_extends(self, extends)?;
            }
        }

        QueryVisitor::leave(self, query)
    }

    /// Walk a source.
    fn walk_source(&mut self, source: &mut Source) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_source(self, source)?.should_walk() {
            return Ok(());
        }
        QueryWalker::walk_dataset(self, &mut source.metric_id.dataset)?;
        QueryWalker::walk_metric(self, &mut source.metric_id.metric)?;
        QueryVisitor::leave_source(self, source)
    }

    /// Walk a dataset.
    fn walk_dataset(&mut self, dataset: &mut Parameterized<Dataset>) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_dataset(self, dataset)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_dataset(self, dataset)
    }

    /// Walk a metric.
    fn walk_metric(&mut self, metric: &mut Metric) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_metric(self, metric)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_metric(self, metric)
    }

    /// Walk a sample.
    fn walk_sample(&mut self, sample: &mut Option<f64>) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_sample(self, sample)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_sample(self, sample)
    }

    /// Walk filters.
    fn walk_filters(&mut self, filters: &mut Vec<FilterOrIfDef>) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_filters(self, filters)?.should_walk() {
            return Ok(());
        }
        for filter in filters.iter_mut() {
            QueryWalker::walk_filter_or_ifdef(self, filter)?;
        }
        QueryVisitor::leave_filters(self, filters)
    }

    /// Walk a filter or ifdef.
    fn walk_filter_or_ifdef(&mut self, filter: &mut FilterOrIfDef) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_filter_or_ifdef(self, filter)?.should_walk() {
            return Ok(());
        }
        match filter {
            FilterOrIfDef::Filter(filter) => QueryWalker::walk_filter(self, filter)?,
            FilterOrIfDef::Ifdef {
                param,
                filter,
                else_filter,
            } => QueryWalker::walk_ifdef(self, param, filter, else_filter)?,
        }
        QueryVisitor::leave_filter_or_ifdef(self, filter)
    }

    /// Walk an ifdef.
    fn walk_ifdef(
        &mut self,
        param: &mut ParamDeclaration,
        filter: &mut Filter,
        else_filter: &mut Option<Filter>,
    ) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_ifdef(self, param, filter, else_filter)?.should_walk() {
            return Ok(());
        }
        QueryWalker::walk_param(self, param)?;
        QueryWalker::walk_filter(self, filter)?;
        if let Some(else_filter) = else_filter {
            QueryWalker::walk_filter(self, else_filter)?;
        }
        QueryVisitor::leave_ifdef(self, param, filter, else_filter)
    }

    /// Walk a filter.
    fn walk_filter(&mut self, filter: &mut Filter) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_filter(self, filter)?.should_walk() {
            return Ok(());
        }
        match filter {
            Filter::And(filters) | Filter::Or(filters) => {
                for filter in filters {
                    QueryWalker::walk_filter(self, filter)?;
                }
            }
            Filter::Not(filter) => QueryWalker::walk_filter(self, filter)?,
            Filter::Cmp { field, rhs } => QueryWalker::walk_cmp(self, field, rhs)?,
        }
        QueryVisitor::leave_filter(self, filter)
    }

    /// Walk a cmp.
    fn walk_cmp(&mut self, field: &mut String, rhs: &mut Cmp) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_cmp(self, field, rhs)?.should_walk() {
            return Ok(());
        }
        match rhs {
            Cmp::Eq(parameterized)
            | Cmp::Ne(parameterized)
            | Cmp::Gt(parameterized)
            | Cmp::Ge(parameterized)
            | Cmp::Lt(parameterized)
            | Cmp::Le(parameterized) => {
                QueryWalker::walk_expr(self, parameterized)?;
            }
            Cmp::RegEx(parameterized) | Cmp::RegExNot(parameterized) => {
                QueryWalker::walk_parameterized_regex(self, parameterized)?;
            }
            Cmp::Is(_tag_type) => (),
        }
        QueryVisitor::leave_cmp(self, field, rhs)
    }
    /// Walks a parameterized value
    fn walk_expr(&mut self, value: &mut Expr) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_expr(self, value)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_expr(self, value)
    }
    /// Walks a parameterized regex
    fn walk_parameterized_regex(
        &mut self,
        regex: &mut Parameterized<EncodableRegex>,
    ) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_parameterized_regex(self, regex)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_parameterized_regex(self, regex)
    }

    /// Walk an op.
    fn walk_op(&mut self, op: &mut ComputeFunction) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_op(self, op)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_op(self, op)
    }

    /// Walk aggregates.
    fn walk_aggregates(&mut self, aggregates: &mut Vec<Aggregate>) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_aggregates(self, aggregates)?.should_walk() {
            return Ok(());
        }
        for aggregate in aggregates.iter_mut() {
            QueryWalker::walk_aggregate(self, aggregate)?;
        }
        QueryVisitor::leave_aggregates(self, aggregates)
    }

    /// Walk an aggregate.
    fn walk_aggregate(&mut self, aggregate: &mut Aggregate) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_aggregate(self, aggregate)?.should_walk() {
            return Ok(());
        }
        match aggregate {
            Aggregate::Map(mapping) => QueryWalker::walk_mapping(self, mapping)?,
            Aggregate::Align(align) => QueryWalker::walk_align(self, align)?,
            Aggregate::GroupBy(group_by) => QueryWalker::walk_group_by(self, group_by)?,
            Aggregate::Bucket(bucket_by) => QueryWalker::walk_bucket_by(self, bucket_by)?,
            Aggregate::As(as_) => QueryWalker::walk_as(self, as_)?,
        }
        QueryVisitor::leave_aggregate(self, aggregate)
    }

    /// Walk a mapping.
    fn walk_mapping(&mut self, mapping: &mut Mapping) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_mapping(self, mapping)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_mapping(self, mapping)
    }

    /// Walk an align.
    fn walk_align(&mut self, align: &mut Align) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_align(self, align)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_align(self, align)
    }

    /// Walk a group by.
    fn walk_group_by(&mut self, group_by: &mut GroupBy) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_group_by(self, group_by)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_group_by(self, group_by)
    }

    /// Walk a bucket by.
    fn walk_bucket_by(&mut self, bucket_by: &mut BucketBy) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_bucket_by(self, bucket_by)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_bucket_by(self, bucket_by)
    }

    /// Walk an as.
    fn walk_as(&mut self, as_: &mut As) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_as(self, as_)?.should_walk() {
            return Ok(());
        }
        QueryWalker::walk_metric(self, &mut as_.name)?;
        QueryVisitor::leave_as(self, as_)
    }

    /// Walk directives.
    fn walk_directives(&mut self, directives: &mut Directives) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_directives(self, directives)?.should_walk() {
            return Ok(());
        }
        for (name, value) in directives.iter_mut() {
            QueryWalker::walk_directive(self, name, value)?;
        }
        QueryVisitor::leave_directives(self, directives)
    }

    /// Walk a directive.
    fn walk_directive(
        &mut self,
        name: &String,
        value: &mut DirectiveValue,
    ) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_directive(self, name, value)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_directive(self, name, value)
    }

    /// Walk params.
    fn walk_params(&mut self, params: &mut Vec<ParamDeclaration>) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_params(self, params)?.should_walk() {
            return Ok(());
        }
        for param in params.iter_mut() {
            QueryWalker::walk_param(self, param)?;
        }
        QueryVisitor::leave_params(self, params)
    }

    /// Walk a param.
    fn walk_param(&mut self, param: &mut ParamDeclaration) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_param(self, param)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_param(self, param)
    }

    /// Walk params.
    fn walk_extends(&mut self, extends: &mut Vec<TagExtend>) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_extends(self, extends)?.should_walk() {
            return Ok(());
        }
        for param in extends.iter_mut() {
            QueryWalker::walk_extend(self, param)?;
        }
        QueryVisitor::leave_extends(self, extends)
    }

    /// Walk a param.
    fn walk_extend(&mut self, extend: &mut TagExtend) -> Result<(), Self::Error> {
        if !QueryVisitor::visit_extend(self, extend)?.should_walk() {
            return Ok(());
        }
        QueryVisitor::leave_extend(self, extend)
    }
}
