# Open items — RW (recursive descent + `rowan` CST), after full pest removal

**Status:** the representative slice was completed in phase 2. The **full** MPL
grammar is ported, **pest is removed** (`rg 'pest' --type toml` → none;
`src/mpl.pest` deleted), and all **669** workspace tests pass (668 + the new
interpolation round-trip test). There are **no skipped grammar constructs** and
**no `SKIPPED(step2)` markers** left in the code.

This file used to track slice gaps. After the full port it tracks the **real
residuals**. Authoritative detail lives in `REPORT.md` → *"Phase 2 — full pest
removal"*. Effort tags are relative scope (XS/S/L), never time.

## Resolved editor grammar-duplication (surgical follow-up)

### 0. ~~JS-side `param`/`plain_ident` grammar in `mpl-codemirror`~~ — **FIXED**
The last grammar fragments in the CodeMirror package's TypeScript are gone, so
production TS now encodes **no** MPL grammar:
- `hover.ts` param hover no longer hand-parses `param $name: T;` /
  `Option<T>` (deleted `PARAM_LINE_RE` + `OPTION_RE`). `parseParamDeclarations`
  now calls the new wasm export `param_declarations`, which **reuses** the
  completion engine's `extract_declared_params` / `parse_param_decl` scanner
  (`completions.rs`) via a small `declared_params` projection.
- `completions.ts` no longer carries `PLAIN_IDENT_RE`; `needsEscape` calls the
  new wasm export `is_plain_ident`, backed by `mpl_lang::is_plain_ident` (the
  same predicate `escape_ident` uses for `Display`, deduped). This also fixed a
  live drift: the JS regex rejected the leading-`_` identifiers the grammar
  accepts.
- `KEYWORD_DOCS` deliberately **stays in TS** — it is static hover help copy
  (descriptions + example syntax), not grammar.

Detail: `REPORT.md` → *"Addendum — last JS grammar duplication removed"*. Pinned
by Rust tests (`is_plain_ident`, `declared_params`) and TS adapter tests (wasm
boundary mocked). `rg 'PARAM_LINE_RE|OPTION_RE|PLAIN_IDENT_RE' packages` → none.

## Residual open items

### 1. ~~Editor string-interpolation sub-highlighting~~ — **FIXED**
The lexer now **descends** into `${ … }` (see `cst::parser::expand_string` /
`lex_interp`): a string literal becomes `STRING_FRAGMENT` tokens plus the
embedded expression as a real `EXPR` subtree parsed by the **same** `expr()`
parser. Highlighting then sub-tokenizes automatically via the existing
`SyntaxKind → TokenType` walk — `"Hello ${ name }!"` highlights as
`String("Hello ) · Variable(name) · String(!")`, and an embedded number lights
up as `Number`. The opaque-token re-parse in lowering (`lower_string`
byte-scanner + the `lower_interp_expr` second grammar) is deleted; lowering now
reads the fragments off the tree (~104 → 37 lines). The CST is byte-for-byte
**lossless** down into interpolations (incl. nested/escaped/empty fragments),
which **unblocks the trivia-preserving formatter**. Pinned by the rewritten
`tokenize::tests::string_interpolation_*_is_sub_tokenized_in_slice` tests and the
new `cst::tests::interpolated_string_roundtrips_losslessly` round-trip test.

### 1b. ~~String-interpolation boundary bug (byte scanner blind to backtick/regex/comments)~~ — **FIXED** (Option B, token-driven)
The interpolation *boundary* detection used to byte-scan (`string_end` /
`find_interp_close`) knowing only `\` and `"`, so it mis-detected the `${ … }`
boundary whenever the interior carried a `}` or `"` inside a backtick ident
(`` `a}b` ``), a `#/regex/` literal or a `// comment` (e.g.
`ds:cpu | where t == "x ${ `a}b` }"` produced 3 spurious errors). **Fixed by
making boundary detection token-driven:** `lex_interp` now lexes each `${ … }`
interior with the normal `logos` lexer and finds the closing `}` by **counting
brace tokens**, so those constructs are single tokens and their inner `}`/`"`
can never be miscounted. `string_end` and `find_interp_close` are **deleted**;
the `unterminated string` diagnostic is now derived from the lexer reaching EOF
still in string/interpolation mode (recorded in `Parser::unterminated`) instead
of a re-scan. Variant chosen: the **hand-rolled mode stack over the `logos`
token stream** (the Rust call stack *is* the mode stack) rather than
`logos::morph` + `Extras`, for fewer moving parts. The previously-`#[ignore]`d
lock `cst::tests::interpolation_with_braced_escaped_ident_parses_cleanly` is
un-ignored and passes; regression tests (a) quoted escaped ident and (b)
multi-line `//`-comment boundary added. Detail: `REPORT.md` → *"Addendum —
string-interpolation boundary made token-driven (Option B)"*.

### 2. ~~Completions cursor engine still hand-rolled~~ — **CLOSED** (fully CST-driven)
The completion *position-detection* layer now derives the cursor's place in the
query structure entirely from the recovering rowan CST
(`cst::parse(query).syntax()`) — **all 5 cursor contexts**, with **no byte
scanner left** in `completions.rs`. CST-driven: `locate_query_context`
(compute-query nesting via `COMPUTE_QUERY` nodes), `find_last_pipe`/`count_pipes`
(via `PIPE` tokens), `extract_source_info` (via the `METRIC_ID` node),
`extract_partial_word` (via the word-token run at the cursor), and — the final
holdout — `classify_string_context` (code vs `${ }` interpolation vs plain
string text, via the innermost `STRING` node + its `STRING_FRAGMENT` children).

**What closed the holdout:** the lexer/CST now recovers an *unterminated* string
into the **same** interior structure it builds for a closed one. `lex_range`
routes the `ERROR` token an open string produces through the existing
`expand_string`/`string_end`/`find_interp_close` machinery (no second lexer);
`string_end` returns `(end, terminated)` and `Parser::string` emits an
`unterminated string` diagnostic over the full extent (so `compile` still
rejects it). `"a ${ b` now carries `STRING_FRAGMENT`/`DOLLAR_BRACE`/embedded
`EXPR`, so highlighting and the completion classifier work mid-edit.

The `skip_backtick` production helper and the `#[cfg(test)]` byte utilities
(`is_char_escaped`, `skip_string_literal`, `skip_interpolation`) + their 8
white-box unit tests are **deleted**. The `cursor_in_interpolation_*` tests are
**re-pointed** at the new CST classifier (behavior coverage preserved), and 3
new mid-edit tests were added. The `suggest_*` builders and stdlib data layer
remain untouched. Net **−92** lines in `completions.rs`. All workspace tests
green (667; −8 deleted helper unit tests, +3 new). Detail: `REPORT.md` →
*"Addendum — completions fully CST-driven (last byte scanner retired)"*.

### 3. Pre-existing, pest-unrelated build bug — `XS`
`mpl-language-server`'s `examples`-feature `query_spec()` references the private
path `mpl_lang::stdlib::STDLIB` (should be `mpl_lang::STDLIB`), so it does not
build under `--features examples`. Untouched by the port; default and wasm
builds never enable that feature.
