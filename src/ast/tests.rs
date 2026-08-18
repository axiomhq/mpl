//! Tests for lowering the syntax tree to the AST.
//!
//! A child module rather than a file under `tests/`, because what is worth pinning here is
//! per-production: the value one literal carries, the parts one interpolation splits into.
//! Reaching those from outside the crate would mean walking `parts()` through half a dozen
//! private fields, so the cases call the lowering functions directly and read the values they
//! return.
//!
//! `lower_first` is the shared shape: parse a query, find the one node the case is about, and
//! hand it to the function under test. It asserts the parse was clean first, so a case that
//! fails is a lowering failure and never a typo in the query wrapped around it.
use super::*;
use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource, Report};
use test_case::test_case;

/// Renders the parser's errors the way a user would see them, so a failing example prints a
/// diagnostic rather than a debug dump.
fn report(name: &str, content: &str, errors: &[&AstError]) -> String {
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    let mut out = String::new();
    for error in errors {
        let diagnostic = error.to_diagnostic();
        let report =
            Report::new(diagnostic).with_source_code(NamedSource::new(name, content.to_string()));
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

/// Parses `src` and lowers its first node of `kind` with `f`, so a case can name one
/// production without restating the query that has to surround it.
fn lower_first<T>(
    src: &str,
    kind: SyntaxKind,
    f: impl FnOnce(&mut Parser, SyntaxNode) -> Result<T>,
) -> T {
    let mut parser = Parser::new(src);
    assert!(
        parser.errors.is_empty(),
        "{src:?} did not parse: {:?}",
        parser.errors
    );
    let node = parser
        .root
        .descendants()
        .find(|n| n.kind() == kind)
        .unwrap_or_else(|| panic!("no {kind:?} node in {src:?}"));
    match f(&mut parser, node) {
        Ok(v) => v,
        Err(Error(why)) => panic!("{src:?} did not lower: {why}: {:?}", parser.errors),
    }
}

/// What lowering `src`'s first node of `kind` reported, rendered as the diagnostics a user
/// would see. Separate from `lower_first` because a lowering that reports and still returns a
/// value is invisible to a case that only looks at what came back.
fn lower_diagnostics<T>(
    src: &str,
    kind: SyntaxKind,
    f: impl FnOnce(&mut Parser, SyntaxNode) -> Result<T>,
) -> Vec<String> {
    let mut parser = Parser::new(src);
    assert!(
        parser.errors.is_empty(),
        "{src:?} did not parse: {:?}",
        parser.errors
    );
    let node = parser
        .root
        .descendants()
        .find(|n| n.kind() == kind)
        .unwrap_or_else(|| panic!("no {kind:?} node in {src:?}"));
    drop(f(&mut parser, node));
    parser.errors.iter().map(ToString::to_string).collect()
}

/// The characters lowering flagged as unknown escapes. The character rather than the
/// diagnostic, because every one of these renders the same sentence and only the character
/// says which sequence was met.
fn unknown_escapes<T>(
    src: &str,
    kind: SyntaxKind,
    f: impl FnOnce(&mut Parser, SyntaxNode) -> Result<T>,
) -> Vec<char> {
    let mut parser = Parser::new(src);
    assert!(
        parser.errors.is_empty(),
        "{src:?} did not parse: {:?}",
        parser.errors
    );
    let node = parser
        .root
        .descendants()
        .find(|n| n.kind() == kind)
        .unwrap_or_else(|| panic!("no {kind:?} node in {src:?}"));
    drop(f(&mut parser, node));
    parser
        .warnings
        .iter()
        .filter_map(|w| match w {
            AstWarning::UnknownEscapeSequence { char, .. } => Some(*char),
            AstWarning::TimeNotSecondAligned { .. } => None,
        })
        .collect()
}

/// The characters a string literal flags, for a body wrapping each escape in `x…y`.
fn string_unknown_escapes(escape: char) -> Vec<char> {
    let src = format!("set a = \"x{}{escape}y\"; d:m", '\\');
    unknown_escapes(&src, SyntaxKind::STRING, |p, n| p.string_const(&n))
}

/// The characters an escaped identifier flags, for the same body shape.
fn ident_unknown_escapes(escape: char) -> Vec<char> {
    let src = format!("`x{}{escape}y`:m", '\\');
    unknown_escapes(&src, SyntaxKind::IDENT, Parser::ident)
}

/// The value a string literal carries once its quotes and escapes are resolved.
fn string_value(literal: &str) -> String {
    let src = format!("set a = {literal}; d:m");
    match lower_first(&src, SyntaxKind::STRING, |p, n| p.string_const(&n)) {
        TagValue::String(s) => s.to_string(),
        other => panic!("{literal} lowered to {other:?}"),
    }
}

/// The pattern a regex literal carries once its delimiters are removed.
fn regex_pattern(literal: &str) -> String {
    let src = format!("d:m | where a == {literal}");
    lower_first(&src, SyntaxKind::REGEX, |p, n| p.regex(&n))
        .0
        .as_str()
        .to_string()
}

/// The name an identifier carries once its backticks and escapes are resolved.
fn ident_name(literal: &str) -> String {
    let src = format!("{literal}:m");
    lower_first(&src, SyntaxKind::IDENT, Parser::ident).to_string()
}

/// The name a variable carries once its sigil, backticks and escapes are resolved.
fn variable_name(literal: &str) -> String {
    let src = format!("param {literal}: string; d:m");
    lower_first(&src, SyntaxKind::VARIABLE, Parser::variable).to_string()
}

/// The parts of an interpolated string: constants quoted, variables in the form that refers
/// to them, so a `$` that leaked into a name shows up as a doubled sigil rather than passing
/// for the reference marker.
fn string_parts(literal: &str) -> String {
    let src = format!("d:m | extend x = {literal}");
    let SyntaxExpr {
        expr: Expr::String(parts),
        ..
    } = lower_first(&src, SyntaxKind::STRING, Parser::string_expr)
    else {
        panic!("{literal} did not lower to an interpolated string")
    };
    parts
        .iter()
        .map(|p| match p {
            StringPart::Const(s) => format!("{s:?}"),
            StringPart::Expr(SyntaxExpr {
                expr: Expr::Var(v), ..
            }) => format!("${v}"),
            StringPart::Expr(SyntaxExpr { expr, .. }) => format!("{expr:?}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every sequence built from `pieces`, up to `depth` long.
fn sequences(pieces: &[&str], depth: usize) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = vec![vec![]];
    let mut all = Vec::new();
    for _ in 0..depth {
        out = out
            .iter()
            .flat_map(|prefix| {
                pieces.iter().map(move |p| {
                    let mut next = prefix.clone();
                    next.push((*p).to_string());
                    next
                })
            })
            .collect();
        all.extend(out.iter().cloned());
    }
    all
}

// ---------------------------------------------------------------------------------------
// String escapes
//
// A backslash escape resolves to exactly one character: the five control-character names map
// to their control character, the three a string has to be able to spell — `\\`, `\"`, `\$` —
// stand for themselves, and anything else stands for itself and is flagged. `\b` is a
// backspace here; it means a word boundary only to the regex engine, which is why a regex is
// not routed through this at all.
// ---------------------------------------------------------------------------------------

#[test_case(r#""plain""#  => "plain"     ; "a literal with no escapes")]
#[test_case(r#""a\nb""#   => "a\nb"      ; "newline")]
#[test_case(r#""a\tb""#   => "a\tb"      ; "tab")]
#[test_case(r#""a\rb""#   => "a\rb"      ; "carriage return")]
#[test_case(r#""a\bb""#   => "a\u{8}b"   ; "backspace")]
#[test_case(r#""a\fb""#   => "a\u{c}b"   ; "form feed")]
#[test_case(r#""a\\b""#   => r"a\b"      ; "backslash")]
#[test_case(r#""a\$b""#   => "a$b"       ; "dollar")]
#[test_case(r#""a\qb""#   => "aqb"       ; "an unnamed escape stands for its character")]
#[test_case(r#""it's""#   => "it's"      ; "an apostrophe is an ordinary character")]
#[test_case(r#""a\"b""#   => "a\"b"      ; "quote")]
#[test_case(r#""a\"""#    => "a\""       ; "quote against the terminator")]
#[test_case(r#""\"""#     => "\""        ; "a literal that is one escaped quote")]
fn string_escapes(literal: &str) -> String {
    string_value(literal)
}

// ---------------------------------------------------------------------------------------
// Escaped identifiers
//
// Backticks carry a name the bare form cannot spell — one holding a space, a keyword, or a
// character the lexer would end the identifier on — so what an identifier lowers to is the
// text between them, with the same escape rule a string body gets. The quoting is a way of
// writing the name, not part of it: `` `sum` `` and `sum` are the same identifier.
// ---------------------------------------------------------------------------------------

#[test_case("plain"     => "plain"   ; "a bare identifier is its own text")]
#[test_case("`plain`"   => "plain"   ; "backticks are not part of the name")]
#[test_case("`a b`"     => "a b"     ; "a space, which the bare form cannot hold")]
#[test_case("`where`"   => "where"   ; "a rule name, which the bare form would be read as")]
#[test_case(r"`a\nb`"   => "a\nb"    ; "newline")]
#[test_case(r"`a\tb`"   => "a\tb"    ; "tab")]
#[test_case(r"`a\\b`"   => r"a\b"    ; "backslash")]
#[test_case(r"`a\`b`"   => "a`b"     ; "backtick")]
#[test_case(r"`a\qb`"   => "aqb"     ; "an unnamed escape stands for its character")]
#[test_case(r"`a\``"    => "a`"      ; "backtick against the terminator")]
fn ident_escapes(literal: &str) -> String {
    ident_name(literal)
}

/// `\uXXXX` is a sequence the grammar spells (`src/mpl.pest:9`, `:25`) and this lowering does
/// not resolve, so it is reported. Asserted through the diagnostic rather than the value,
/// because lowering hands back a string either way: a caller that reads the value without
/// reading `errors()` sees a plausible name and no sign that four hex digits were meant to be
/// one character.
/// Both alphabets spell it, and an identifier reaches the check by its own route through
/// `unescape_ident`, so each is stated separately.
#[test]
fn a_unicode_escape_is_reported() {
    let bs = '\\';
    assert_eq!(
        lower_diagnostics(
            &format!("set a = \"{bs}u0041\"; d:m"),
            SyntaxKind::STRING,
            |p, n| p.string_const(&n)
        ),
        ["unicode escape sequences are not supported"],
        "a string resolved a unicode escape silently"
    );
    assert_eq!(
        lower_diagnostics(&format!("`{bs}u0041`:m"), SyntaxKind::IDENT, Parser::ident),
        ["unicode escape sequences are not supported"],
        "an escaped identifier resolved a unicode escape silently"
    );
    assert_eq!(
        lower_diagnostics(
            &format!("param $`{bs}u0041`: string; d:m"),
            SyntaxKind::VARIABLE,
            Parser::variable
        ),
        ["unicode escape sequences are not supported"],
        "an escaped variable resolved a unicode escape silently"
    );
}

/// An escape outside the alphabet still yields its character, so the value alone cannot say
/// whether the sequence was understood — the warning is what distinguishes `\q` from `q`, and
/// it carries the character because every one of these renders the same sentence.
///
/// `/` is here to state that an alphabet is judged on its own: a slash needs no escaping in
/// either of these, and that it carries meaning to the regex engine is the regex alphabet's
/// business. Reading `\/` as known here would equally argue for reading `` \` `` as known in
/// a string, because a backtick means something to an identifier.
#[test_case('q' ; "a character with no meaning anywhere")]
#[test_case('/' ; "a character with meaning in another alphabet")]
fn an_escape_outside_the_alphabet_is_flagged(escape: char) {
    assert_eq!(
        string_unknown_escapes(escape),
        [escape],
        "silent in a string"
    );
    assert_eq!(
        ident_unknown_escapes(escape),
        [escape],
        "silent in an identifier"
    );
}

/// The two alphabets are not the same set: a string has to spell `\$` and never needs to
/// spell a backtick, an identifier the reverse. Stated as the crossing pairs because that is
/// what one shared implementation would get wrong — a single alphabet would leave each
/// accepting the other's escape.
#[test]
fn the_two_alphabets_differ_where_their_delimiters_do() {
    assert_eq!(string_unknown_escapes('$'), [], r"a string flagged \$");
    assert_eq!(ident_unknown_escapes('`'), [], r"an identifier flagged \`");
    assert_eq!(string_unknown_escapes('`'), ['`']);
    assert_eq!(ident_unknown_escapes('$'), ['$']);
}

/// The escapes both alphabets share pass without comment.
#[test_case('n' ; "newline")]
#[test_case('t' ; "tab")]
#[test_case('r' ; "carriage return")]
#[test_case('b' ; "backspace")]
#[test_case('f' ; "form feed")]
#[test_case('\\' ; "backslash")]
fn a_shared_escape_is_silent(escape: char) {
    assert_eq!(string_unknown_escapes(escape), [], "flagged in a string");
    assert_eq!(
        ident_unknown_escapes(escape),
        [],
        "flagged in an identifier"
    );
}

/// Quoting a name that never needed quoting changes nothing: for every identifier that is
/// already spellable bare, wrapping it in backticks lowers to the same name. Stated as a
/// relation rather than a table of pairs because it is the property that makes the two forms
/// interchangeable at every site an identifier is accepted, which no single pair shows.
#[test]
fn backticks_around_a_bare_identifier_are_inert() {
    for bare in ["a", "sum", "where", "_x", "a_b", "é"] {
        assert_eq!(
            ident_name(&format!("`{bare}`")),
            ident_name(bare),
            "`{bare}` and {bare} are not the same identifier"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Escaped variables
//
// A variable names a parameter, and the name is what a caller supplies: `ProvidedParam`
// carries `"dataset"`, and resolution matches it against the declaration by string. The `$`
// is how the source marks a parameter reference, so it is punctuation rather than part of
// the name, and the backticks are the same quoting an identifier gets.
// ---------------------------------------------------------------------------------------

#[test_case("$plain"    => "plain"   ; "a bare variable is its text without the sigil")]
#[test_case("$`plain`"  => "plain"   ; "backticks are not part of the name")]
#[test_case("$`a b`"    => "a b"     ; "a space, which the bare form cannot hold")]
#[test_case("$`where`"  => "where"   ; "a rule name, which the bare form would be read as")]
#[test_case(r"$`a\nb`"  => "a\nb"    ; "newline")]
#[test_case(r"$`a\tb`"  => "a\tb"    ; "tab")]
#[test_case(r"$`a\\b`"  => r"a\b"    ; "backslash")]
#[test_case(r"$`a\`b`"  => "a`b"     ; "backtick")]
#[test_case(r"$`a\qb`"  => "aqb"     ; "an unnamed escape stands for its character")]
#[test_case(r"$`a\``"   => "a`"      ; "backtick against the terminator")]
fn variable_escapes(literal: &str) -> String {
    variable_name(literal)
}

/// The two spellings of a variable name the same parameter. This is what makes a declaration
/// and a reference match: `param $p` and a later `` $`p` `` have to agree on the string, or a
/// parameter is declared under one name and looked up under another and never resolves.
///
/// Paired with the identifier relation rather than stated alone, because the name a variable
/// carries is the name an identifier would carry — the sigil marks the reference, and does
/// not belong to what is referenced.
#[test]
fn both_spellings_of_a_variable_name_the_same_parameter() {
    for bare in ["p", "dataset", "where", "_x", "a_b", "é"] {
        let plain = variable_name(&format!("${bare}"));
        assert_eq!(plain, variable_name(&format!("$`{bare}`")));
        assert_eq!(plain, ident_name(bare), "a $ leaked into the name");
    }
}

/// One escape resolves to one character, whatever follows the backslash. Stated as a count
/// rather than a value because that is the part no individual case can pin: a table checks
/// the escapes someone thought of, while the count catches a body that silently gained or
/// lost a character — a swallowed trailing backslash, or a delimiter stripped more times than
/// it appears.
#[test]
fn an_escape_resolves_to_exactly_one_character() {
    let pieces = ["a", " ", "é", r"\n", r"\t", r"\\", "\\\"", r"\$", r"\q"];
    let cases = sequences(&pieces, 3);
    assert_eq!(cases.len(), 9 + 81 + 729, "the generator lost coverage");
    for parts in cases {
        let body = parts.concat();
        let value = string_value(&format!("\"{body}\""));
        assert_eq!(
            value.chars().count(),
            parts.len(),
            "{body:?} lowered to {value:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Regex literals
//
// The body reaches `regex` exactly as written: Rust's engine owns this alphabet, decodes
// every sequence below itself, and reports the ones it does not recognise. Decoding here
// first would strip the backslash that carries the meaning — `\.` would match any character
// and `\d` the letter `d` — so the only work this layer does is remove `#/` and the closing
// `/`.
// ---------------------------------------------------------------------------------------

#[test_case("#/plain/"     => "plain"      ; "a literal with no escapes")]
#[test_case(r"#/a\.b/"     => r"a\.b"      ; "an escaped metacharacter keeps its backslash")]
#[test_case(r"#/\d+/"      => r"\d+"       ; "a perl class keeps its backslash")]
#[test_case(r"#/\bword\b/" => r"\bword\b"  ; "a word boundary is not a backspace")]
#[test_case(r"#/\p{L}/"    => r"\p{L}"     ; "a unicode class keeps its backslash")]
#[test_case(r"#/a\\b/"     => r"a\\b"      ; "an escaped backslash keeps both characters")]
#[test_case(r"#/a\/b/"     => r"a\/b"      ; "an escaped delimiter mid pattern")]
#[test_case(r"#/\/api\//"  => r"\/api\/"   ; "an escaped delimiter against both ends")]
#[test_case("#/#/"         => "#"          ; "a body that repeats the opening delimiter")]
fn regex_literals(literal: &str) -> String {
    regex_pattern(literal)
}

/// The pattern that reaches the regex engine is the source between the delimiters, byte for
/// byte. Stated over generated bodies because the failure it guards is positional: stripping
/// the delimiters with a repeating trim eats a second `/` only when the body happens to end
/// in an escaped one, which no fixed table is likely to contain.
///
/// Compiling each pattern is the other half. Equality alone would still pass if the
/// delimiters were removed correctly and the result were nonsense to the engine; every piece
/// here is a valid pattern, and concatenating valid patterns leaves one valid, so anything
/// this layer does to the text shows up as a compile error.
#[test]
fn a_regex_reaches_the_engine_unchanged() {
    let pieces = ["a", r"\.", r"\d", r"\/", r"\\", r"\bx\b", "[a-z]", "#"];
    let cases = sequences(&pieces, 3);
    assert_eq!(cases.len(), 8 + 64 + 512, "the generator lost coverage");
    for parts in cases {
        let body = parts.concat();
        let pattern = regex_pattern(&format!("#/{body}/"));
        assert_eq!(pattern, body, "{body:?} did not survive lowering");
        assert!(
            regex::Regex::new(&pattern).is_ok(),
            "{body:?} lowered to a pattern the engine rejects: {pattern:?}"
        );
    }
}

/// Interpolation is a run of tokens, not one: `"` to the first `${` opens it, every `}` to
/// the next `${` continues it, and the last `}` to `"` closes it. A single interpolation
/// never produces the middle shape, so the continuation is only reachable from two, and the
/// constant either side of an expression is empty when the expression sits flush against a
/// delimiter.
#[test]
fn a_string_interpolates_more_than_once() {
    assert_eq!(
        string_parts(r#""${$host}/${$path}""#),
        r#""" $host "/" $path """#
    );
    assert_eq!(string_parts(r#""a${$x}b${$y}c""#), r#""a" $x "b" $y "c""#);
}

/// The filter chain is the one place in the lowering where "no operands" is a truth value
/// instead of a parse failure: an empty conjunction holds for every row and an empty
/// disjunction for none. A `FILTER_OR` or `FILTER_AND` whose only child is an `INVALID` node
/// is indistinguishable from one with no operands, because `INVALID` is trivia to the
/// lowering — so both have to be refused. Accepting them would turn an over-nested `where`
/// into a filter that silently matches every row.
///
/// The recursion cap is the reachable way to produce such a node: it drops the production it
/// was about to build and leaves an `INVALID` node in the parent. Which of the four filter
/// productions that lands on is fixed by the cap's residue, and a parenthesis costs exactly
/// four nodes — so varying the nesting cannot move it and the sweep varies the cap instead.
/// Both shapes are counted, so the test cannot pass by never reaching the case.
#[test]
fn a_capped_filter_never_lowers_to_an_empty_operand_list() {
    let src = format!("d:m | where {}a == 1{}", "(".repeat(40), ")".repeat(40));
    let mut empty_ors = 0_usize;
    let mut empty_ands = 0_usize;
    for cap in 20..=40 {
        let SyntaxTree { root, errors } =
            syntax_tree::Parser::new(&src).with_tree_depth(cap).parse();
        assert!(
            !errors.is_empty(),
            "a cap of {cap} left the filter uncapped"
        );
        let mut parser = Parser {
            root,
            errors: errors.into_iter().map(AstError::InvalidSyntax).collect(),
            warnings: Vec::new(),
            parts: Vec::new(),
        };
        let nodes = parser.root.descendants().collect::<Vec<_>>();
        for node in nodes {
            let mut children = node.children();
            if children.n().is_some() {
                continue;
            }
            match node.kind() {
                SyntaxKind::FILTER_OR => {
                    empty_ors += 1;
                    assert!(
                        parser.filter_or(&node).is_err(),
                        "a FILTER_OR with no operands lowered at a cap of {cap}"
                    );
                }
                SyntaxKind::FILTER_AND => {
                    empty_ands += 1;
                    assert!(
                        parser.filter_and(&node).is_err(),
                        "a FILTER_AND with no operands lowered at a cap of {cap}"
                    );
                }
                _ => {}
            }
        }
    }
    assert!(
        empty_ors > 0,
        "the sweep never produced a FILTER_OR to check"
    );
    assert!(
        empty_ands > 0,
        "the sweep never produced a FILTER_AND to check"
    );
}

#[test]
fn test_ast_parse() {
    let input = r#"
        // test
        set a = 43;
        set b;
        set c = 1.2;
        set d = [1, 2, "snot", [42.0, []]];
        param $test: string;
        param $test2: Option<string>;
        a:b as c
        | where code == #/[123]../
        | ifdef ($test2) { where value == $test }
        | ifdef ($test2) { where value == $test } else { where value == "" }
        | map filter::gt(1)
        | map * 2
        | align using avg
        | align to 1m using prom::rate
        | align to $duration using sum
        | extend a = 1, b = "gobble", c = "hello ${ $world } snot { $badger }"
        "#;
    let parser = Parser::new(input);
    let ast = parser.lower();
    for error in &ast.errors {
        eprintln!("{}", report("test", input, &[error]));
    }
    assert!(ast.errors.is_empty());
    dbg!(&ast.parts);
}

#[test]
fn test_e_notation() {
    let input = r"
        `xx-xx-xx`:`xx_xx.xx_xx_xx`
        | map / 1.09951163e12
        | align to $__interval using avg
        | group using avg
        ";
    let parser = Parser::new(input);
    let ast = parser.lower();
    for error in &ast.errors {
        eprintln!("{}", report("test", input, &[error]));
    }
    assert!(ast.errors.is_empty());
    dbg!(&ast.parts);
}
