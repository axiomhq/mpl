# Open items — WI (`winnow` combinators), after full pest removal

**Status:** the **full** MPL grammar is ported, **pest is removed**
(`rg 'pest' --type toml` → none; `src/mpl.pest` deleted), all **657** workspace
tests pass, plus the wasm artifact test (`node tests/wasm/test-wasm.mjs`) at
**38/38**. There are **no skipped grammar constructs** and **no `SKIPPED(step2)`
markers** left in the code.

## Resolved

### String-interpolation highlighting — fixed (highlighting only)
The grammar always split interpolated literals into `StringFragment`s at the AST
level (`grammar::string_expr`), but the **highlight** lexer used to emit the
whole `"…${ expr }…"` literal as **one opaque `String` token**. `lex.rs` now
descends into `${ … }` (`lex_string`), reusing the same run-split logic as
`string_expr` and the existing `single_token` classifier for the embedded
expression, so `"host ${ $h } end"` highlights as `String("\"host ")`,
`Variable("$h")`, `String(" end\"")` (the `${`/`}` delimiters are trivia). This
is the 3-token model the old pest highlighter had. **Scope is highlighting
only** — WI still has no position-addressable CST, so this does *not* enable a
trivia-preserving formatter (see residual #1); the fix rebuilds fragment spans
from the byte stream for colouring, not a reusable CST. Tests:
`mpl-language-server` `string_interpolation_splits_into_subtokens` (rewritten
from the old `whole_string_is_one_token` single-token pin) plus three `lex`
tests; lib 95→98, workspace 654→657, all green.

### Phase A — editor-side grammar parsing moved to Rust/WASM (Tier-1 parity)
The last two JS hand-parsers that duplicated grammar are gone, replaced by WASM
exports backed by the parser's own definitions (single source of truth):
- `hover.ts` `PARAM_LINE_RE`/`OPTION_RE` → `param_declarations(query)` WASM export
  (reuses the existing `extract_declared_params` scanner; projects to
  `{name,type,optional}`).
- `completions.ts` `PLAIN_IDENT_RE` → `is_plain_ident(name)` WASM export backed by
  `src/query.rs::is_plain_ident` (also now drives `query/fmt.rs::escape_ident`).
  This **fixed a real drift bug**: the JS regex `^[A-Za-z]…` rejected
  leading-underscore idents (`_foo`, `_`) the grammar accepts (`[A-Za-z_]…`).
- Dead code removed: `extra/mpl-language-server/src/parser.rs` (an abandoned
  hand-rolled tokenizer under a blanket `#![allow(dead_code)]`, only ever called
  by its own `#[cfg(test)]` tests — a circular test-cfg-test loop). No
  `allow(dead_code)` remains in the tree.
- The `query_spec` build bug (`mpl_lang::stdlib::STDLIB` → private module) is
  fixed to `mpl_lang::STDLIB`; `--features examples` builds again.

Detail in `REPORT.md` → *"Phase A — Tier-1 parity with the RW reference"*.

This file used to track slice gaps. After the full port it tracks the **real
residuals**. Authoritative detail lives in `REPORT.md` → *"Phase 2 — full pest
removal"*.

## Residual open items

### 1. Completions cursor byte-scanner kept (principled non-removal) — `L`
winnow's recovery yields a best-effort **AST**, not a position-addressable
**CST** — it discards trivia and the exact offsets of the incomplete fragment,
so it cannot map a byte cursor to a node. Building a parallel position-mapped CST
would cost *more* than the existing scanner. (This is the explicit WI-vs-RW
boundary: a CST could host `node_at(offset)`; winnow's manual-recovery AST
cannot.) The one place pest was load-bearing — `extract_source_via_parser` — is
now a 12-line `wparser::parse_file` call; the scanner itself is unchanged.

### 2. Per-feature grammar verbosity — characteristic, not a defect
The hand-written winnow grammar is wordier per rule than a combinator/CST
equivalent (it absorbs both parsing **and** AST lowering in one Rust source).
Worth weighing for "the language will grow"; not an open bug.

### 3. Historical test names — `XS` (cosmetic)
`assert_matches_pest` / `pest_q` now assert parse-vs-`compile()` / structure
agreement (pest is gone). The names are leftover and could be renamed; behaviour
is correct.
