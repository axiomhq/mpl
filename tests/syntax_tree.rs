//! Syntax-tree tests for the MPL parser.
//!
//! The parser builds a lossless `rowan` CST: every byte of the input — including whitespace
//! and comments — ends up in the tree, and the tree's structure is what tools (the language
//! server, the formatter) read. Those are two independent claims, so the tests come in two
//! layers.
//!
//! The **tables** are written as `source => tree shape`, where the shape is a one-line
//! s-expression: nodes render as `KIND(children)` and tokens as their own source text, with
//! trivia dropped. Whole-tree shapes get long, so most tables render one extracted subtree
//! (`rule`, `cmp`, `konst`, …) and let the surrounding query stay implicit. Every table row
//! also asserts that the parse produced no errors and that the tree reproduces the source
//! byte for byte, so the shape is never read off a tree that quietly dropped input.
//!
//! The **properties** at the bottom run over the shipped examples, an adversarial corpus and
//! randomly generated valid queries. They check the relationships a table cannot state:
//! losslessness, that no lexer token goes missing even when the parse fails, that every
//! node's text really is the input at its range, that node and token kinds never mix, and
//! that no input — truncated, malformed or random — panics or hangs the parser.

use std::fs;

use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource, Report};
use mpl_lang::lexer::Lexer;
use mpl_lang::syntax_tree::{Lang, Parser, SyntaxError, SyntaxKind, SyntaxNode};
use rowan::{Language, NodeOrToken};
use test_case::test_case;

// ---------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------

/// Trivia is dropped from every rendering: it is covered byte-exactly by the round-trip
/// property, and leaving it in would put a `" "` between every pair of table cells.
fn is_trivia(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::LX_WHITESPACE | SyntaxKind::LX_COMMENT)
}

/// Renders a subtree as `KIND(child child …)`, tokens as their source text.
///
/// Tokens keep their text rather than their kind because the kind is already pinned by
/// `tests/lexer.rs`; what these tests are about is which node a token landed in.
fn shape(node: &SyntaxNode) -> String {
    let parts = node
        .children_with_tokens()
        .filter_map(|child| match child {
            NodeOrToken::Node(n) => Some(shape(&n)),
            NodeOrToken::Token(t) => (!is_trivia(t.kind())).then(|| t.text().to_string()),
        })
        .collect::<Vec<_>>();
    format!("{:?}({})", node.kind(), parts.join(" "))
}

/// Parses, and fails the test unless the parse was clean and lossless.
///
/// Both checks belong here rather than in a separate test: a shape read off a tree that
/// dropped a token looks perfectly well-formed, so a table row that does not assert
/// losslessness can pass while the parser is eating input.
fn parse_clean(src: &str) -> SyntaxNode {
    let (tree, errors) = Parser::new(src).parse();
    assert!(
        errors.is_empty(),
        "unexpected errors for {src:?}: {errors:?}"
    );
    assert_eq!(tree.to_string(), src, "tree does not reproduce {src:?}");
    tree
}

/// Shape of the whole tree.
fn tree(src: &str) -> String {
    shape(&parse_clean(src))
}

/// Shape of the first node of `kind`, so a table can talk about one production without
/// re-stating the query that has to surround it.
fn first(src: &str, kind: SyntaxKind) -> String {
    let tree = parse_clean(src);
    let node = tree
        .descendants()
        .find(|n| n.kind() == kind)
        .unwrap_or_else(|| panic!("no {kind:?} node in {src:?}"));
    shape(&node)
}

/// Shape of a pipeline rule, e.g. `sample 0.5`.
fn rule(src: &str) -> String {
    first(&format!("d:m | {src}"), SyntaxKind::RULE)
}

/// Shape of a single comparison, e.g. `a == 1`.
fn cmp(src: &str) -> String {
    first(&format!("d:m | where {src}"), SyntaxKind::FILTER_CMP)
}

/// Shape of a constant in value position, e.g. `-1.5`.
fn konst(src: &str) -> String {
    first(&format!("set a = {src}; d:m"), SyntaxKind::CONST)
}

/// Shape of an array literal. `where … in` is the only position an array is reachable from
/// today — see `arrays_parse_in_constant_position` at the bottom of this file.
fn array(src: &str) -> String {
    first(&format!("d:m | where a in {src}"), SyntaxKind::ARRAY)
}

/// Shape of a duration, e.g. `7d`.
fn duration(src: &str) -> String {
    first(
        &format!("d:m | align to {src} using avg"),
        SyntaxKind::DURATION,
    )
}

/// Renders the boolean structure of a filter and nothing else: each comparison collapses to
/// the tag it tests, and a wrapper node that has exactly one operand disappears.
///
/// `filter_or` / `filter_and` / `filter_not` / `filter_paren` are entered unconditionally,
/// so the full shape of even `a == 1` is five nested nodes and the grouping — the thing
/// precedence tests are actually about — is invisible in the noise. Collapsing single-child
/// wrappers is safe precisely because such a wrapper carries no grouping information; the
/// unconditional chain itself is pinned in full by `filter_wrapper_chain`.
fn groups(node: &SyntaxNode) -> String {
    let operands = || {
        node.children()
            .filter(|c| c.kind() != SyntaxKind::KEYWORD)
            .collect::<Vec<_>>()
    };
    let joined = |label: &str, nodes: &[SyntaxNode]| {
        let inner = nodes.iter().map(groups).collect::<Vec<_>>().join(" ");
        format!("{label}({inner})")
    };
    match node.kind() {
        SyntaxKind::FILTER_OR | SyntaxKind::FILTER_AND => {
            let operands = operands();
            let label = if node.kind() == SyntaxKind::FILTER_OR {
                "OR"
            } else {
                "AND"
            };
            match operands.as_slice() {
                [only] => groups(only),
                many => joined(label, many),
            }
        }
        // A `not` shows up as a KEYWORD child; without one the node is pure scaffolding.
        SyntaxKind::FILTER_NOT => {
            let operands = operands();
            // A KEYWORD node absorbs the trivia that follows it, so its text is `"not "`.
            let negated = node
                .children()
                .any(|c| c.kind() == SyntaxKind::KEYWORD && c.text().to_string().trim() == "not");
            match (negated, operands.as_slice()) {
                (false, [only]) => groups(only),
                (_, many) => joined("NOT", many),
            }
        }
        // Parentheses need no marker of their own: they show up as nesting in the output,
        // which is exactly the difference a precedence test is looking for.
        SyntaxKind::FILTER_PAREN => operands().first().map_or_else(|| "?".to_string(), groups),
        SyntaxKind::FILTER_CMP => node
            .descendants()
            .find(|n| n.kind() == SyntaxKind::IDENT)
            .map_or_else(|| "?".to_string(), |n| n.text().to_string().trim().into()),
        other => format!("{other:?}"),
    }
}

/// Boolean structure of a filter expression, e.g. `a == 1 or b == 2 and c == 3`.
fn grouping(src: &str) -> String {
    let src = format!("d:m | where {src}");
    let tree = parse_clean(&src);
    let node = tree
        .descendants()
        .find(|n| n.kind() == SyntaxKind::FILTER_OR)
        .expect("no FILTER_OR node");
    groups(&node)
}

/// Renders one error as `<what>@<offset>+<len>`, so a table pins both the diagnostic and
/// where it points. `Eof` carries no span.
fn describe_error(error: &SyntaxError) -> String {
    match error {
        SyntaxError::Eof { .. } => "Eof".to_string(),
        SyntaxError::TokenAfterEoq { kind, range } => {
            format!("TokenAfterEoq({kind:?})@{}+{}", range.offset(), range.len())
        }
        SyntaxError::Generic { message, range } => {
            format!("{message:?}@{}+{}", range.offset(), range.len())
        }
    }
}

/// All errors for an input, in the order the parser reported them.
fn errors(src: &str) -> String {
    let (_tree, errors) = Parser::new(src).parse();
    errors
        .iter()
        .map(describe_error)
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------------------
// Queries
//
// A query is either a dataset/metric pair or a pair of sub-queries combined by `compute`.
// The dataset side accepts an escaped ident or a variable, which is what makes
// `IDENT_OR_VARIABLE` a node of its own rather than a bare `IDENT`.
// ---------------------------------------------------------------------------------------

#[test_case(
    "d:m"
    => "ROOT(QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(d)) : IDENT(m))))"
    ; "dataset and metric"
)]
#[test_case(
    "`a b`:m"
    => "ROOT(QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(`a b`)) : IDENT(m))))"
    ; "escaped dataset"
)]
#[test_case(
    "$ds:m"
    => "ROOT(QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(VARIABLE($ds)) : IDENT(m))))"
    ; "dataset from a parameter"
)]
#[test_case(
    "$`a b`:m"
    => "ROOT(QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(VARIABLE($`a b`)) : IDENT(m))))"
    ; "escaped variable dataset"
)]
#[test_case(
    "d:m as x"
    => "ROOT(QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(d)) : IDENT(m) KEYWORD(as) IDENT(x))))"
    ; "trailing as binds to the query, not to a rule"
)]
fn simple_queries(src: &str) -> String {
    tree(src)
}

