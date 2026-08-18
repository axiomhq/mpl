//! Tests for lowering the AST to a `Query`.
//!
//! A child module rather than a file under `tests/`, because the cases read the `ParseError`
//! variants `lower` produces. Going through `compile2` would fold them into a `CompileError`
//! and run the typecheck visitors on top, so a case could no longer say which stage rejected
//! the query.
use std::collections::HashMap;

use test_case::test_case;

use super::*;

/// Runs the two stages `compile2` runs before its visitors, so a case states a query and reads
/// the errors a user would be shown.
fn lower_with(
    src: &str,
    system_params: HashMap<String, ParamType>,
) -> Result<(crate::Query, Vec<Warning>), Vec<ParseError>> {
    Parser::new(ast::Parser::new(src).lower()).lower(system_params)
}

fn lower(src: &str) -> Result<(crate::Query, Vec<Warning>), Vec<ParseError>> {
    lower_with(src, HashMap::new())
}

/// Lowers `src` and hands back its errors, printing the accepted query when there are none so
/// a regression names what got through.
fn errors_of(src: &str) -> Vec<ParseError> {
    match lower(src) {
        Ok((query, _)) => panic!("`{src}` lowered cleanly to {query:?}"),
        Err(errors) => errors,
    }
}

/// The time an `align` or `bucket` clause carries, so a case can name the value without walking
/// the aggregate list itself.
fn aggregate_time(query: &crate::Query) -> Option<&Parameterized<RelativeTime>> {
    let crate::Query::Simple { aggregates, .. } = query else {
        return None;
    };
    aggregates.iter().find_map(|a| match a {
        Aggregate::Align(align) => align.time.as_ref(),
        Aggregate::Bucket(bucket) => bucket.time.as_ref(),
        _ => None,
    })
}

/// The AST records an error and drops the construct it belongs to. `lower` owns passing that
/// error on: a query that reaches the backend with a construct missing runs something the user
/// did not write.
#[test]
fn an_invalid_regex_in_a_conjunct_is_reported() {
    let src = r#"d:m | where a == #/[/ and b == "y""#;
    let errors = errors_of(src);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ParseError::AST(AstError::InvalidRegex { .. }))),
        "`{src}`: expected an invalid regex error, got {errors:?}"
    );
}

/// Directive values are literals. An identifier is rejected, and the rejection has to reach the
/// caller because a dropped `set` changes how the result is rendered.
#[test]
fn a_directive_value_that_is_not_a_literal_is_reported() {
    let src = "set unit = bytes; d:m";
    let errors = errors_of(src);
    assert!(!errors.is_empty(), "`{src}`: expected an error");
}

/// `TimeType::Avg` declares no arguments, so `avg(1)` is an arity error rather than an argument
/// the lowering may drop.
#[test]
fn align_rejects_an_argument_its_function_does_not_take() {
    let src = "a:b | align using avg(1)";
    let errors = errors_of(src);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ParseError::InvalidArgumentCount { .. })),
        "`{src}`: expected an argument count error, got {errors:?}"
    );
}

/// `TagsType::Sum` declares no arguments; the tags come from the `by` list.
#[test]
fn group_by_rejects_an_argument_its_function_does_not_take() {
    let src = "a:b | group by c using sum(9)";
    let errors = errors_of(src);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ParseError::InvalidArgumentCount { .. })),
        "`{src}`: expected an argument count error, got {errors:?}"
    );
}

/// `BucketType::args` declares the spec list as `Repeated { min: 1 }`: a bucket aggregate with
/// no spec has no bucket to produce.
#[test_case("a:b | bucket using histogram" ; "no parens")]
#[test_case("a:b | bucket using histogram()" ; "empty parens")]
fn bucket_requires_at_least_one_spec(src: &str) {
    let errors = errors_of(src);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ParseError::MissingBucketSpec { .. })),
        "`{src}`: expected a missing bucket spec error, got {errors:?}"
    );
}

/// A duration keeps the unit it was written in, or the user is told it could not. Rounding
/// `1500ms` down to `1s` is a 1.5x wider window than the query asked for.
#[test_case("d:m | align to 1500ms using avg", 1500 ; "align")]
#[test_case("d:m | bucket to 1500ms using histogram(0.5)", 1500 ; "bucket")]
fn a_sub_second_duration_survives_or_is_reported(src: &str, millis: u64) {
    let Ok((query, warnings)) = lower(src) else {
        // Rejecting the duration also tells the user; what this pins is the silent rounding.
        return;
    };
    let time = aggregate_time(&query).unwrap_or_else(|| panic!("`{src}` carries a time"));
    let kept = matches!(time, Parameterized::Concrete(t) if t.value == millis && t.unit == TimeUnit::Millisecond);
    if !kept {
        warnings.first().unwrap_or_else(|| {
            panic!(
                "`{src}` lowered to {time:?} and warned about nothing.\n\
                 NOTE: saying so needs a warning `parser2` raises itself. `Warning` carries one \
                 variant, `Ast(AstWarning)`, and the AST raises `TimeNotSecondAligned` for \
                 `align` only, so `bucket` has nothing to report with."
            )
        });
    }
}

/// `get_param_decl` resolves a name by taking the first declaration that matches, so a second
/// declaration of the same name has to be reported rather than kept and ignored.
#[test]
fn a_parameter_declared_twice_is_reported() {
    let src = "param $x: string; param $x: string; a:b";
    let errors = errors_of(src);
    assert!(!errors.is_empty(), "`{src}`: expected an error");
}

/// The other half of the reserved prefix: a query may declare `$__foo`, and is warned that the
/// host can shadow it.
#[test]
fn a_user_parameter_using_the_reserved_prefix_warns() {
    let src = "param $__foo: string; a:b";
    let (_, warnings) = lower(src).unwrap_or_else(|e| panic!("`{src}` did not lower: {e:?}"));
    warnings
        .first()
        .unwrap_or_else(|| panic!("`{src}` warned about nothing."));
}

/// A query samples at one rate. Two `sample` clauses have to be reported, because taking either
/// one silently returns a different amount of data than the query reads as asking for.
#[test]
fn a_second_sample_clause_is_reported() {
    let src = "d:m | sample 0.1 | sample 0.9";
    let errors = errors_of(src);
    assert!(!errors.is_empty(), "`{src}`: expected an error");
}

/// `interpolate_cumulative_histogram` takes the conversion mode first and the specs after it, so
/// the spec numbering an error reports counts from the start of the argument list.
#[test]
fn cumulative_histogram_numbers_specs_after_the_conversion_mode() {
    let src = r#"a:b | bucket using interpolate_cumulative_histogram(rate, "x")"#;
    let errors = errors_of(src);
    let n = errors
        .iter()
        .find_map(|e| match e {
            ParseError::InvalidArgumentType { n, .. } => Some(*n),
            _ => None,
        })
        .unwrap_or_else(|| panic!("`{src}`: expected an argument type error, got {errors:?}"));
    assert_eq!(n, 2, "`{src}`: the string is the second argument");
}

/// `MapType::Min` declares one required `Float`, the value each datapoint is clamped to, so
/// `map min` is missing an argument.
#[test]
fn map_min_without_its_argument_is_reported() {
    let src = "a:b | map min";
    let errors = errors_of(src);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ParseError::InvalidArgumentCount { .. })),
        "`{src}`: expected an argument count error, got {errors:?}"
    );
}
