# Open items — CH (`chumsky` combinators), after full pest removal

**Status:** the **full** MPL grammar is ported, **pest is removed**
(`rg 'pest' --type toml` → none; `src/mpl.pest` deleted), and all **659**
workspace tests pass (666 before Phase A; **+3** for `is_plain_ident` /
`declared_params`, **−10** for the dead `parser.rs` self-tests removed in the
Phase A hygiene pass — see *Resolved* below). There are **no skipped grammar
constructs** and **no `SKIPPED(step2)` markers** left in the code.

This file used to track slice gaps. After the full port it tracks the **real
residuals**. Authoritative detail lives in `REPORT.md` → *"Phase 2 — full pest
removal"*.

## Resolved

### Editor grammar fragments moved into Rust + dead-lexer removed (Phase A)
**Tier-1 RW parity.** The two remaining editor-side grammar fragments are now
parsed in Rust and surfaced over wasm — the editor no longer re-implements any
grammar rule:
- **`param $x: T;` hover scan** — `hover.ts` deleted `PARAM_LINE_RE`/`OPTION_RE`;
  it now calls the new wasm `param_declarations`, backed by
  `mpl_language_server::declared_params` (reuses the existing
  `extract_declared_params` scanner — no new scanner).
- **backtick-escape decision** — `completions.ts` deleted `PLAIN_IDENT_RE`;
  `needsEscape` now calls the new wasm `is_plain_ident`, backed by
  `mpl_lang::is_plain_ident`, which reuses the chumsky lexer's own
  `slice::is_ident_start` / `is_ident_continue` classes. `escape_ident` routes
  through it too (one definition). This **fixes a real drift bug**: the old JS
  regex `^[A-Za-z]…` rejected leading-underscore idents the grammar accepts.
- **`KEYWORD_DOCS`** in `hover.ts` is intentionally **kept** — presentation copy,
  not grammar.
- **Hygiene**: deleted `extra/mpl-language-server/src/parser.rs` (571 lines) — a
  superseded hand-rolled lexer reachable only from its own `#[cfg(test)]` tests
  and hidden behind the workspace's only `#![allow(dead_code)]`. Verified dead
  via `ra references`. Production tokenizing is `tokenize.rs`.

Detail in `REPORT.md` → *"Phase A — Tier-1 RW parity"*. This **resolves residual
item #4 below** (TS-side non-grammar leftovers — only `KEYWORD_DOCS` remains, by
design).

### String-interpolation highlighting — sub-tokenization (Phase 2.1)
**Highlighting only.** The parser's `string_expr` always descended into `${ … }`
and built `StringFragment::{Text,Expr}` for the AST, but the highlight lexer
(`src/slice.rs` `highlighter()`) used to emit the whole `"…${ expr }…"` literal as
one opaque `String` token. `highlighter()` is now `recursive` and re-enters the
**same** token set inside `${ … }` (reusing `classify_word`/`param`/`number`, no
second grammar), so `"host ${ $h } end"` highlights as `String("\"host ")`,
`Variable($h)`, `String(" end\"")` — the 3-token model. The lexer stays total, so
mid-edit input (`"a ${ $b`, `"${`, `"${x}${y}"`) never panics. **This is a
highlighting correctness fix, NOT a formatter win:** CH has no lossless CST, so
the trivia-preserving-formatter benefit the RW port got does not transfer here.
Detail in `REPORT.md` → *"Phase 2.1 — string-interpolation highlighting"*.

## Residual open items

### 1. Completions cursor byte-scanner kept (irreducible) — `L`
The cursor-context engine (`locate_query_context`, `classify_string_context`,
`extract_partial_word`, `skip_literal`, …) answers "what is under / before the
cursor in this incomplete, mid-edit string". chumsky's recovery yields a *tree*,
not a cursor-relative position map, so it cannot drive this. Only the one place
completions actually invoked pest — `extract_source_via_parser` (~52 lines
walking `Rule::source`) — was removed, replaced by a ~17-line `slice::parse`
call. The scanner stays.

### 2. Verbose typed-error carrier — accepted cost
Matching pest's typed `ParseError` variants **and** exact spans required carrying
semantic errors verbatim in chumsky's `Rich` *context* slot — deliberately
verbose. Localized and documented; the price of error-message parity.

### 3. chumsky check-mode footgun (maintenance watch-item)
Stateful rules (`directive` / `param_decl` mutating `SimpleState`) must stay on
the **emit** path of `file()` (`…then(query)`), never on an `ignore_then` /
`then_ignore` *ignored* side — otherwise state pushes are silently skipped in
check mode. **Any future rule addition must respect this.** This is the main
maintainability hazard CH carries.

### 4. TS-side non-grammar leftovers — RESOLVED (Phase A)
~~`language.ts` has zero grammar; what remains is a hover-only param-decl scan
(`PLAIN_IDENT_RE`) and `KEYWORD_DOCS`.~~ **Resolved**: the param-decl scan
(`PARAM_LINE_RE`/`OPTION_RE`) and the escape regex (`PLAIN_IDENT_RE`) are now in
Rust behind the `param_declarations` / `is_plain_ident` wasm exports (single
source of truth, drift bug fixed). Only `KEYWORD_DOCS` remains in `hover.ts` —
presentation copy, intentionally not migrated. See *Resolved → Phase A* above.