#[test_case(
    "(a:b, c:d) | compute x using /"
    => "COMPUTE_QUERY(( QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(a)) : IDENT(b))) , \
        QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(c)) : IDENT(d))) ) | KEYWORD(compute) \
        IDENT(x) KEYWORD(using) /)"
    ; "operator as the compute function"
)]
#[test_case(
    "(a:b, c:d,) | compute x using sum"
    => "COMPUTE_QUERY(( QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(a)) : IDENT(b))) , \
        QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(c)) : IDENT(d))) , ) | KEYWORD(compute) \
        IDENT(x) KEYWORD(using) FUNCTION_PATH(IDENT(sum)))"
    ; "trailing comma is allowed"
)]
#[test_case(
    "(a:b, c:d) | compute x using a::b"
    => "COMPUTE_QUERY(( QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(a)) : IDENT(b))) , \
        QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(c)) : IDENT(d))) ) | KEYWORD(compute) \
        IDENT(x) KEYWORD(using) FUNCTION_PATH(IDENT(a) :: IDENT(b)))"
    ; "module path as the compute function"
)]
fn compute_queries(src: &str) -> String {
    first(src, SyntaxKind::COMPUTE_QUERY)
}

/// A compute query is a query, so it nests wherever a query is accepted. The inner
/// `COMPUTE_QUERY` sitting under the outer one's first `QUERY` is the whole point.
#[test]
fn compute_queries_nest() {
    assert_eq!(
        tree("((a:b, c:d) | compute x using +, e:f) | compute y using -"),
        "ROOT(QUERY(COMPUTE_QUERY(( QUERY(COMPUTE_QUERY(( \
         QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(a)) : IDENT(b))) , \
         QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(c)) : IDENT(d))) ) | KEYWORD(compute) \
         IDENT(x) KEYWORD(using) +)) , \
         QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(e)) : IDENT(f))) ) | KEYWORD(compute) \
         IDENT(y) KEYWORD(using) -)))"
    );
}

// ---------------------------------------------------------------------------------------
// Directives
//
// `set` and `param` share the `DIRECTIVE` kind, and both are only accepted before the query.
// The `TYPE` node nests for `Option<…>`, which is the one recursive production here.
// ---------------------------------------------------------------------------------------

#[test_case("set a;"                   => "DIRECTIVE(KEYWORD(set) IDENT(a) ;)"                                  ; "flag without a value")]
#[test_case("set a = 42;"              => "DIRECTIVE(KEYWORD(set) IDENT(a) = CONST(INTEGER(42)) ;)"             ; "with a value")]
fn directives(src: &str) -> String {
    first(&format!("{src} d:m"), SyntaxKind::DIRECTIVE)
}

#[test_case("param $p: string;"        => "PARAM(KEYWORD(param) VARIABLE($p) : TYPE(string) ;)"             ; "declared parameter")]
#[test_case("param $p: Dataset;"       => "PARAM(KEYWORD(param) VARIABLE($p) : TYPE(Dataset) ;)"            ; "custom type")]
#[test_case("param $p: Option<int>;"   => "PARAM(KEYWORD(param) VARIABLE($p) : TYPE(Option < TYPE(int) >) ;)" ; "option nests a type")]
fn params(src: &str) -> String {
    first(&format!("{src} d:m"), SyntaxKind::PARAM)
}

/// Directives are a flat sequence of siblings under `ROOT`, ahead of the query — not
/// children of the query they configure.
#[test]
fn directives_precede_the_query_as_siblings() {
    assert_eq!(
        tree("set a = 1; param $p: bool; d:m"),
        "ROOT(DIRECTIVE(KEYWORD(set) IDENT(a) = CONST(INTEGER(1)) ;) \
         PARAM(KEYWORD(param) VARIABLE($p) : TYPE(bool) ;) \
         QUERY(SIMPLE_QUERY(IDENT_OR_VARIABLE(IDENT(d)) : IDENT(m))))"
    );
}

// ---------------------------------------------------------------------------------------
// Constants
//
// Signs are not part of the number token (`tests/lexer.rs::value_literals`), so `CONST`
// collects them itself and a negative literal is two children rather than one. `inf` is a
// float, matching `number = { inf | float | int }` in mpl.pest:30.
// ---------------------------------------------------------------------------------------

#[test_case("42"    => "CONST(INTEGER(42))"        ; "integer")]
#[test_case("1.5"   => "CONST(FLOAT(1.5))"         ; "float")]
#[test_case("-42"   => "CONST(- INTEGER(42))"      ; "sign is a sibling of the number")]
#[test_case("+42"   => "CONST(+ INTEGER(42))"      ; "explicit plus")]
#[test_case("--42"  => "CONST(- - INTEGER(42))"    ; "signs are collected, not folded")]
#[test_case("inf"   => "CONST(FLOAT(inf))"         ; "inf is a float")]
#[test_case("-inf"  => "CONST(- FLOAT(inf))"       ; "negative infinity")]
#[test_case("true"  => "CONST(BOOL(true))"         ; "true literal")]
#[test_case("false" => "CONST(BOOL(false))"        ; "false literal")]
#[test_case("\"s\"" => "CONST(STRING(\"s\"))"      ; "string")]
fn constants(src: &str) -> String {
    konst(src)
}

// ---------------------------------------------------------------------------------------
// String interpolation
//
// The lexer hands the parser a `StringSegment` for every fragment that ends in `${` and a
// `String` for the one that closes the literal (`tests/lexer.rs::string_interpolation`).
// `Parser::string` loops on that distinction alone, so what these cases pin down is that one
// `EXPR` lands between each pair of fragments and that the fragments themselves stay in the
// tree — the markers are the only record of where the interpolation was.
// ---------------------------------------------------------------------------------------

#[test_case(
    "\"a${ x }b\""
    => "CONST(STRING(\"a${ EXPR(IDENT(x)) }b\"))"
    ; "one interpolation"
)]
#[test_case(
    "\"${ $v }\""
    => "CONST(STRING(\"${ EXPR(VARIABLE($v)) }\"))"
    ; "variable body"
)]
#[test_case(
    "\"${ 1 }\""
    => "CONST(STRING(\"${ EXPR(CONST(INTEGER(1))) }\"))"
    ; "constant body"
)]
#[test_case(
    "\"a${ x }b${ y }c\""
    => "CONST(STRING(\"a${ EXPR(IDENT(x)) }b${ EXPR(IDENT(y)) }c\"))"
    ; "two interpolations share the middle fragment"
)]
#[test_case(
    "\"${ \"n${ z }\" }\""
    => "CONST(STRING(\"${ EXPR(CONST(STRING(\"n${ EXPR(IDENT(z)) }\"))) }\"))"
    ; "interpolated string inside an interpolation"
)]
fn string_interpolation(src: &str) -> String {
    konst(src)
}

// ---------------------------------------------------------------------------------------
// Arrays
//
// Elements are `EXPR`, not `CONST`: idents and variables are accepted alongside literals,
// and the element types may be mixed. See `tests/examples/where-in.mpl`.
// ---------------------------------------------------------------------------------------

#[test_case("[]"       => "ARRAY([ ])"                                             ; "empty")]
#[test_case("[1]"      => "ARRAY([ EXPR(CONST(INTEGER(1))) ])"                     ; "one element")]
#[test_case("[1, 2]"   => "ARRAY([ EXPR(CONST(INTEGER(1))) , EXPR(CONST(INTEGER(2))) ])" ; "two elements")]
#[test_case("[a]"      => "ARRAY([ EXPR(IDENT(a)) ])"                              ; "ident element")]
#[test_case("[$v]"     => "ARRAY([ EXPR(VARIABLE($v)) ])"                          ; "variable element")]
#[test_case(
    "[\"a\", true, 1.5]"
    => "ARRAY([ EXPR(CONST(STRING(\"a\"))) , EXPR(CONST(BOOL(true))) , EXPR(CONST(FLOAT(1.5))) ])"
    ; "mixed element types"
)]
fn arrays(src: &str) -> String {
    array(src)
}

// ---------------------------------------------------------------------------------------
// Pipeline rules
//
// One case per rule kind, because the rule name is matched by text in `Parser::rules` and a
// missing arm degrades to `unknown rule` rather than to a parse error at the argument.
// ---------------------------------------------------------------------------------------

#[test_case(
    "where a == 1"
    => "RULE(FILTER(KEYWORD(where) FILTER_OR(FILTER_AND(FILTER_NOT(FILTER_PAREN(\
        FILTER_CMP(IDENT(a) == EXPR(CONST(INTEGER(1))))))))))"
    ; "where rule"
)]
#[test_case(
    "filter a == 1"
    => "RULE(FILTER(KEYWORD(filter) FILTER_OR(FILTER_AND(FILTER_NOT(FILTER_PAREN(\
        FILTER_CMP(IDENT(a) == EXPR(CONST(INTEGER(1))))))))))"
    ; "filter is a synonym of where"
)]
#[test_case("as x"       => "RULE(AS(KEYWORD(as) IDENT(x)))"                            ; "as rule")]
#[test_case("sample 0.5" => "RULE(SAMPLE(KEYWORD(sample) FLOAT(0.5)))"                  ; "sample")]
#[test_case("map rate"   => "RULE(MAP(KEYWORD(map) FUNCTION_PATH(IDENT(rate))))"         ; "map without arguments")]
#[test_case(
    "map filter::gt(1)"
    => "RULE(MAP(KEYWORD(map) FUNCTION_PATH(IDENT(filter) :: IDENT(gt)) ( CONST(INTEGER(1)) )))"
    ; "map with a module path and an argument"
)]
#[test_case(
    "map * 2"
    => "RULE(MAP(KEYWORD(map) MAP_MATH(* EXPR(CONST(INTEGER(2))))))"
    ; "map with an operator takes the MAP_MATH branch"
)]
#[test_case(
    "map + $v"
    => "RULE(MAP(KEYWORD(map) MAP_MATH(+ EXPR(VARIABLE($v)))))"
    ; "map math against a parameter"
)]
#[test_case(
    "align using avg"
    => "RULE(ALIGN(KEYWORD(align) KEYWORD(using) FUNCTION_PATH(IDENT(avg))))"
    ; "align without a target"
)]
#[test_case(
    "align to 7d using avg"
    => "RULE(ALIGN(KEYWORD(align) KEYWORD(to) DURATION(7 TIME_UNIT(d)) KEYWORD(using) \
        FUNCTION_PATH(IDENT(avg))))"
    ; "align to a duration"
)]
#[test_case(
    "align to $d using avg"
    => "RULE(ALIGN(KEYWORD(align) KEYWORD(to) VARIABLE($d) KEYWORD(using) FUNCTION_PATH(IDENT(avg))))"
    ; "align to a parameter takes the variable branch"
)]
#[test_case(
    "group using max"
    => "RULE(GROUP(KEYWORD(group) KEYWORD(using) FUNCTION_PATH(IDENT(max))))"
    ; "group without tags"
)]
#[test_case(
    "group by a, b using sum"
    => "RULE(GROUP(KEYWORD(group) KEYWORD(by) TAG_LIST(IDENT(a) , IDENT(b)) KEYWORD(using) \
        FUNCTION_PATH(IDENT(sum))))"
    ; "group by a tag list"
)]
#[test_case(
    "bucket using histogram()"
    => "RULE(BUCKET(KEYWORD(bucket) KEYWORD(using) FUNCTION_PATH(IDENT(histogram)) ( )))"
    ; "bucket with empty argument list emits no BUCKET_ARGS"
)]
#[test_case(
    "bucket by a to 5m using histogram(1.0, 2.0)"
    => "RULE(BUCKET(KEYWORD(bucket) KEYWORD(by) TAG_LIST(IDENT(a)) KEYWORD(to) \
        DURATION(5 TIME_UNIT(m)) KEYWORD(using) FUNCTION_PATH(IDENT(histogram)) ( \
        BUCKET_ARGS(BUCKET_ARG(FLOAT(1.0)) , BUCKET_ARG(FLOAT(2.0))) )))"
    ; "bucket with every clause"
)]
#[test_case(
    "bucket using histogram(le)"
    => "RULE(BUCKET(KEYWORD(bucket) KEYWORD(using) FUNCTION_PATH(IDENT(histogram)) ( \
        BUCKET_ARGS(BUCKET_ARG(IDENT(le))) )))"
    ; "bucket argument may be an ident"
)]
#[test_case(
    "extend a = 1"
    => "RULE(EXTEND(KEYWORD(extend) EXTEND_PART(IDENT(a) = EXPR(CONST(INTEGER(1))))))"
    ; "extend with one part"
)]
#[test_case(
    "extend a = 1, b = \"x\""
    => "RULE(EXTEND(KEYWORD(extend) EXTEND_PART(IDENT(a) = EXPR(CONST(INTEGER(1)))) , \
        EXTEND_PART(IDENT(b) = EXPR(CONST(STRING(\"x\"))))))"
    ; "extend parts are siblings"
)]
#[test_case(
    "ifdef ($p) { where a == 1 }"
    => "RULE(IFDEF(KEYWORD(ifdef) ( VARIABLE($p) ) { FILTER(KEYWORD(where) FILTER_OR(\
        FILTER_AND(FILTER_NOT(FILTER_PAREN(FILTER_CMP(IDENT(a) == EXPR(CONST(INTEGER(1))))))))) }))"
    ; "ifdef"
)]
#[test_case(
    "ifdef ($p) { where a == 1 } else { where b == 2 }"
    => "RULE(IFDEF(KEYWORD(ifdef) ( VARIABLE($p) ) { FILTER(KEYWORD(where) FILTER_OR(\
        FILTER_AND(FILTER_NOT(FILTER_PAREN(FILTER_CMP(IDENT(a) == EXPR(CONST(INTEGER(1))))))))) } \
        KEYWORD(else) { FILTER(KEYWORD(where) FILTER_OR(FILTER_AND(FILTER_NOT(FILTER_PAREN(\
        FILTER_CMP(IDENT(b) == EXPR(CONST(INTEGER(2))))))))) }))"
    ; "ifdef else"
)]
fn rules(src: &str) -> String {
    rule(src)
}

/// Rules are siblings under the query, one `RULE` per `|`, in source order — a pipeline is
/// flat, not left-nested.
#[test]
fn rules_are_flat_siblings() {
    let tree = parse_clean("d:m | where a == 1 | group using sum | sample 0.5");
    let kinds = tree
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::RULE)
        .map(|n| {
            n.children()
                .map(|c| format!("{:?}", c.kind()))
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["FILTER", "GROUP", "SAMPLE"]);
}

// ---------------------------------------------------------------------------------------
// Durations
//
// The unit is a separate `TIME_UNIT` node and it is optional, so `align to 5 using …` is
// accepted with a bare number. Only the eight units in `Parser::duration` are units; any
// other trailing ident is left for the next production, which is what turns
// `tests/errors/invalid_time_unit.mpl` into an error at `using` rather than at the unit.
// ---------------------------------------------------------------------------------------

#[test_case("1ms" => "DURATION(1 TIME_UNIT(ms))" ; "milliseconds")]
#[test_case("1s"  => "DURATION(1 TIME_UNIT(s))"  ; "seconds")]
#[test_case("1m"  => "DURATION(1 TIME_UNIT(m))"  ; "minutes")]
#[test_case("1h"  => "DURATION(1 TIME_UNIT(h))"  ; "hours")]
#[test_case("1d"  => "DURATION(1 TIME_UNIT(d))"  ; "days")]
#[test_case("1w"  => "DURATION(1 TIME_UNIT(w))"  ; "weeks")]
#[test_case("1M"  => "DURATION(1 TIME_UNIT(M))"  ; "months are case sensitive")]
#[test_case("1y"  => "DURATION(1 TIME_UNIT(y))"  ; "years")]
#[test_case("5"   => "DURATION(5)"               ; "unit is optional")]
fn durations(src: &str) -> String {
    duration(src)
}

// ---------------------------------------------------------------------------------------
// Comparisons
//
// The right-hand side is an `EXPR` for every operator except `==` / `!=`, which additionally
// accept a `REGEX`, and `is` / `in`, which take a type and an array respectively. `is` and
// `in` are matched by text, not by token kind — the lexer emits no keywords
// (`tests/lexer.rs::identifiers`) — so each needs its own case.
// ---------------------------------------------------------------------------------------

#[test_case("a == 1"    => "FILTER_CMP(IDENT(a) == EXPR(CONST(INTEGER(1))))"  ; "equal")]
#[test_case("a != 1"    => "FILTER_CMP(IDENT(a) != EXPR(CONST(INTEGER(1))))"  ; "not equal")]
#[test_case("a < 1"     => "FILTER_CMP(IDENT(a) < EXPR(CONST(INTEGER(1))))"   ; "less than")]
#[test_case("a <= 1"    => "FILTER_CMP(IDENT(a) <= EXPR(CONST(INTEGER(1))))"  ; "less than or equal")]
#[test_case("a > 1"     => "FILTER_CMP(IDENT(a) > EXPR(CONST(INTEGER(1))))"   ; "greater than")]
#[test_case("a >= 1"    => "FILTER_CMP(IDENT(a) >= EXPR(CONST(INTEGER(1))))"  ; "greater than or equal")]
#[test_case("a == #/x/" => "FILTER_CMP(IDENT(a) == REGEX(#/x/))"              ; "regex is not wrapped in EXPR")]
#[test_case("a != #/x/" => "FILTER_CMP(IDENT(a) != REGEX(#/x/))"              ; "negated regex")]
#[test_case("a == b"    => "FILTER_CMP(IDENT(a) == EXPR(IDENT(b)))"           ; "tag against tag")]
#[test_case("a == $v"   => "FILTER_CMP(IDENT(a) == EXPR(VARIABLE($v)))"       ; "tag against a parameter")]
#[test_case(
    "a == \"x${ y }\""
    => "FILTER_CMP(IDENT(a) == EXPR(CONST(STRING(\"x${ EXPR(IDENT(y)) }\"))))"
    ; "interpolated string on the right"
)]
#[test_case("a is string" => "FILTER_CMP(IDENT(a) KEYWORD(is) OTEL_TYPE(IDENT(string)))" ; "is string")]
#[test_case("a is int"    => "FILTER_CMP(IDENT(a) KEYWORD(is) OTEL_TYPE(IDENT(int)))"    ; "is int")]
#[test_case("a is float"  => "FILTER_CMP(IDENT(a) KEYWORD(is) OTEL_TYPE(IDENT(float)))"  ; "is float")]
#[test_case("a is bool"   => "FILTER_CMP(IDENT(a) KEYWORD(is) OTEL_TYPE(IDENT(bool)))"   ; "is bool")]
#[test_case("a is array"  => "FILTER_CMP(IDENT(a) KEYWORD(is) OTEL_TYPE(IDENT(array)))"  ; "is array")]
#[test_case(
    "a in [1, 2]"
    => "FILTER_CMP(IDENT(a) KEYWORD(in) ARRAY([ EXPR(CONST(INTEGER(1))) , EXPR(CONST(INTEGER(2))) ]))"
    ; "in an array literal"
)]
#[test_case(
    "a in $v"
    => "FILTER_CMP(IDENT(a) KEYWORD(in) VARIABLE($v))"
    ; "in a parameter takes the variable branch"
)]
#[test_case("`a b` == 1" => "FILTER_CMP(IDENT(`a b`) == EXPR(CONST(INTEGER(1))))" ; "escaped tag name")]
fn comparisons(src: &str) -> String {
    cmp(src)
}

// ---------------------------------------------------------------------------------------
// Boolean grouping
//
// Precedence is encoded in the shape of the tree, not in a precedence table, so it is the
// nesting that has to be asserted: `or` sits above `and`, `and` above `not`, and a
// parenthesised group re-enters at `or`. Rendered by `groups`, which collapses the wrappers
// that carry no grouping information.
// ---------------------------------------------------------------------------------------

#[test_case("a == 1"                        => "a"                     ; "a bare comparison has no grouping")]
#[test_case("a == 1 and b == 2"             => "AND(a b)"              ; "and")]
#[test_case("a == 1 or b == 2"              => "OR(a b)"               ; "or")]
#[test_case("a == 1 and b == 2 and c == 3"  => "AND(a b c)"            ; "and chains flat")]
#[test_case("a == 1 or b == 2 or c == 3"    => "OR(a b c)"             ; "or chains flat")]
#[test_case("a == 1 or b == 2 and c == 3"   => "OR(a AND(b c))"        ; "and binds tighter than or")]
#[test_case("a == 1 and b == 2 or c == 3"   => "OR(AND(a b) c)"        ; "and binds tighter on the left too")]
#[test_case("(a == 1 or b == 2) and c == 3" => "AND(OR(a b) c)"        ; "parentheses override precedence")]
#[test_case("not a == 1"                    => "NOT(a)"                ; "not")]
#[test_case("not a == 1 and b == 2"         => "AND(NOT(a) b)"         ; "not binds tighter than and")]
#[test_case("not (a == 1 or b == 2)"        => "NOT(OR(a b))"          ; "not over a group")]
#[test_case("not a == 1 or not b == 2"      => "OR(NOT(a) NOT(b))"     ; "two negated operands")]
#[test_case("((a == 1))"                    => "a"                     ; "redundant parentheses add no grouping")]
fn boolean_grouping(src: &str) -> String {
    grouping(src)
}

/// The wrapper chain `groups` collapses, pinned once in full so the collapsing cannot hide a
/// node that stopped being emitted.
#[test]
fn filter_wrapper_chain() {
    assert_eq!(
        cmp("a == 1"),
        "FILTER_CMP(IDENT(a) == EXPR(CONST(INTEGER(1))))"
    );
    assert_eq!(
        rule("where a == 1"),
        "RULE(FILTER(KEYWORD(where) FILTER_OR(FILTER_AND(FILTER_NOT(FILTER_PAREN(\
         FILTER_CMP(IDENT(a) == EXPR(CONST(INTEGER(1))))))))))"
    );
}

// ---------------------------------------------------------------------------------------
// Trivia
//
// Whitespace and comments are kept in the tree — that is what makes it lossless — but they
// must not change its shape. Both halves matter: dropping them would break a formatter, and
// letting them separate productions would make the shape depend on formatting.
// ---------------------------------------------------------------------------------------

#[test_case("d:m|where a==1"                       ; "no whitespace at all")]
#[test_case("d : m | where a == 1"                 ; "space around every token")]
#[test_case("d:m\n| where a == 1"                  ; "newline before the pipe")]
#[test_case("d:m // note\n| where a == 1"          ; "comment between query and rule")]
#[test_case("d:m |\n  where\n  a == 1"             ; "rule spread over lines")]
#[test_case("d:m | where a\n==\n1"                 ; "operator on its own line")]
fn trivia_does_not_change_the_shape(src: &str) {
    assert_eq!(
        tree(src),
        tree("d:m|where a==1"),
        "shape changed for {src:?}"
    );
}

#[test_case("// only a comment"        ; "comment")]
#[test_case("   "                      ; "whitespace")]
#[test_case("\n\t "                    ; "mixed whitespace")]
fn trivia_only_input_is_kept_and_reported(src: &str) {
    let (tree, errors) = Parser::new(src).parse();
    assert_eq!(tree.to_string(), src, "trivia was dropped from {src:?}");
    assert!(
        !errors.is_empty(),
        "{src:?} is not a query but parsed clean"
    );
}

/// Leading and trailing trivia belong to the tree too, including a comment that runs to the
/// end of input without a newline.
#[test_case("\nd:m"                    ; "leading newline")]
#[test_case("d:m\n"                    ; "trailing newline")]
#[test_case("d:m // trailing"          ; "trailing comment without a newline")]
#[test_case("// leading\nd:m"          ; "leading comment")]
fn trivia_at_the_edges_is_kept(src: &str) {
    // `parse_clean` asserts the round trip, which is the whole claim here.
    parse_clean(src);
}

// ---------------------------------------------------------------------------------------
// Errors
//
// The parser reports every error it finds rather than stopping at the first, so a case pins
// the whole sequence. Spans matter as much as messages: they are what the language server
// underlines, and an error carrying the wrong span is worse than one carrying a vague
// message.
//
// `Eof` appears once per lookahead past the end of input, so a truncated query reports
// several. That is not obviously desirable — see the note on `assert_errors_are_bounded` —
// but it is pinned here so a change to the recovery strategy is a deliberate one.
//
// `TokenAfterEoq` names the first token of the unparsed tail and spans that tail to the end
// of input, because all of it is what the parser failed to place and what an editor should
// underline. Its `kind` and its span therefore describe different amounts of text.
//
// Whether it overlaps the error before it records whether that error consumed its token.
// `error` consumes, so `misspelled rule name` partitions the input: `@6+6` names the rule,
// `@13+6` covers what followed it. The `error_token` call sites that report a token they
// only peeked do not consume, so `not an operator` and `unknown otel type` still report the
// same text under two errors — and, because `error_token` also emits what it reports, put
// that text into the tree twice. See the note on `error_recovery_is_lossless`.
// ---------------------------------------------------------------------------------------

#[test_case(
    ""
    => "\"expected query\"@0+0"
    ; "empty input"
)]
#[test_case(
    "| where a == 1"
    => "\"expected query\"@0+1 TokenAfterEoq(Ident)@2+12"
    ; "rule without a query"
)]
#[test_case(
    "d:m x"
    => "TokenAfterEoq(Ident)@4+1"
    ; "trailing token after a complete query"
)]
#[test_case(
    "d:m | fflter a == 1"
    => "\"unknown rule: fflter\"@6+6 TokenAfterEoq(Ident)@13+6"
    ; "misspelled rule name"
)]
#[test_case(
    "d:m | where a ~ 1"
    => "\"expected comparison operator\"@14+1 TokenAfterEoq(Integer)@16+1"
    ; "not an operator"
)]
#[test_case(
    "d:m | where a is nonsense"
    => "\"expected otel type ident\"@17+8"
    ; "unknown otel type"
)]
#[test_case(
    "param $p: nonsense; d:m"
    => "\"unknown type nonsense\"@10+8"
    ; "unknown parameter type"
)]
#[test_case(
    "d:m | sample 1"
    => "\"expected Float, got Integer\"@13+1"
    ; "sample takes a float"
)]
#[test_case(
    "d:m | bucket using histogram(1)"
    => "\"expected ident or float in bucket arg\"@29+1"
    ; "bucket argument may not be an integer"
)]
#[test_case(
    "d:"
    => "\"expected ident but got  (Eof)\"@2+0 Eof"
    ; "query truncated after the colon"
)]
#[test_case(
    "d:m | where a == \"unterminated"
    => "\"expected constant\"@17+13"
    ; "unterminated string reaches the parser as an invalid token"
)]
fn error_reporting(src: &str) -> String {
    errors(src)
}

/// A time range is part of MPL — `time_range`, mpl.pest:71, reachable from `source` at :82.
/// `Parser::time_range` covers the `time_relative` and `time_timestamp` forms; `mpl.pest`
/// also admits `time_rfc_3339` and `time_modifier`, which the lexer does not yet tokenise
/// separately, so `d:m[2025-03-01T13:00:00Z..]` is still out of reach.
#[test]
fn time_ranges_parse() {
    for src in ["d:m[5m..]", "d:m[1..2] | group using sum"] {
        let (_tree, errors) = Parser::new(src).parse();
        assert!(errors.is_empty(), "{src:?} should parse: {errors:?}");
    }
}

// ---------------------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------------------

/// Inputs kept together so every property below gets the same adversarial set: valid
/// queries, queries that are only nearly valid, and the truncations that exercise the
/// end-of-input paths.
const CORPUS: &[&str] = &[
    "",
    " ",
    "\n",
    "// comment only",
    "d:m",
    "d:m | where a == 1",
    "d:m|where a==1",
    "$ds:`a b` as x | where a in [1, \"2\", true] or not b == #/x/",
    "set a = 1; set b; param $p: Option<int>; d:m | sample 0.5",
    "(a:b, c:d,) | compute x using /",
    "((a:b, c:d) | compute x using +, e:f) | compute y using max",
    "d:m | extend u = \"a${ x }b${ $y }c\"",
    "d:m | ifdef ($p) { where a == 1 } else { where b == 2 }",
    "d:m | bucket by a, b to 5m using histogram(le, 1.0)",
    "d:m | align to 7d using avg | group by pod using sum",
    // Malformed: each one leaves the parser in a different recovery state.
    "d",
    "d:",
    "d:m |",
    "d:m | where",
    "d:m | where a",
    "d:m | where a ==",
    "d:m | fflter a == 1",
    "d:m x",
    "| where a == 1",
    "d:m[5m..]",
    "set a = [1, 2]; d:m",
    "d:m | where a == \"unterminated",
    // The three inputs that reach an `error_token` call site reporting a token it only
    // peeked — see `error_recovery_does_not_duplicate`.
    "d:m | where a ~ 1",
    "d:m | bucket using histogram(1)",
    "d:m | where a is nonsense",
    "param $p: nonsense; d:m",
    "((((",
    "))))",
    "|||",
    "d:m | where (((a == 1)))",
    "🎉",
    "d:m | where é == \"héllo ${ ö }\"",
];

/// Every `.mpl` file under `dir`, as `(file name, contents)`.
fn files(dir: &str, extension: &str) -> Vec<(String, String)> {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{dir} is not readable: {e}"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == extension))
        .map(|entry| {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let content = fs::read_to_string(&path).expect("readable example");
            (name, content)
        })
        .collect::<Vec<_>>();
    // A renamed directory or extension would otherwise turn every test built on this into a
    // silent pass over nothing, which is how `parse_unimplemented_examples` below got lost.
    assert!(!entries.is_empty(), "no .{extension} files in {dir}");
    entries
}

/// Renders the parser's errors the way a user would see them, so a failing example prints a
/// diagnostic rather than a debug dump.
fn report(name: &str, content: &str, errors: &[SyntaxError]) -> String {
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    let mut out = String::new();
    for error in errors {
        let report = Report::msg(error.to_string())
            .with_source_code(NamedSource::new(name, content.to_string()));
        let mut rendered = String::new();
        if handler
            .render_report(&mut rendered, report.as_ref())
            .is_err()
        {
            rendered = report.to_string();
        }
        out.push_str(&rendered);
    }
    out
}

#[test]
fn parse_examples() {
    for (name, content) in files("./tests/examples", "mpl") {
        let (_tree, errors) = Parser::new(&content).parse();
        assert!(
            errors.is_empty(),
            "[{name}] {}\n{errors:?}",
            report(&name, &content, &errors)
        );
    }
}

/// The `.unimplemented` examples are features the parser does not have yet, kept next to the
/// working ones so the gap is visible.
///
/// The status of each is pinned rather than assumed: this test used to filter on the
/// extension `mpl-todo`, which no file has, so it ran over an empty set and passed. Pinning
/// means that when a feature lands the test fails and the file gets moved to
/// `tests/examples/*.mpl`, instead of the example sitting there unread forever.
#[test_case("enrich.mpl.unimplemented"         => false ; "enrich needs a join rule")]
#[test_case("nested-enrich.mpl.unimplemented"  => false ; "nested enrich needs a join rule")]
#[test_case("replace_labels.mpl.unimplemented" => true  ; "replace labels already parses")]
fn unimplemented_examples_parse(name: &str) -> bool {
    let content = fs::read_to_string(format!("./tests/examples/{name}"))
        .unwrap_or_else(|e| panic!("{name} is not readable: {e}"));
    let (_tree, errors) = Parser::new(&content).parse();
    errors.is_empty()
}

/// The error corpus is a mix: some files are rejected by the syntax tree, others parse
/// cleanly and are rejected later by the type checker or the linker. Which is which is
/// pinned, because a file silently moving from one group to the other means the parser's
/// coverage changed.
#[test]
fn parse_error_examples() {
    /// Files the syntax tree itself rejects. Everything else in `tests/errors` is a
    /// well-formed query that fails a later stage.
    const REJECTED_BY_THE_PARSER: &[&str] = &[
        "incomplete_query.mpl",
        "missing_pipe.mpl",
        "typo_keyword.mpl",
        "in-int.mpl",
        "invalid_time_unit.mpl",
        "in-trailing-comma.mpl",
        "invalid_operator.mpl",
    ];

    for (name, content) in files("./tests/errors", "mpl") {
        let (_tree, errors) = Parser::new(&content).parse();
        if REJECTED_BY_THE_PARSER.contains(&name.as_str()) {
            assert!(!errors.is_empty(), "[{name}] expected a syntax error");
        } else {
            assert!(
                errors.is_empty(),
                "[{name}] {}\n{errors:?}",
                report(&name, &content, &errors)
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// Structural properties
//
// These state the relationships the tables cannot: they run over the corpus, the shipped
// examples, every prefix of those examples, and randomly generated queries.
// ---------------------------------------------------------------------------------------

/// Property: a clean parse reproduces its input byte for byte.
///
/// This is the defining property of a lossless CST and the one every consumer depends on —
/// a formatter that prints the tree must print the file back, and the language server maps
/// positions by walking token lengths. It also underwrites every table above, since a
/// dropped token leaves a shape that still looks well-formed.
///
/// It is stated for clean parses only because recovery does not hold it: the `error_token`
/// call sites that report a token they only peeked emit it into the tree *and* leave it in
/// the lexer, so the next production emits it a second time. `d:m | where a ~ 1` renders as
/// `d:m | where a ~~ 1`. See `error_recovery_is_lossless` for the sites and the cases.
fn assert_lossless(src: &str) {
    let (tree, errors) = Parser::new(src).parse();
    if errors.is_empty() {
        assert_eq!(tree.to_string(), src, "clean parse of {src:?} lost text");
    }
}

/// Property: every token the lexer produced reaches the tree, in source order.
///
/// This is the half of losslessness that survives error recovery, and it needs its own
/// statement because `assert_lossless` gives up the moment there is an error — which leaves
/// broken input, the case a CST exists to handle, checked by nothing at all.
///
/// Deliberately a subsequence and not an equality. Recovery is allowed to emit a token twice
/// today, so an equality would fail; what must never happen is a token going missing, since
/// a formatter that silently drops one corrupts the file it is formatting, which is worse
/// than one that prints a stray character. Stating it this way also makes the test useful in
/// exactly the case that matters: if the duplication in `error_recovery_does_not_duplicate`
/// is ever "fixed" by discarding the token rather than consuming it, this goes red.
fn assert_no_token_is_dropped(src: &str) {
    let (tree, _errors) = Parser::new(src).parse();
    // `any` advances the iterator, so consecutive calls match the tree's tokens in order —
    // extra tokens between two matches are skipped, which is what makes this a subsequence.
    let mut in_tree = tree
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .map(|token| token.text().to_string())
        // The synthetic end-of-input token carries no text and has no lexer counterpart.
        .filter(|text| !text.is_empty());

    for token in Lexer::new(src).filter(|token| !token.is_eof()) {
        assert!(
            in_tree.any(|text| text == token.text()),
            "lexer token {:?} at byte {} never reached the tree for {src:?}",
            token.text(),
            token.pos()
        );
    }
}

/// Property: a node's text is exactly the input at its own range.
///
/// Ranges are what a language server resolves a cursor position against, and they are
/// computed from token lengths rather than from the input, so nothing but this check ties
/// them back to the source. Multi-byte input is where it bites: a range in characters
/// instead of bytes still looks plausible until a `é` shifts everything after it.
fn assert_ranges_match_the_input(src: &str) {
    let (tree, errors) = Parser::new(src).parse();
    if !errors.is_empty() {
        return; // text is missing, so ranges cannot line up with the input
    }
    assert_eq!(
        usize::from(tree.text_range().end()),
        src.len(),
        "root range does not cover {src:?}"
    );
    for node in tree.descendants() {
        let range = node.text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        assert!(
            src.is_char_boundary(start) && src.is_char_boundary(end),
            "{:?} range {start}..{end} is not on a char boundary in {src:?}",
            node.kind()
        );
        assert_eq!(
            node.text().to_string(),
            src[start..end],
            "{:?} text does not match the input at {start}..{end} in {src:?}",
            node.kind()
        );
        if let Some(parent) = node.parent() {
            assert!(
                parent.text_range().contains_range(range),
                "{:?} escapes its {:?} parent in {src:?}",
                node.kind(),
                parent.kind()
            );
        }
    }
}

/// Property: tokens carry lexer kinds and nodes never do.
///
/// The two kind spaces share one enum, and `GreenNodeBuilder` will happily accept a node
/// kind for a token or the reverse. Getting it wrong produces a tree that renders correctly
/// and round-trips, and only breaks whichever consumer matches on kinds — which is all of
/// them.
fn assert_kind_discipline(src: &str) {
    let (tree, _errors) = Parser::new(src).parse();
    for node in tree.descendants() {
        assert!(
            !is_lexer_kind(node.kind()),
            "node has the token kind {:?} in {src:?}",
            node.kind()
        );
        for token in node.children_with_tokens().filter_map(|c| c.into_token()) {
            assert!(
                is_lexer_kind(token.kind()),
                "token has the node kind {:?} in {src:?}",
                token.kind()
            );
        }
    }
}

/// Kinds that stand for a lexer token. `EOF` is one of them by position in the enum. A clean
/// parse never builds it — `parse` consumes the end-of-input token and drops it — but a
/// failing one does: `Parser::error` at end of input reports the synthetic `Eof` token, and
/// `error_token` wraps whatever it reports in an `INVALID` node. Parsing `""` gives
/// `ROOT → QUERY → INVALID → EOF@0..0 ""`. Its text is empty, so it costs the round trip
/// nothing, but it is a token kind that reaches consumers.
fn is_lexer_kind(kind: SyntaxKind) -> bool {
    (SyntaxKind::EOF as u16..=SyntaxKind::LX_INF as u16).contains(&(kind as u16))
}

/// Property: every error points somewhere inside the input.
///
/// A span past the end of the source panics `miette` when it renders the diagnostic, which
/// turns a syntax error into a crash in the language server. Truncated input is where the
/// risk lives, since the end-of-input token is synthesised at `input.len()`.
fn assert_spans_are_in_bounds(src: &str) {
    let (_tree, errors) = Parser::new(src).parse();
    for error in &errors {
        let range = match error {
            SyntaxError::Eof { .. } => continue,
            SyntaxError::TokenAfterEoq { range, .. } | SyntaxError::Generic { range, .. } => range,
        };
        let end = range.offset() + range.len();
        assert!(
            end <= src.len(),
            "{} ends at {end} but the input is {} bytes: {src:?}",
            describe_error(error),
            src.len()
        );
    }
}

/// Property: the parser terminates, and reports a number of errors proportional to the
/// input rather than looping on one.
///
/// Every lookahead past the end of input pushes another `Eof`, so a production that fails to
/// consume would spin while the error list grows. The bound is what catches that: a real
/// hang never returns, but a slow loop shows up here as an implausible error count.
///
/// The factor is deliberately loose. Nesting is the worst case — an unclosed `(` costs a
/// measured 20 errors per level, since every production between `compute_query` and the
/// end of the query reports one — and the point of the bound is to catch growth that is not
/// linear at all, not to pin that constant.
fn assert_errors_are_bounded(src: &str) {
    let (_tree, errors) = Parser::new(src).parse();
    assert!(
        errors.len() <= 32 * src.len() + 32,
        "{} errors for {} bytes of input {src:?}",
        errors.len(),
        src.len()
    );
}

#[test]
fn corpus_properties() {
    for src in CORPUS {
        assert_lossless(src);
        assert_no_token_is_dropped(src);
        assert_ranges_match_the_input(src);
        assert_kind_discipline(src);
        assert_spans_are_in_bounds(src);
        assert_errors_are_bounded(src);
    }
}

#[test]
fn example_properties() {
    for (_name, content) in files("./tests/examples", "mpl") {
        assert_lossless(&content);
        assert_no_token_is_dropped(&content);
        assert_ranges_match_the_input(&content);
        assert_kind_discipline(&content);
        assert_spans_are_in_bounds(&content);
        assert_errors_are_bounded(&content);
    }
}

/// Every prefix of every example, which is what an editor sends while someone is typing.
///
/// Truncation reaches end-of-input from inside every production in turn — the one place a
/// recursive descent parser can loop or index past the end — and no single hand-written case
/// covers all of those states.
#[test]
fn every_prefix_of_every_example_parses() {
    let mut checked = 0usize;
    for (_name, content) in files("./tests/examples", "mpl") {
        for split in 0..=content.len() {
            if !content.is_char_boundary(split) {
                continue;
            }
            let prefix = &content[..split];
            assert_spans_are_in_bounds(prefix);
            assert_errors_are_bounded(prefix);
            assert_kind_discipline(prefix);
            assert_lossless(prefix);
            assert_no_token_is_dropped(prefix);
            checked += 1;
        }
    }
    assert!(checked > 1000, "only {checked} prefixes checked");
}

// ---------------------------------------------------------------------------------------
// Syntax kinds
// ---------------------------------------------------------------------------------------

/// Property: every discriminant up to `ROOT` survives the round trip through `rowan`.
///
/// `kind_from_raw` indexes `ALL_KINDS`, whose length is written as `ROOT as usize + 1`, so a
/// variant added before `ROOT` and left out of the table is a compile error. What the type
/// system cannot catch is the table listing the right number of kinds in the wrong order, or
/// a variant added *after* `ROOT` — both leave the length correct. This walks every
/// discriminant to catch the first; `kinds_in_real_trees_round_trip` catches the second for
/// any kind the parser actually emits.
#[test]
fn every_kind_round_trips_through_raw() {
    for raw in 0..=(SyntaxKind::ROOT as u16) {
        let kind = Lang::kind_from_raw(rowan::SyntaxKind(raw));
        assert_eq!(
            Lang::kind_to_raw(kind),
            rowan::SyntaxKind(raw),
            "kind {raw} does not round trip"
        );
    }
}

#[test]
fn kind_from_raw_rejects_values_past_root() {
    assert_eq!(
        Lang::kind_from_raw(rowan::SyntaxKind(SyntaxKind::ROOT as u16 + 1)),
        SyntaxKind::THIS_SHOULD_NEVER_BE_EMITTED_GOD_DAMN_IT,
        "kind past `ROOT` should be rejected"
    );
}

/// Every kind the parser actually builds round-trips, which is the same check made against
/// real trees rather than against the enum.
#[test]
fn kinds_in_real_trees_round_trip() {
    for (_name, content) in files("./tests/examples", "mpl") {
        let (tree, _errors) = Parser::new(&content).parse();
        for element in tree.descendants_with_tokens() {
            let kind = element.kind();
            assert_eq!(
                Lang::kind_from_raw(Lang::kind_to_raw(kind)),
                kind,
                "{kind:?} does not round trip"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// Generated queries
//
// The tables cover one shape per production. What they cannot cover is the combinations:
// rules in any order and any number, nested compute queries, and trivia in every gap. The
// generator emits only valid MPL, so the property is sharp — every generated query must
// parse without a single error and reproduce itself exactly.
// ---------------------------------------------------------------------------------------

/// Xorshift64* so a failing case is reproducible from its seed alone, matching
/// `tests/lexer.rs`.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let idx = usize::try_from(self.next_u64() % items.len() as u64).unwrap_or(0);
        &items[idx]
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// Trivia to drop between two tokens. Every entry contains whitespace, so it always
/// separates the tokens it sits between — `group by` must not become `groupby`. The comment
/// carries its own newline, since a comment runs to the end of the line.
const TRIVIA: &[&str] = &[" ", "  ", "\n", "\t", "\n  ", " // note\n"];

/// Joins tokens with random trivia, which is what puts a comment or a line break in every
/// gap the grammar allows one in.
fn join(rng: &mut Rng, tokens: &[&str]) -> String {
    let mut out = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            out.push_str(rng.pick::<&str>(TRIVIA));
        }
        out.push_str(token);
    }
    out
}

const IDENTS: &[&str] = &["a", "b", "svc", "`a b`", "code", "pod"];
const FUNCS: &[&str] = &["sum", "max", "avg", "filter::gt", "is::lt", "a::b::c"];
const SCALARS: &[&str] = &[
    "1",
    "-1",
    "0.5",
    "-0.5",
    "inf",
    "-inf",
    "true",
    "false",
    "\"s\"",
    "\"a${ x }b\"",
];

fn gen_expr(rng: &mut Rng) -> String {
    match rng.below(3) {
        0 => (*rng.pick(IDENTS)).to_string(),
        1 => "$v".to_string(),
        _ => (*rng.pick(SCALARS)).to_string(),
    }
}

fn gen_array(rng: &mut Rng) -> String {
    let len = rng.below(4);
    let items = (0..len).map(|_| gen_expr(rng)).collect::<Vec<_>>();
    format!("[{}]", items.join(", "))
}

fn gen_cmp(rng: &mut Rng) -> String {
    let tag = *rng.pick(IDENTS);
    match rng.below(6) {
        0 => {
            let op = *rng.pick(&["==", "!="]);
            join(rng, &[tag, op, "#/x.+/"])
        }
        1 => {
            let tpe = *rng.pick(&["string", "int", "float", "bool", "array"]);
            join(rng, &[tag, "is", tpe])
        }
        2 => {
            let array = gen_array(rng);
            join(rng, &[tag, "in", &array])
        }
        3 => join(rng, &[tag, "in", "$v"]),
        _ => {
            let op = *rng.pick(&["==", "!=", "<", "<=", ">", ">="]);
            let rhs = gen_expr(rng);
            join(rng, &[tag, op, &rhs])
        }
    }
}

/// `depth` bounds the parenthesised nesting; without it the generator recurses until the
/// stack runs out rather than until the test is interesting.
fn gen_filter(rng: &mut Rng, depth: u64) -> String {
    let operand = |rng: &mut Rng| {
        if depth > 0 && rng.below(4) == 0 {
            format!("({})", gen_filter(rng, depth - 1))
        } else {
            gen_cmp(rng)
        }
    };
    // Only one `not` per operand: `filter_not` consumes a single keyword, so `not not` is
    // not in the language.
    let term = |rng: &mut Rng| {
        if rng.below(4) == 0 {
            let inner = operand(rng);
            join(rng, &["not", &inner])
        } else {
            operand(rng)
        }
    };
    let mut out = term(rng);
    for _ in 0..rng.below(3) {
        let op = *rng.pick(&["and", "or"]);
        let rhs = term(rng);
        out = join(rng, &[&out, op, &rhs]);
    }
    out
}

fn gen_duration(rng: &mut Rng) -> String {
    let unit = *rng.pick(&["ms", "s", "m", "h", "d", "w", "M", "y", ""]);
    format!("{}{unit}", 1 + rng.below(60))
}

fn gen_rule(rng: &mut Rng) -> String {
    let ident = *rng.pick(IDENTS);
    let func = *rng.pick(FUNCS);
    match rng.below(10) {
        0 => {
            let keyword = *rng.pick(&["where", "filter"]);
            let filter = gen_filter(rng, 2);
            join(rng, &[keyword, &filter])
        }
        1 => join(rng, &["as", ident]),
        2 => {
            let rate = *rng.pick(&["0.1", "0.5", "1.0"]);
            join(rng, &["sample", rate])
        }
        3 => match rng.below(3) {
            0 => join(rng, &["map", func]),
            1 => {
                let arg = *rng.pick(SCALARS);
                join(rng, &["map", &format!("{func}({arg})")])
            }
            _ => {
                let op = *rng.pick(&["*", "/", "+", "-"]);
                let rhs = gen_expr(rng);
                join(rng, &["map", op, &rhs])
            }
        },
        4 => {
            let target = if rng.below(2) == 0 {
                gen_duration(rng)
            } else {
                "$v".to_string()
            };
            join(rng, &["align", "to", &target, "using", func])
        }
        5 => join(rng, &["align", "using", func]),
        6 => {
            let count = rng.below(2);
            let tags = (0..=count)
                .map(|_| *rng.pick(IDENTS))
                .collect::<Vec<_>>()
                .join(", ");
            join(rng, &["group", "by", &tags, "using", func])
        }
        7 => {
            let args = match rng.below(3) {
                0 => String::new(),
                1 => "le".to_string(),
                _ => "1.0, 2.0".to_string(),
            };
            let dur = gen_duration(rng);
            join(
                rng,
                &[
                    "bucket",
                    "by",
                    ident,
                    "to",
                    &dur,
                    "using",
                    func,
                    &format!("({args})"),
                ],
            )
        }
        8 => {
            let then = gen_filter(rng, 1);
            let otherwise = gen_filter(rng, 1);
            if rng.below(2) == 0 {
                join(rng, &["ifdef", "($v)", "{", "where", &then, "}"])
            } else {
                join(
                    rng,
                    &[
                        "ifdef", "($v)", "{", "where", &then, "}", "else", "{", "where",
                        &otherwise, "}",
                    ],
                )
            }
        }
        _ => {
            let count = rng.below(2);
            let parts = (0..=count)
                .map(|_| {
                    let name = *rng.pick(IDENTS);
                    format!("{name} = {}", gen_expr(rng))
                })
                .collect::<Vec<_>>()
                .join(", ");
            join(rng, &["extend", &parts])
        }
    }
}

/// `comma_terminated` says whether this query is followed by a comma, i.e. whether it sits
/// inside a compute query's argument list. If it is, its last rule may not be `extend`: the
/// rule's own comma-separated parts would swallow the separator — see
/// `extend_swallows_the_comma_of_a_compute_query`.
fn gen_query(rng: &mut Rng, depth: u64, comma_terminated: bool) -> String {
    let count = rng.below(4);
    let mut rules = (0..count).map(|_| gen_rule(rng)).collect::<Vec<_>>();
    if comma_terminated && rules.last().is_some_and(|r| r.starts_with("extend")) {
        let alias = *rng.pick(IDENTS);
        rules.push(join(rng, &["as", alias]));
    }
    let mut query = if depth > 0 && rng.below(4) == 0 {
        let left = gen_query(rng, depth - 1, true);
        let right = gen_query(rng, depth - 1, true);
        let trailing = if rng.below(2) == 0 { "," } else { "" };
        let func = *rng.pick(FUNCS);
        let op = *rng.pick(&["+", "-", "*", "/"]);
        let f = if rng.below(2) == 0 { func } else { op };
        let name = *rng.pick(IDENTS);
        format!(
            "({left}, {right}{trailing}) {}",
            join(rng, &["|", "compute", name, "using", f])
        )
    } else {
        let dataset = *rng.pick(&["d", "`a b`", "$v"]);
        let metric = *rng.pick(IDENTS);
        let mut simple = join(rng, &[dataset, ":", metric]);
        if rng.below(4) == 0 {
            let alias = *rng.pick(IDENTS);
            simple = join(rng, &[&simple, "as", alias]);
        }
        simple
    };
    for rule in rules {
        query = join(rng, &[&query, "|", &rule]);
    }
    query
}

fn gen_program(rng: &mut Rng) -> String {
    let mut out = String::new();
    for _ in 0..rng.below(3) {
        let value = *rng.pick(SCALARS);
        let name = *rng.pick(IDENTS);
        let directive = if rng.below(2) == 0 {
            join(rng, &["set", name, ";"])
        } else {
            join(rng, &["set", name, "=", value, ";"])
        };
        out.push_str(&directive);
        out.push_str(rng.pick::<&str>(TRIVIA));
    }
    for _ in 0..rng.below(3) {
        let tpe = *rng.pick(&[
            "string",
            "int",
            "float",
            "bool",
            "array",
            "Dataset",
            "Duration",
            "Regex",
            "Option<int>",
        ]);
        out.push_str(&join(rng, &["param", "$v", ":", tpe, ";"]));
        out.push_str(rng.pick::<&str>(TRIVIA));
    }
    out.push_str(&gen_query(rng, 2, false));
    out
}

/// Property: everything the generator emits parses cleanly and losslessly.
///
/// This is the strongest statement in the file, because it is quantified over the language
/// rather than over a list: any valid combination of rules, any nesting of compute queries,
/// and trivia in every gap. The kind coverage assertion at the end keeps it honest — a
/// generator that only ever emitted `d:m` would satisfy the property trivially.
#[test]
fn generated_queries_parse_cleanly() {
    let mut rng = Rng(0x5eed_1234_abcd_ef01);
    let mut seen = Vec::new();
    for _ in 0..2000 {
        let src = gen_program(&mut rng);
        let (tree, errors) = Parser::new(&src).parse();
        assert!(
            errors.is_empty(),
            "generated query failed: {src:?}\n{errors:?}"
        );
        assert_eq!(tree.to_string(), src, "generated query did not round trip");
        assert_ranges_match_the_input(&src);
        assert_kind_discipline(&src);
        for element in tree.descendants_with_tokens() {
            let kind = element.kind();
            if !seen.contains(&kind) {
                seen.push(kind);
            }
        }
    }

    // Every production the grammar can reach from a valid query. A kind missing here means
    // the generator stopped covering it, not that the parser changed — but either way the
    // property above went quiet for that production.
    for kind in [
        SyntaxKind::QUERY,
        SyntaxKind::SIMPLE_QUERY,
        SyntaxKind::COMPUTE_QUERY,
        SyntaxKind::DIRECTIVE,
        SyntaxKind::TYPE,
        SyntaxKind::RULE,
        SyntaxKind::FILTER,
        SyntaxKind::FILTER_OR,
        SyntaxKind::FILTER_AND,
        SyntaxKind::FILTER_NOT,
        SyntaxKind::FILTER_PAREN,
        SyntaxKind::FILTER_CMP,
        SyntaxKind::OTEL_TYPE,
        SyntaxKind::REGEX,
        SyntaxKind::ARRAY,
        SyntaxKind::TAG_LIST,
        SyntaxKind::SAMPLE,
        SyntaxKind::MAP,
        SyntaxKind::MAP_MATH,
        SyntaxKind::ALIGN,
        SyntaxKind::AS,
        SyntaxKind::GROUP,
        SyntaxKind::BUCKET,
        SyntaxKind::BUCKET_ARG,
        SyntaxKind::BUCKET_ARGS,
        SyntaxKind::IFDEF,
        SyntaxKind::EXTEND,
        SyntaxKind::EXTEND_PART,
        SyntaxKind::DURATION,
        SyntaxKind::TIME_UNIT,
        SyntaxKind::FUNCTION_PATH,
        SyntaxKind::EXPR,
        SyntaxKind::CONST,
        SyntaxKind::INTEGER,
        SyntaxKind::FLOAT,
        SyntaxKind::BOOL,
        SyntaxKind::STRING,
        SyntaxKind::IDENT,
        SyntaxKind::IDENT_OR_VARIABLE,
        SyntaxKind::VARIABLE,
        SyntaxKind::KEYWORD,
        SyntaxKind::LX_COMMENT,
        SyntaxKind::LX_WHITESPACE,
        SyntaxKind::LX_ESCAPED_IDENT,
        SyntaxKind::LX_INF,
        SyntaxKind::LX_STRING_SEGMENT,
    ] {
        assert!(
            seen.contains(&kind),
            "generated queries never produced {kind:?}"
        );
    }
}

/// Fragments that make near-miss MPL rather than uniform noise: every delimiter that has to
/// balance, every keyword the rule dispatch matches on, and multi-byte characters.
const FRAGMENTS: &[&str] = &[
    "d", ":", "m", "|", "where", "filter", "a", "==", "!=", "1", "(", ")", "[", "]", "{", "}", ",",
    ";", "$v", "set", "param", "compute", "using", "\"", "${", "#/x/", "/", "as", "in", "is",
    "not", "and", "or", "5m", "1.5", "inf", "true", " ", "\n", "::", "map", "align", "to", "group",
    "by", "bucket", "sample", "ifdef", "else", "extend", "=", "é", "🎉", "\\", "`",
];

/// Property: no input, however malformed, panics or loops the parser.
///
/// The generator above only produces valid queries, so it never reaches the recovery paths.
/// This one is the opposite: almost nothing it emits parses, which is exactly what exercises
/// them. What matters is not the errors reported but that a result comes back at all, with
/// spans that a diagnostic renderer can use.
#[test]
fn random_input_does_not_break_the_parser() {
    let mut rng = Rng(0x1234_5678_9abc_def0);
    for _ in 0..3000 {
        let len = usize::try_from(rng.below(20)).unwrap_or(0);
        let mut src = String::new();
        for _ in 0..len {
            src.push_str(rng.pick::<&str>(FRAGMENTS));
        }
        assert_spans_are_in_bounds(&src);
        assert_errors_are_bounded(&src);
        assert_kind_discipline(&src);
        assert_lossless(&src);
        assert_no_token_is_dropped(&src);
    }
}

// ---------------------------------------------------------------------------------------
// Known gaps
//
// Each test below states the behaviour that should hold, so the gap lives in the suite
// instead of in someone's head. None of them asserts the current behaviour where that
// behaviour is a defect: a test that did would report green for as long as the defect
// survived and red the moment it was fixed, which is backwards.
//
// `error_recovery_does_not_duplicate` is the one still open, and it is the one test in this
// file that fails: it is left running rather than `#[ignore]`d so the gap is visible on every
// build instead of only under `--include-ignored`. The other three pass, for three different
// reasons the notes give: arrays in constant position was a real gap and closed, the
// compute-query ambiguity is a property of the language rather than a defect and so is
// asserted normally, and `error_recovery_is_lossless` is green over the call sites that
// consume the token they report.
// ---------------------------------------------------------------------------------------

/// Arrays are reachable from every constant position: `set`, `extend`, a `map` argument, an
/// interpolation body, and nested inside another array. That was not always true —
/// `Parser::constant` used to dispatch an array on `LBrace` (`{`) while `Parser::array`
/// consumes `LBracket` (`[`), which left `where … in [ … ]` the only array that parsed, and
/// only because `filter_cmp` calls `array` directly instead of going through `constant`.
///
/// The five cases below are one per position, so a future rearrangement of `constant` cannot
/// quietly strand a position again.
#[test]
fn arrays_parse_in_constant_position() {
    for src in [
        "set a = [1, 2]; d:m",
        "d:m | extend a = [1, 2]",
        "d:m | map f([1])",
        "d:m | where a in [[1], [2]]",
        "d:m | extend a = \"${ [1] }\"",
    ] {
        let (_tree, errors) = Parser::new(src).parse();
        assert!(errors.is_empty(), "{src:?} should parse: {errors:?}");
    }
}

/// An `extend` as the last rule of a compute query's sub-query eats the comma that separates
/// the sub-queries: `extend` takes a comma-separated list of parts and has no way to know the
/// list ended, so it reads `c` of `, c:d` as another part and fails at the `:`.
///
/// This is an ambiguity in the language, not a mistake in the implementation — the same two
/// commas collide in `mpl.pest` — so it is pinned rather than reported as a bug. Every other
/// comma-carrying rule is safe because its list is closed by something: `group by a, b` ends
/// at `using`, `bucket` at its `)`, and `in [a, b]` at its `]`.
#[test]
fn extend_swallows_the_comma_of_a_compute_query() {
    let ambiguous = "(a:b | extend x = 1, c:d) | compute y using sum";
    let (_tree, errors) = Parser::new(ambiguous).parse();
    assert!(
        !errors.is_empty(),
        "the ambiguity appears to have been resolved"
    );

    // The same shape with any other comma-carrying rule in the same position is fine.
    for src in [
        "(a:b | group by x, y using sum, c:d) | compute y using sum",
        "(a:b | where t in [1, 2], c:d) | compute y using sum",
        "(a:b | bucket by x using f(1.0, 2.0), c:d) | compute y using sum",
    ] {
        parse_clean(src);
    }
}

/// Recovery keeps the token it could not place: `ident`, `keyword`, `variable_type` and
/// `string` route it through `error_token`, which wraps it in an `INVALID` node instead of
/// discarding it, so the tree still reproduces the input. That is what the cases below pin.
///
/// It does not hold at every call site — see `error_recovery_does_not_duplicate` — so the
/// cases here are the ones whose error path consumes the token it reports.
#[test]
fn error_recovery_is_lossless() {
    for src in ["d:m | fflter a == 1", "param $p: nonsense; d:m"] {
        let (tree, errors) = Parser::new(src).parse();
        assert!(!errors.is_empty(), "{src:?} was expected to fail");
        assert_eq!(
            tree.to_string(),
            src,
            "{src:?} lost text during error recovery"
        );
    }
}

/// Three `error_token` call sites report a token they only *peeked*: `filter_cmp`
/// (`src/syntax_tree.rs:897`), `bucket_arg` (`:1022`) and `type_ident` (`:1089`). Each hands
/// the token to `error_token`, which wraps it in an `INVALID` node and emits it — but leaves
/// it in the lexer, so the next production consumes and emits it a second time and the tree
/// comes out longer than the input.
///
/// Ranges are why this matters more than the stray character suggests. Every offset after
/// the duplicate is shifted, so a language server resolving a cursor position against the
/// tree lands in the wrong place, and a formatter printing it emits text the user never
/// wrote. Neither `assert_lossless` nor `assert_ranges_match_the_input` can catch it: both
/// bail out on any input that produced an error, which is every input that reaches these
/// sites. The half that does hold — that no token goes *missing* — is checked on every
/// input by `assert_no_token_is_dropped`.
///
/// Left failing rather than inverted or ignored. Asserting that `d:m | where a ~ 1` renders
/// as `d:m | where a ~~ 1` would make the defect the contract and go red the day it is
/// fixed; `#[ignore]` would hide it from every build that does not pass
/// `--include-ignored`. A red test names the gap on every run, which is the point.
///
/// The fix is for those three sites to consume what they report. When they do, this goes
/// green and the `errors.is_empty()` guard in `assert_lossless` can go with it — it is the
/// same property stated for clean parses only.
#[test]
fn error_recovery_does_not_duplicate() {
    // Second column is what the tree renders today, so a failure reads as a diff rather than
    // as a bare inequality.
    for src in [
        "d:m | where a ~ 1",
        "d:m | bucket using histogram(1)",
        "d:m | where a is nonsense",
    ] {
        let (tree, errors) = Parser::new(src).parse();
        assert!(!errors.is_empty(), "{src:?} was expected to fail");
        assert_eq!(
            tree.to_string(),
            src,
            "{src:?} duplicated a token during recovery (renders as {src:?})"
        );
    }
}
