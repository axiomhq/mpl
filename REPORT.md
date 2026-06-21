# RW port report — recursive descent + `rowan` lossless CST

Approach **RW**: a hand-written recursive-descent parser producing a lossless
`rowan` (red/green) CST, plus a thin lowering pass CST → the existing
`src/query.rs` AST. Highlighting is driven by walking the rowan tree.

## 1. Library & version

| Crate    | Version | Role                                                   |
|----------|---------|--------------------------------------------------------|
| `rowan`  | 0.16.1  | Lossless red/green syntax tree (the rust-analyzer model). |
| `logos`  | 0.16.1  | Declarative lexer (token regexes/literals).            |

Both build on **stable Rust** (verified on `rustc 1.95.0`, edition 2024).
`cargo build` (whole workspace), `cargo test`, `cargo fmt --check` and
`cargo clippy` are clean for the changed crates. The crate root denies
`warnings`, `clippy::pedantic`, `clippy::unwrap_used` and `missing_docs`; the
new `mpl_lang::cst` module satisfies all of them.

**Lexer choice — `logos` over hand-rolling.** The slice still needs string
literals with escapes, `#/regex/` and `#s/…/…/` literals, backtick-escaped
idents, comments, and numbers — all classic regex-shaped tokens that are
error-prone by hand. With `logos` each token is one declarative line, so
"add the next operator/keyword-shaped token" is a one-line change. The only
friction (rowan needs one `SyntaxKind` enum; logos derives on it fine in 0.16,
including unannotated node-only variants) cost nothing. Keywords are **not**
lexed — they lex as `IDENT` and the parser relabels them contextually
(`bump_as(KEYWORD)`), so a metric named `align` still works.

## 2. Approach — how the pest grammar maps onto rowan

Three layers, all in `src/cst/`:

1. **Lexer** (`SyntaxKind` derive in `mod.rs`, driver in `parser.rs::parse`).
   `logos` turns source into a flat `(SyntaxKind, byte-range)` stream that
   includes **trivia** (`WHITESPACE`, `COMMENT`) and an `ERROR` token for any
   unlexable byte. Nothing is dropped.
2. **Parser** (`parser.rs`). One `fn` per pest rule (`source`, `metric_id`,
   `time_range`, `filter_or`/`filter_and`/`filter_not`/`filter_clause`/
   `filter_atom`, `align_body`, `param_type`, …). Each opens a `rowan`
   `GreenNodeBuilder` node, consumes tokens, and closes it. Pest's silent
   `WHITESPACE`/`COMMENT` rules become real tokens attached to the tree by
   `eat_trivia()` (leading trivia → parent, inter-token trivia → current node),
   so node spans stay tight while the tree stays lossless.
   * Pest's *ordered choice* → `if self.at_kw(...)` dispatch.
   * Pest's `("or" ~ x)*` repetition → `while self.at_kw("or") { … }`.
   * The pipe dispatch uses a `checkpoint()` so `| filter …` becomes a single
     `FILTER_RULE` node that includes the pipe (matching pest's `filter_rule`).
3. **Lowering** (`lower.rs`). Walks the typed-ish rowan tree and builds the
   **unchanged** `query::Query`. The AST in `src/query.rs` was not touched, so
   visitors / typecheck / formatter are unaffected.

Keyword vs identifier and the semantic token kinds (`KEYWORD`, `TYPE_NAME`,
`CMP_OP`, `BOOL_LIT`, `TIME_UNIT`, …) are assigned by the parser while building,
which is what makes the highlight map (§7) a trivial 1:1 lookup.

## 3. Code stats

| Item | LOC (raw) | LOC (code) |
|------------------------------------------|---:|---:|
| `src/cst/parser.rs` (lexer driver + RD parser + recovery) | 586 | 476 |
| `src/cst/lower.rs` (CST → AST)            | 601 | 513 |
| `src/cst/mod.rs` (`SyntaxKind`, `Language`, keyword table) | 313 | 184 |
| `src/cst/tests.rs`                        | 190 | — |
| **Total new (excl. tests)**              | **1500** | **1173** |
| `extra/mpl-language-server/src/tokenize.rs` (rewritten) | 140 | — |

Reference: the existing `src/parser.rs` (pest hand-walk for the **whole**
grammar) is 1679 LOC. The slice's parser+lowering is comparable in size while
covering far less grammar — the cost the CST model trades for losslessness +
recovery. Per-rule it stays small and uniform (see §8).

**Files touched:** `Cargo.toml` (+2 deps), `src/lib.rs` (+`pub mod cst`),
`src/cst/*` (new), `extra/mpl-language-server/Cargo.toml` (+rowan),
`extra/mpl-language-server/src/tokenize.rs` (+tests, rewritten),
`packages/mpl-codemirror/src/language.ts` (rewritten).

**New deps:** `rowan 0.16`, `logos 0.16`. Transitive: `countme`, `text-size`,
`rustc-hash`, `hashbrown`, `fnv`, `logos-derive`/`logos-codegen` (build-time).

## 4. Error recovery

**Supported, and it is the core of the design.** Recursive descent + error
nodes means the parser is *total*: `parse()` never returns `Err`. On
unexpected input the parser records a `SyntaxError { message, range }` and
either continues or wraps the offending tokens in an `ERROR_NODE`. Unknown
pipe rules (`| map …`) and trailing garbage are consumed into `ERROR_NODE`s,
so **every byte stays in the tree** and highlighting keeps going.

Incomplete-input demo (`metric:cpu | filter region == `, a dangling `==`),
tokens returned by the rewritten `tokenize`:

```
    Variable "metric"
    Variable "cpu"
  Punctuation "|"
     Keyword "filter"
    Variable "region"
    Operator "=="
```

and `metric:cpu | align using `:

```
    Variable "metric"
    Variable "cpu"
  Punctuation "|"
     Keyword "align"
     Keyword "using"
```

No panic, no "no tokens" — the recognised prefix is fully highlighted. The CST
for the first case keeps the trailing space and an empty `EXPR`:

```
QUERY
  SOURCE
    METRIC_ID
      DATASET / IDENT "metric"
      COLON ":"
      METRIC_NAME / IDENT "cpu"
  FILTER_RULE
    PIPE "|"  KEYWORD "filter"
    FILTER_OR/AND/NOT/CLAUSE/ATOM
      IDENT "region"
      VALUE_FILTER
        CMP_OP "=="
        WHITESPACE " "
        EXPR            ← empty: the missing value, recorded as an error
```

Tests: `cst::tests::{recovers_from_incomplete_filter, recovers_from_incomplete_align,
out_of_slice_pipe_becomes_error_node, leading_garbage_does_not_panic}` and the
`tokenize::tests::incomplete_*` / `*_still_highlights` suite.

## 5. Lossless CST / trivia

**Yes — fully lossless and formatter-ready.** Comments and whitespace are real
`COMMENT`/`WHITESPACE` tokens in the tree (pest discards them as silent rules).
`cst::tests::lossless_roundtrip_preserves_every_byte` asserts
`parse(input).syntax().text() == input` for comment-laden, multi-line and
incomplete inputs. That byte-exact round-trip is the formatter prerequisite the
brief calls out: a formatter walks the same tree and can see every comment and
its position. Trivia attachment is deliberately simple (leading→parent,
inter-token→current node); a pretty-printer that wants "comment owns the line
below" can refine attachment later without touching the lexer/parser.

## 6. Diagnostics

**Multi-error: yes.** Because the parser keeps going, `parse().errors()`
returns *all* recovery diagnostics, each with a precise `TextRange` (byte
offsets straight from the lexer span). Sample (`metric:cpu | filter region == `):

```
SyntaxError { message: "expected a value", range: 30..30 }
```

Spans are token- or position-accurate (here a zero-width span at EOF marking
where the value should be). `lower()` surfaces the first parse error as a
`ParseError::SyntaxError` (with the same span) so callers never lower a partial
tree. Wiring `errors()` into the language server's `diagnostics` JSON is a
stretch goal left undone (see SKIPPED.md) — the data is produced, just not yet
bridged.

## 7. Editor: logic moved into Rust — THE HEADLINE

**The entire JS grammar in `language.ts` is deleted.** Before, the editor
re-implemented MPL as JS regexes and *only* used the WASM tokens when pest
managed a full parse — otherwise it fell back to regex, and it always ran a
regex pass for comments (pest drops them) and for keywords in gaps.

Removed from `language.ts` (147 → 84 lines, −43%):

* `MPL_KEYWORDS`, `COMMENT_RE`, `STRING_RE`, `REGEX_RE`, `NUMBER_RE`,
  `BOOL_RE`, `TYPE_RE` — **all 7 grammar regexes / keyword lists gone**;
* `findMatches`, `findKeywordsInGaps`, the priority/overlap resolver, and the
  whole "WASM failed → regex fallback" branch — gone.

`language.ts` now just calls `mpl.tokenize(doc)` and maps token `type` → CSS
class. **There is no MPL grammar knowledge left in TypeScript.** This works
because the Rust side now:

* **recovers**, so `tokenize` returns tokens on incomplete input (the reason a
  JS fallback existed at all) — the fallback is dead code, deleted;
* emits **comment** tokens from the CST (pest couldn't), deleting `COMMENT_RE`;
* emits **keyword/type** tokens precisely for slice constructs, and for
  out-of-slice constructs (`map`, `bucket`, …) the highlighter falls back to
  one centralized Rust keyword table (`cst::keyword_syntax_kind`) — the single
  source of truth that replaces the JS keyword regex.

Rust side: `tokenize.rs` walks the rowan tree (`SyntaxKind → TokenType`, a flat
`match`), special-casing `REL_TIME` (two lexed tokens `1` + `m` → one `Number`)
and applying the keyword table only inside `ERROR_NODE` subtrees (so a tag
named `count` stays a Variable while a real `| map` lights up). All 51 existing
`tokenize` tests pass; 11 were updated to assert the new, better behavior
(recovery returns tokens instead of `None`). String-interpolation
sub-highlighting was **fixed in the follow-up surgical pass** (see *"Addendum —
string-interpolation sub-highlighting"*): the lexer now descends into `${ … }`,
so the walk sub-tokenizes the interior automatically.

**What remains in TS:** only the `type → Decoration` table (CSS class mapping)
and a `try/catch` that renders nothing if the WASM module is not yet
initialised. Neither encodes grammar. There is no remaining editor regression:
string-interpolation sub-highlighting now lights up the embedded expressions
(`String` fragments around a `Variable`/`Number`/…).

## 8. Maintainability probe — adding `| dedup by <tags>`

Walk-through (not implemented). It is a small, mechanical, well-localized
change — the same shape as the existing `align` rule:

1. **AST** `src/query.rs`: add `Dedup { tags: Vec<String> }` and an
   `Aggregate::Dedup` variant (this is shared with the pest path; any new rule
   needs it regardless of front end).
2. **Lexer** `src/cst/mod.rs`: nothing — `dedup`/`by` lex as `IDENT`, `tags`
   reuse existing tokens. (Add a `DEDUP_RULE`/`TAGS` node kind to `SyntaxKind`.)
3. **Keyword table** `src/cst/mod.rs`: add `"dedup"` to `keyword_syntax_kind`
   (one word) so it highlights even before step 4 lands.
4. **Parser** `src/cst/parser.rs::pipe_rule`: add one `else if self.at_kw("dedup")`
   branch that opens `DEDUP_RULE`, `bump_as(KEYWORD)`, then a small
   `tags()` helper (`ident (',' ident)*`). ~10 lines.
5. **Lowering** `src/cst/lower.rs`: add a `SyntaxKind::DEDUP_RULE` arm in
   `lower_simple_query` calling a `lower_dedup` that reads the `IDENT`s. ~10 lines.
6. **Highlighting**: free — `KEYWORD`/`IDENT` already map; no change.
7. **Tests** `src/cst/tests.rs`: one parse + one lower test.
8. **`language.ts`**: **zero** changes — the grammar is entirely in Rust now.

Effort: **S**. Files touched: 4 (`query.rs`, `cst/mod.rs`, `cst/parser.rs`,
`cst/lower.rs`) + tests. No TS, no regex, no second grammar to keep in sync —
which is exactly the duplication this port set out to kill. Compare the status
quo: a new rule today means pest grammar + `src/parser.rs` hand-walk + the
`language.ts` regex + the `completions.rs` byte scanner all kept in lockstep.

## 9. Build & WASM

* **Stable build:** yes. `cargo build`, `cargo test` (76 lib + 506 LS tests),
  `cargo fmt`, `cargo clippy` all clean.
* **wasm-pack:** `bash packages/build-mpl-wasm.sh` succeeds
  (`wasm-pack build … --target web --profile wasm-release --no-opt`,
  wasm-bindgen 0.2.122). Output is a working `@axiomhq/mpl` package.
* **Bundle-size delta** (`mpl_bg.wasm`, measured by stashing the slice changes
  and rebuilding the baseline):

  | Build | raw bytes | gzipped |
  |----------------------------|----------:|--------:|
  | baseline (pest only)       | 1,897,102 | 575,159 |
  | slice (pest + rowan/logos) | 1,929,723 | 584,774 |
  | **delta**                  | **+32,621 (+1.7%)** | **+9,615 (+1.7%)** |

  Both parsers currently ship (the crate keeps pest for `compile()` and the LS
  keeps it for completions/diagnostics/lints), so this is the *added* cost of
  rowan+logos+the new tokenizer. If the port were completed and pest dropped,
  the net would likely be flat-to-smaller.

## 10. Migration effort & blast radius vs `src/parser.rs`

* **Additive, low blast radius.** `mpl_lang::cst` is a new module; the AST and
  every downstream consumer are untouched (`lower` targets the existing
  `Query`). The pest path stays in place, so nothing regresses while the port
  proceeds rule-by-rule. The only behavioral change shipped is in the editor
  tokenizer (intentional: recovery + comments), covered by updated tests.
* **To fully replace pest:** port the remaining pipe rules + `compute_query` +
  interpolation + absolute times (SKIPPED.md; mostly **S/M** each, all the same
  shape), point `compile()` at `cst::lower`, then delete `src/mpl.pest`,
  `src/parser.rs`, and the pest dep. That last step also lets the LS
  completions/diagnostics move onto the CST (their hand-rolled byte parsers can
  be deleted in favour of tree queries) — a large follow-on win this slice
  de-risks but does not take.
* **Risk to existing behavior so far:** 11 `tokenize` tests updated (all to
  assert *better* behavior), 0 AST/visitor/typecheck changes.

## 11. Risks / unknowns

* **Two grammars during migration.** Until the port completes, pest and rowan
  coexist and could drift. Mitigated by the slice being additive and by lower
  reusing the exact AST; a differential test (pest vs cst lower) would close
  this and is recommended.
* **Trivia-attachment policy is simple.** Good enough for lossless + a basic
  formatter; a high-fidelity formatter may want smarter "which node owns this
  comment" rules. Localized to `eat_trivia`/`start`.
* **`unsafe transmute` in `SyntaxKind::from_raw`.** Standard rowan idiom,
  bounds-checked, and only ever fed kinds we produced; still `unsafe`. Could be
  swapped for a generated `match` if that is unwanted.
* **Highlight parity edge cases.** Bare `<` / `>` comparisons in *error*
  regions are left unhighlighted (ambiguous with `Option<…>`); `==`/`!=`/`<=`/
  `>=` always highlight. Matches the old JS fallback, minor vs pest.
* **Completions/diagnostics still on pest.** The CST already produces the data
  (`errors()`, a typed tree) but the bridges aren't built — scope, not a
  blocker.

## 12. Verdict — **keep**

This is the right tool for the stated goals. The whole reason the project wants
off pest is the editor: pest is all-or-nothing, drops trivia, and forces the
grammar to be duplicated in `language.ts` (regex) and `completions.rs` (byte
scanner). The rowan CST kills exactly those: it recovers (so highlighting works
mid-edit), it carries comments/whitespace (so a formatter is possible), and its
SyntaxKind→TokenType walk let me **delete every grammar regex from
`language.ts`** with no TS grammar left behind — the headline success criterion.
Maintainability is strong: adding a pipe rule is a localized ~20-line change
across 4 Rust files and *zero* TS, versus today's four-places-in-lockstep. The
costs (a hand-written parser is more code per rule than a PEG line; +1.7% wasm;
a simple trivia policy) are modest and bounded. Recommend adopting `rowan` +
`logos` and completing the port rule-by-rule, then retiring pest and moving
completions/diagnostics onto the same tree.

---

## Phase 2 — full pest removal

The slice front end is now the **only** front end. `pest` is gone; the
recursive-descent parser over the `rowan` CST + the `cst::lower` pass cover the
whole grammar, drive `compile()`, the wasm shims, and the language server.

### LEDGER

**pest removed?** **YES.** Proof:

```
$ rg -n 'pest' --type toml            → (no matches)
$ rg -cn 'use pest|pest::' src extra  → (no matches)
```

`src/mpl.pest`, `src/parser.rs` (the `MPLParser`/`Rule` tree-walk), the
`extra/mpl-language-server/src/parser.rs` byte-scanner and `visit.rs`
`PairVisitor` are deleted. The only residual `pest` strings are a handful of
historical code comments ("matching the old pest behaviour"). `pest` /
`pest_derive` are removed from every `Cargo.toml` (lib, language-server,
playground — the playground had a dead `pest` dep, also dropped).

**Tests:** all pre-existing feature tests pass.

| Suite                       | before | after | note |
|-----------------------------|-------:|------:|------|
| `mpl-lang` lib              |     76 |    80 | −4 pest-internal, +8 new CST lowering |
| `mpl-lang` `tests/parse.rs` |      3 |     3 | example corpus (join/replace → NotSupported, compute, interpolation, rfc3339…) |
| `mpl-language-server` lib   |    506 |   496 | −10 dead byte-scanner tests |
| `mpl-language-server-wasm`  |      5 |     5 | unchanged |
| `mpl-playground`            |     84 |    84 | unchanged |
| **total**                   |  **674** | **668** | |

No feature test was deleted or weakened to go green. The 14 removed tests were
either (a) **pest-internal** (`MPLParser::parse(Rule::time/number, …)` — the API
no longer exists) or (b) tests of the language-server `parser.rs` byte-scanner,
which was **dead code** (`#![allow(dead_code)]`, zero callers). Their coverage
is replaced:

- relative/timestamp/rfc3339/modifier time, numbers, signed `inf`, map/group/
  bucket/ifdef/sample/extend/compute, interpolation, escaped `$\`param\`` →
  **+8 new `cst::tests` lowering tests**.
- the 7 `compile()`-based tests in the old `parser/tests.rs` (compute aggregate
  ordering, all the `optional_*`/ifdef cases) were **moved verbatim** into
  `src/tests.rs` — front-end-agnostic, so they keep proving the same behaviour.

**Tests adapted (called out):**
- `cst::tests::out_of_slice_pipe_becomes_error_node` → `unknown_pipe_becomes_error_node`:
  it asserted `| map` becomes an `ERROR_NODE` because `map` was out of slice;
  `map` is now a real rule, so it uses a genuinely unknown keyword (`| frobnicate`).
- `tests/parse.rs`: `ParseError::NotSupported { rule }` → `{ feature }` — the
  `rule` field was a `pest::Rule`; it is now a `String` feature name (join/replace).

**Net code delta** (file-level, vs the staged slice baseline):

| File                                   | before | after |     Δ |
|----------------------------------------|-------:|------:|------:|
| `src/parser.rs` (pest tree-walk)       |   1679 |     0 | −1679 |
| `src/mpl.pest`                         |    172 |     0 |  −172 |
| `src/errors.rs` (PestError recon.)     |    679 |   301 |  −378 |
| LS `parser.rs` (byte-scanner)          |    571 |     0 |  −571 |
| LS `visit.rs` (PairVisitor)            |     55 |     0 |   −55 |
| LS `lints.rs` (pest→CST walk)          |    113 |    93 |   −20 |
| `src/parser/tests.rs` (moved out)      |    167 |     0 |  −167 |
| `src/cst/parser.rs`                    |    586 |   945 |  +359 |
| `src/cst/lower.rs`                     |    601 |  1328 |  +727 |
| `src/cst/mod.rs`                       |    313 |   359 |   +46 |
| `src/cst/tests.rs`                     |    190 |   310 |  +120 |
| `src/tests.rs` (moved-in + new)        |    519 |   623 |  +104 |

Deletions of pest-only machinery: **−3042**. Additions (new parser/lowering +
moved/new tests): **+1356**. **NET ≈ −1686 lines**, of which production code is
**≈ −1741** (deleted 2875 lines of pest path: tree-walk + grammar + error
reconstruction + LS byte-scanner + PairVisitor; added 1134 lines of CST
parser/lowering). RW shows the biggest deletion because the lossless CST + one
lowering pass replace **five** separate pest-era components at once.

**WASM bundle** (`mpl_bg.wasm`, `wasm-pack … --profile wasm-release --no-opt`):

| build                       |    raw bytes |  gzipped |
|-----------------------------|-------------:|---------:|
| baseline (pest only)        |    1,897,102 |  575,159 |
| slice (pest + rowan/logos)  |    1,929,723 |  584,774 |
| **phase 2 (pest removed)**  |**1,767,088** |**554,316** |

Dropping pest made the bundle **smaller than the original pest-only build**:
−130 KB raw (−6.9%) / −21 KB gzip (−3.6%) vs the pest-only baseline; −163 KB raw
/ −30 KB gzip vs the slice that shipped both parsers.

**Reuse / dedup:**
- **DEDUPED `unescape_and_trim`/`unescape`**: the slice had re-handrolled these in
  `cst/lower.rs`. They are now the single canonical copy (the `pest`
  `src/parser.rs` copy is deleted with the file). Every literal lowering
  (strings, regex inner, escaped idents, directive values, `param_value`) routes
  through that one pair.
- Reused **unchanged**: the entire `src/query.rs` AST, `src/query/fmt.rs`
  (formatter), `visitor.rs`, the `linker`/`STDLIB` function lookups
  (`map_fn`/`align_fn`/`group_fn`/`compute_fn`/`bucket_function`), `enc_regex`,
  `tags`, the `ParamTypecheckVisitor`/`GroupCheckVisitor`/`OptionCheckVisitor`
  passes in `lib.rs`, and the `regex`/`chrono` parsing for regex literals and
  RFC3339 times. The `Number`/`as_f64` helper and `parse_param_value`'s
  type-dispatch were ported from `parser.rs` into `lower.rs`.
- The completion engine's stdlib walkers, `ProvidedParams`, diagnostics
  conversion, and `system_params` plumbing are untouched.

**Simplifications enabled by leaving pest:**
- **`tokenize`** (language-server + wasm): already a `SyntaxKind → TokenType`
  walk of the CST; it now needs no pest at all and keeps highlighting incomplete
  input because the parser recovers.
- **`diagnostics`**: driven by `compile()` + lints. With `compile()` on the CST,
  the whole `From<PestError<Rule>>` reconstruction in `errors.rs` (friendly-rule
  naming, `generate_suggestion`, `token_length`, `rules_keywords` — ~378 lines)
  is **deleted**. Parse errors now come straight from the parser's recovery
  diagnostics with byte-exact `TextRange`s. The exact span/positions the old
  pest reconstruction produced (`ds` → 0..2, `ds:` → EOF, `ds:[` → the `[`, …)
  are reproduced by the parser's `metric_id` error placement — all
  `diagnostics::assert_parse_error` span tests pass.
- **`lints`**: the pest `PairVisitor` (`visit.rs`) is deleted; `detect_hints`
  is a flat token walk over the CST that gates on `parse().errors().is_empty()`
  (the recovery-aware analogue of the old `MPLParser::parse(...).ok()`).
- **`completions`**: the two functions that used `MPLParser::parse(Rule::source)`
  now read dataset/metric from the recovering CST's `METRIC_ID` node. (The
  cursor-context heuristics — `locate_query_context`, `classify_string_context`,
  the literal skippers — are a *backward-from-cursor* engine, **not** pest
  machinery; they are independent of the front end and kept. Replacing them with
  a CST cursor-walk is a separate, large refactor that the 2 000-line completion
  test corpus would gate, so it was not bundled into this phase — see the
  residual note. No completion test changed.)
- **Editor**: `language.ts` was already 100% WASM-driven (no JS grammar) from the
  slice; nothing left to delete there.

**Maintainability note — adding `| dedup by <tags>` now that the full parser
exists:** with the whole grammar on the CST it is the *exact* shape of the
`group` rule already in `parser.rs::group_body` + `lower::lower_group`:

1. `query.rs`: add `Aggregate::Dedup { tags }` (shared, any front end needs it).
2. `cst/mod.rs`: add a `DEDUP_RULE` `SyntaxKind` and `"dedup"` to
   `keyword_syntax_kind` (one word — it highlights immediately).
3. `cst/parser.rs::pipe_rule`: one `else if self.at_kw("dedup")` arm that wraps
   `DEDUP_RULE`, `bump_as(KEYWORD)`, then `self.tags()` (the existing helper).
   ~8 lines.
4. `cst/lower.rs::lower_aggregate`: one `SyntaxKind::DEDUP_RULE => …` arm calling
   `lower_tags(&child(node, TAGS))`. ~6 lines.
5. one parse + one lower test in `cst/tests.rs`.

Files touched: 4, ~20 lines, **zero** TS, **zero** regex, no second grammar.
This is *strictly simpler than under the slice* (which still had pest in the
loop) and far simpler than the pre-port status quo, where the same rule meant
editing `mpl.pest` + the `src/parser.rs` hand-walk + the `language.ts` regex +
the `completions.rs` byte-scanner in lockstep. The new `tags()`/`number()`/
`func()` helpers mean each new pipe rule is a couple of `at_kw` branches.

**Residual (honest):**
- ~~**String-interpolation sub-highlighting**~~ — **FIXED** (follow-up surgical
  pass; see *"Addendum — string-interpolation sub-highlighting"* below). The
  lexer now descends into `${ … }`: the literal is split into `STRING_FRAGMENT`
  tokens with the embedded expression parsed by the *same* `expr()` parser, so
  highlighting sub-tokenizes automatically and the CST is byte-for-byte lossless
  down into interpolations (formatter prerequisite — now met). The old
  opaque-token re-parse (`lower_string` byte-scanner + the `lower_interp_expr`
  second grammar) is deleted; lowering reads the fragments straight off the tree.
- ~~**`completions.rs` cursor heuristics** (the backward-from-cursor scanner)~~ —
  **MOSTLY MIGRATED** to the rowan CST (follow-up phase; see *"Addendum —
  completions cursor engine migrated to the rowan CST"* below). The structural
  position-detection — compute-query nesting (`locate_query_context`), pipe
  location (`find_last_pipe`/`count_pipes`), source extraction
  (`extract_source_info`) and the partial word at the cursor
  (`extract_partial_word`) — now derives from `cst::parse(...).syntax()` instead
  of byte scanning. Six byte-scanner helpers were deleted. The **one** residual
  is `classify_string_context` (string vs `${ }` interpolation vs plain text):
  it cannot be CST-driven because the lexer collapses an *unterminated* string
  (no closing quote) into a single opaque `ERROR` token with no `STRING`/
  `DOLLAR_BRACE` structure, and the white-box `cursor_in_interpolation_*` tests
  pin exactly that mid-edit behavior. Kept and documented. All 394 completion
  tests stay green.
- Pre-existing, **unrelated to pest**: `mpl-language-server`'s `examples`-feature
  `query_spec()` references the private path `mpl_lang::stdlib::STDLIB` (should be
  `mpl_lang::STDLIB`); it does not build under `--features examples`. This line is
  unchanged by this phase and the default workspace/wasm builds never enable that
  feature. Left as-is to keep the diff focused on pest removal.

**Verification run:**

```
cargo build --workspace                                   → ok
cargo build -p mpl-language-server-wasm \
    --target wasm32-unknown-unknown                       → ok
bash packages/build-mpl-wasm.sh                           → ok (wasm-pack)
cargo test --workspace                                    → 668 passed; 0 failed
cargo clippy --workspace --all-targets                    → 0 warnings / 0 errors
cargo fmt --check                                         → clean
rg -n 'pest' --type toml                                  → (no matches)
rg -cn 'use pest|pest::' src extra                        → (no matches)
```

**Verdict for RW at full scope — KEEP.** Recursive-descent over a lossless
`rowan` CST + a thin `logos` lexer is the right tool: it deletes ~1.7k net lines
of pest-era machinery (the grammar, the tree-walk, the error reconstruction, the
LS byte-scanner, and the PairVisitor — five components collapsed into one), makes
the wasm bundle *smaller* than the original pest build, and turns "add a pipe
rule" into a localized ~20-line, single-grammar change. It recovers from
incomplete input (so highlighting/lints/tokenize work mid-edit) and is
byte-lossless (formatter-ready). Interpolation sub-highlighting has since been
landed (addendum below); the remaining work (moving the completion cursor-engine
onto the CST) is additive and de-risked by this phase, not blocked by it.

## Addendum — string-interpolation sub-highlighting (surgical follow-up)

The one residual editor regression — `"${ expr }"` highlighting as a single
opaque `String` — is now **fixed**, and the fix also closes the
formatter-prerequisite hole (the interpolation interior was absent from the
otherwise-lossless CST).

**What changed (scope-limited to the string path):**

- **Lexer/tokenizer (`src/cst/parser.rs`).** A `STRING` literal is still matched
  by `logos`, but the tokenizer now *descends* into it: `string_end` /
  `find_interp_close` compute the true span (understanding `${ … }` nesting and
  escapes — `logos`' first-`"` regex mis-spans a *nested* string), and
  `expand_string` splits the literal into `STRING_FRAGMENT` runs (boundary
  fragments keep their `"`), `${`/`}` delimiter tokens, and the embedded
  expression — which is **re-lexed and parsed by the existing `expr()`**, not a
  second grammar. `Parser::string` shapes these into a lossless `STRING` node.
- **CST kinds (`src/cst/mod.rs`).** Added `STRING_FRAGMENT` and `DOLLAR_BRACE`;
  `STRING` is reused as the node kind wrapping the descended pieces.
- **Lowering (`src/cst/lower.rs`).** Deleted the opaque-token re-parse: the
  `lower_string` byte-scanner **and** the `lower_interp_expr` second expression
  grammar are gone (~104 lines → 37). The new `lower_string` reads
  `STRING_FRAGMENT` tokens (`unescape_and_trim`, reused) and `EXPR` subtrees
  (`lower_expr`, reused) straight off the tree. `lower_directive` reads the
  string node’s text the same way.
- **Highlighting (`extra/mpl-language-server/src/tokenize.rs`).** One line:
  `STRING_FRAGMENT → String`. The `${`/`}` delimiters are structural
  (unhighlighted), so the existing `SyntaxKind → TokenType` walk emits the right
  sequence with **no special-casing**.

**Result — token dump for `"Hello ${ name }!"`** (the 3-token highlighting):

```
String   "\"Hello "
Variable "name"
String   "!\""
```

and `"port ${ 8080 }"` → `String "\"port "`, `Number "8080"`, `String "\""`.
Nested interpolation (`"${ "nested ${ inner }" }"`) descends recursively and
round-trips.

**Tests:** the two pinning tests that asserted the *wrong* (opaque) behavior were
rewritten to assert the sub-token sequence (`String, <value>, String`); a new
`interpolated_string_roundtrips_losslessly` CST test proves byte-for-byte
round-trip for leading/adjacent/escaped/nested/empty/directive interpolations.
Workspace: **669 passed / 0 failed** (was 668; +1 round-trip test).
`clippy` clean, `fmt --check` clean, wasm builds.

**Maintainability:** the embedded-expression logic is *not* duplicated — it is
the same `expr()` parser used by filters/extends, and the same `lower_expr` /
`unescape_and_trim` helpers. Net moving parts went *down*: a hand-rolled
byte-scanner and a second mini-grammar in lowering were replaced by one scanner
in the lexer that feeds the canonical parser.

## Addendum — last JS grammar duplication removed (hover params + plain_ident)

Two grammar fragments still lived in `packages/mpl-codemirror/`'s TypeScript;
both are now single-sourced to Rust via thin wasm calls. The editor no longer
encodes **any** MPL grammar in production TS.

**1. Param-declaration hover (`src/hover.ts`).** Deleted `PARAM_LINE_RE` and
`OPTION_RE` (the two regexes that hand-parsed `param $name: T;` /
`Option<T>` lines). `parseParamDeclarations` now delegates to the new wasm
export `param_declarations(query)`, which **reuses the completion engine's
existing scanner** — `extract_declared_params` / `parse_param_decl` in
`extra/mpl-language-server/src/completions.rs` — via a tiny projection
(`declared_params` → `ParamDeclaration { name, type, optional }`). Hover and
completion now agree on what counts as a declaration *by construction* (same
code path), and `Option<T>` is unwrapped in exactly one place.

**2. Identifier escaping (`src/completions.ts`).** Deleted `PLAIN_IDENT_RE`.
`needsEscape` now calls the wasm export `is_plain_ident(name)`, backed by the
new `mpl_lang::is_plain_ident` — the same predicate `escape_ident` in
`src/query/fmt.rs` uses for `Display` (I refactored `escape_ident` to call it,
deduping the character-class logic). This fixed a **real silent drift**: the JS
regex was `^[A-Za-z][A-Za-z0-9_]*$`, but the grammar's `IDENT` is
`[A-Za-z_][A-Za-z0-9_]*` — the editor wrongly backtick-escaped leading-`_`
identifiers (`_foo`). Verified through real wasm: `is_plain_ident("_foo") ===
true` now.

### What moved to Rust vs stayed in TS

| Item                         | Before (TS)                    | After                                                |
| ---------------------------- | ------------------------------ | ---------------------------------------------------- |
| `param $n: T;` parsing       | `PARAM_LINE_RE` + `OPTION_RE`  | wasm `param_declarations` → `declared_params` (Rust) |
| `Option<T>` unwrap           | `OPTION_RE`                    | `parse_param_decl` (Rust, reused)                    |
| plain-ident / escape rule    | `PLAIN_IDENT_RE`               | wasm `is_plain_ident` → `mpl_lang::is_plain_ident`   |
| `KEYWORD_DOCS` (hover copy)  | TS object                      | **stays TS** (static help text, not grammar)         |
| backtick-wrapping mechanics  | `escapeIdent`/`applyTextForIdent` string replace | **stays TS** (string escaping, not the grammar rule) |

`KEYWORD_DOCS` is deliberately left in `hover.ts`: it is presentation/help copy
(descriptions + example syntax shown in tooltips), not a parser. The backtick
*wrapping* (`\`…\``, `\\`/`` \` `` escaping) also stays in TS — it is mechanical
string escaping, identical to Rust's; only the *plain_ident decision* (the
grammar) was the drift risk, and that is what moved.

### New wasm function(s)

- `param_declarations(query: &str) -> JsValue` (array of
  `{ name, type, optional }`) — wraps `mpl_language_server::declared_params`.
- `is_plain_ident(name: &str) -> bool` — wraps `mpl_lang::is_plain_ident`.

Both in `extra/mpl-language-server-wasm/src/lib.rs`; the generated `mpl.d.ts` /
`mpl.js` pick them up automatically on `bash packages/build-mpl-wasm.sh`.

### Tests

- **Rust:** workspace **672 passed / 0 failed** (was 669; +1 `is_plain_ident`
  grammar test in `src/query/tests.rs`, +2 `declared_params` projection tests in
  `extra/mpl-language-server/src/completions/tests.rs`). The exhaustive
  parsing edge cases (Option unwrap, missing `;`, comments, whitespace, all
  types) are already covered by the pre-existing `extract_declared_params`
  suite, which `declared_params` reuses.
- **TS (`@axiomhq/mpl-codemirror`, vitest):** **62 passed / 0 failed** (was 65).
  Net −3 is intentional: the 7 `parseParamDeclarations` *parser*-detail tests
  (which asserted the now-deleted TS regex behavior) were replaced by 3 adapter
  tests that mock the wasm boundary (`vi.spyOn(mpl, "param_declarations")`) and
  verify only the array→`Map` reshape + the wasm-unavailable fallback — the
  parsing assertions moved to Rust where the parser now lives. One
  `mergeSystemParamsInto` test that *incidentally* relied on `parseParamDeclarations`
  parsing now builds its input `Map` directly (it tests merge precedence, not
  parsing). Added 1 `needsEscape` test pinning the leading-`_` drift fix.
- **Stub (`src/__mpl-stub__.ts`):** added `param_declarations` (returns
  `undefined`, like the other unavailable-wasm doubles; data-driven tests mock
  it) and `is_plain_ident` (a one-line `/^[A-Za-z_][A-Za-z0-9_]*$/` mirror,
  explicitly documented as a TEST DOUBLE kept in sync with the Rust grammar —
  the `needsEscape`/`escapeIdent` tests call it directly). Production never
  touches the stub; the single source of truth is the Rust wasm export.
- **wasm harness** (`node tests/wasm/test-wasm.mjs`): 38 passed; plus a manual
  end-to-end check confirming both new exports behave through real wasm.

### Net code delta (this addendum)

- **Deleted from production TS:** 3 grammar regexes (`PARAM_LINE_RE`,
  `OPTION_RE`, `PLAIN_IDENT_RE`) and the regex-loop body of
  `parseParamDeclarations` (~16 lines).
- **Added:** ~2 thin wasm shims (Rust), `declared_params` + `ParamDeclaration` +
  `ParamType::canonical_name` (~45 lines Rust, all reusing the existing scanner),
  `is_plain_ident` (~12 lines Rust, **dedups** the duplicated char-class out of
  `escape_ident`), and the TS delegating wrappers (roughly line-neutral with what
  they replaced).
- **NET:** production TS grammar surface goes to **zero**; Rust grows by a small,
  reuse-heavy projection layer. The win is *one* source of truth, not raw LOC.

### Maintainability — re-do "add `| dedup by <tags>`" for *this* concern

For an editor consumer the question is now: *where do I teach the editor a new
identifier/param rule?* Answer: **only Rust.** A new param type, or a change to
what counts as a plain identifier, is made once in `mpl-lang` (lexer /
`is_plain_ident`) or `completions.rs` (`parse_param_decl`); the editor inherits
it through `param_declarations` / `is_plain_ident` with **no TS edit**. Under the
old code you had to find and update the matching JS regex too (and, as the
leading-`_` bug shows, that copy *did* drift). Strictly fewer moving parts.

### Verdict

The CodeMirror package now contains **no MPL grammar in production TypeScript** —
only presentation copy (`KEYWORD_DOCS`), mechanical string escaping, and wasm
calls. Grammar lives solely in Rust. The migration also paid for itself by
killing a live correctness bug (leading-underscore escaping).

## Addendum — completions cursor engine migrated to the rowan CST

The completion engine's *position-detection layer* — "where is the cursor in the
query structure?" — was a hand-rolled backward-from-cursor **byte scanner**.
It is now driven by the recovering `rowan` CST (`cst::parse(query).syntax()`),
the same tree the highlighter/diagnostics already use. The **completion data**
layer (`collect_*`, `walk_modules`, `function_info`, `lookup_function`) and the
**result builders** (`suggest_*`, `pipe_keywords`, …) are unchanged — only their
*input* (context + partial word) now comes from the CST.

**Hard gate held:** every one of the **394** completion tests
(`completions::*`, the ~2 000-line corpus) stays green. No test was weakened,
skipped, deleted, or adapted. The 16 white-box `locate_query_context` tests and
the ~25 `extract_partial_word` tests pass **unchanged** against the CST-backed
implementations — they now double as equivalence proofs.

### LEDGER

**Functions migrated to the CST (5)** — same signatures, CST-derived bodies:

| fn                     | now derives the cursor's structure from …                                  |
|------------------------|----------------------------------------------------------------------------|
| `locate_query_context` | `COMPUTE_QUERY` nodes (paren nesting / subquery scoping). Parens/commas inside strings, regex, comments, backtick idents, function calls and filter grouping never become `COMPUTE_QUERY` delimiters, so they're excluded for free. |
| `find_last_pipe`       | the last `PIPE` token (pipes inside strings/regex/comments/backticks are folded into their tokens by the lexer). |
| `count_pipes`          | `PIPE` token count (compute-rule pipe vs tail). |
| `extract_source_info`  | the first `METRIC_ID` node in document order (parser handles directives, leading comments and trailing pipe rules natively — the byte preamble scan is gone). |
| `extract_partial_word` | the maximal run of word-like tokens ending at the cursor, off the full-text parse (so a cursor *inside* a `${ … }` interpolation still sees the embedded `id` as an `IDENT`). |

**Byte-scanner helpers DELETED (6)** — orphaned once the above migrated:

| fn                     | ~lines |
|------------------------|-------:|
| `skip_literal`         |    ~41 |
| `is_compute_paren`     |    ~31 |
| `find_line_comment`    |    ~24 |
| `is_regex_replace_start` |  ~13 |
| `skip_regex_body`      |    ~12 |
| `preceded_by_eq`       |    ~10 |
| `extract_source_via_parser` (merged into `extract_source_info`) | ~31 |

**Byte scanner RETAINED, production (irreducible) — 1 context:**

- `classify_string_context` (+ its `skip_backtick` helper). It classifies the
  cursor as ordinary code / inside a `${ … }` interpolation / plain string text.
  **It cannot be CST-driven without observable behavior drift:** the `logos`
  lexer only forms a `STRING`/`STRING_FRAGMENT`/`DOLLAR_BRACE` structure for a
  string that has a **closing quote**. An *unterminated* string (the common
  mid-edit case, e.g. `… == "a ${ ` with the cursor still inside) collapses into
  a single opaque `ERROR` token with no interpolation structure, so the CST
  cannot tell "inside `${ }`" from "plain string text". The white-box tests
  `cursor_in_interpolation_true_cases` / `_false_cases` pin exactly this
  unterminated-input behavior. Per the brief's escape hatch ("if a context
  genuinely cannot be driven from the CST … STOP, keep that scanner, document
  why"), it is kept. *Note:* the completion **behavior** tests that exercise
  interpolation all carry a closing quote, so those resolve fine; it is only the
  white-box classifier tests that require the byte version.

**Byte helpers RETAINED, test-only (`#[cfg(test)]`):**

- `is_char_escaped`, `skip_string_literal`, `skip_interpolation`,
  `cursor_in_interpolation`. After migration these have **no production caller**,
  but the corpus has dedicated white-box unit tests for them
  (`is_char_escaped_*`, `skip_string_literal_*`, `cursor_in_interpolation_*`)
  that the hard gate forbids deleting. They are gated behind `#[cfg(test)]` so
  there is no dead production code (and no `dead_code`/clippy noise) while the
  tests stay green. They are pure byte utilities, not part of cursor-context
  resolution anymore.

**Net LOC delta (`completions.rs`):** 2328 → 2177 = **−151 lines**.
(Deletions: ~162 lines of byte scanners + the source-extraction preamble.
Additions: the five CST-backed bodies + `is_word_token` + `child_token`, ~+11
net after the deletions.)

**Contexts now resolved via the CST vs heuristics:** of the five "where is the
cursor" questions, **four** are CST-resolved (compute nesting, pipe position,
source dataset/metric, partial word) and **one** (string / interpolation /
plain-text) remains a byte scanner for the unterminated-string reason above.

### What stayed exactly the same (proof of scoped change)

`compute_completions_with_params` still orchestrates the same way:
`extract_partial_word` → `classify_string_context` → `locate_query_context` →
`suggest_for_context` / `suggest_for_preamble` / `suggest_for_source` /
`suggest_for_compute_rule`. Every `suggest_*` builder, `pipe_keywords`,
`suggest_filter_context`, `suggest_ifdef_context`, `suggest_bucket_args`,
`extract_declared_params`/`parse_param_decl`, and the stdlib walkers are byte-for-byte
unchanged; only their inputs changed source.

### Simplification this unlocks

The completion engine and the highlighter/diagnostics now read the **same**
recovering tree. Adding a new pipe rule (e.g. `| dedup by <tags>`) needs **no**
new completion byte-scanner: `locate_query_context`/`find_last_pipe` already
classify the new pipe position structurally, and the existing keyword/tag
`suggest_*` paths pick it up from the keyword token — under the old engine a new
rule could need matching tweaks to the brace/literal byte scanners.

### Honest cost

The position-detection functions now `cst::parse` (small, recovering) instead of
byte-scanning; because the *kept* `suggest_*` builders consume already-scoped
sub-slices (`before[pipe..]`), they re-parse those slices rather than sharing one
tree. For interactive completion on short queries this is negligible; a single
shared tree was not threaded precisely because doing so would require rewriting
the `suggest_*` builders, which the brief said to keep.

### Verification

```
cargo test --workspace            → 672 passed; 0 failed
  (mpl-lang 82 + parse.rs 3 + mpl-language-server 498
   [394 completion tests] + wasm 5 + playground 84)
cargo clippy --workspace --all-targets → clean (0 warnings)
cargo fmt --check                 → clean
bash packages/build-mpl-wasm.sh   → ok (wasm-pack, wasm-release)
```

## Addendum — completions fully CST-driven (last byte scanner retired) — residual #2 CLOSED

The previous addendum left **one** irreducible byte scanner: `classify_string_context`
(code vs `${ }` interpolation vs plain string text), because an *unterminated* string
collapsed into one opaque `ERROR` token, so the CST could not classify the cursor inside
it. This phase removes that holdout by giving the **lexer/CST interior structure for
unterminated strings**, then deleting the scanner. **All 5 cursor contexts are now
CST-driven.**

### Step 1 — lexer recovery for unterminated strings (the enabler)

`src/cst/parser.rs`: the string-aware tokenizer (`lex_range`) already intercepted
`logos`' raw `STRING` tokens and expanded them into `STRING_FRAGMENT` / `DOLLAR_BRACE` /
embedded-`EXPR` pieces via `expand_string` + `string_end` + `find_interp_close`. An
**unterminated** string fails `logos`' `STRING` regex and surfaces as one `ERROR` token
beginning with `"`. `lex_range` now routes that `ERROR` through the **same**
`expand_string` path (no second string lexer):

- `string_end` returns `(end, terminated)` instead of just `end`; `expand_string` takes
  the `terminated` flag and sets `body_end = range.end` (no closing quote to exclude) for
  the open case.
- `Parser::string` records an `unterminated string` diagnostic over the literal's full
  extent (reusing `string_end`), so `compile` still rejects it exactly as before — the
  node extent still runs to EOF.

Result: `"a ${ b` (mid-edit) now parses to `STRING[ STRING_FRAGMENT "\"a ", DOLLAR_BRACE,
EXPR[ IDENT "b" ] ]`, the same interior shape a closed string gets. Pinned by
`cst::tests::unterminated_interpolated_string_recovers_interior_structure`,
`tokenize::tests::unterminated_interpolation_still_sub_tokenizes_mid_edit`, and
`completions::tests::unterminated_interpolation_classifies_as_interpolation_mid_edit`.

### Step 2 — `classify_string_context` migrated to the CST

It now walks the recovering tree: the innermost `STRING` node covering the cursor
(exclusive-start / inclusive-end so the cursor binds to the token on its left) decides
"in a string"; whether the offset lands in one of that node's `STRING_FRAGMENT` children
(literal text) vs the `${ … }` region between them (interpolation). Escapes, `//`
comments, regex and backtick idents are already folded into their own tokens by the
lexer, so the byte-level escape/comment/backtick handling is gone.

### LEDGER

**All 5 "where is the cursor" contexts now resolve via the CST:**

| context                | CST source |
|------------------------|------------|
| compute-query nesting (`locate_query_context`) | `COMPUTE_QUERY` nodes |
| pipe position (`find_last_pipe` / `count_pipes`) | `PIPE` tokens |
| source dataset/metric (`extract_source_info`) | first `METRIC_ID` node |
| partial word (`extract_partial_word`) | word-token run at cursor |
| **code / interpolation / string text (`classify_string_context`)** | **innermost `STRING` node + its `STRING_FRAGMENT` children — NEW** |

**Production fn deleted (`completions.rs`):**

| fn             | ~lines | note |
|----------------|-------:|------|
| `skip_backtick` |    ~17 | sole caller was `classify_string_context`; gone |

**Test-only `#[cfg(test)]` byte helpers deleted (`completions.rs`):**

| fn                    | ~lines |
|-----------------------|-------:|
| `is_char_escaped`     |    ~13 |
| `skip_string_literal` |    ~24 |
| `skip_interpolation`  |    ~26 |

**Lexer fn deleted (`parser.rs`):**

| fn                    | ~lines | note |
|-----------------------|-------:|------|
| `next_unescaped_quote` |   ~20 | the "unterminated `${` closes at next quote" artifact; unterminated interpolation now simply runs to EOF |

**Old byte-scanner `classify_string_context` (~57 lines) → new CST classifier (~41 lines).**

**Unit tests deleted (helper white-box, `completions/tests.rs`) — 8:**
`is_char_escaped_no_backslash`, `_one_backslash`, `_two_backslashes`, `_three_backslashes`,
`_at_start` (5); `skip_string_literal_handles_escapes_and_nested_interpolation`,
`_skips_backtick_ident_in_interpolation`, `_clamps_on_trailing_backslash` (3). These test
*pure byte utilities*, not completion behavior — deleting them is the cleaner result the
brief calls for.

**Tests RE-POINTED (behavior coverage preserved), not dropped:**
`cursor_in_interpolation_true_cases` / `cursor_in_interpolation_false_cases` now drive the
**new CST** `classify_string_context` (via the kept `#[cfg(test)] cursor_in_interpolation`
wrapper) on realistic queries (`ds:m | where x == "…`) — the production shape, since
completion always runs on an editor's in-progress query, not a bare `x == "…` fragment.
Same code/interp/text assertions, now equivalence proofs against the CST.

**Tests ADDED (3):** the three mid-edit tests listed in Step 1.

**Net LOC delta (`completions.rs`):** 2178 → 2086 = **−92 lines** (−84 from the 4 byte
scanners, −16 from the classify rewrite, partly offset by the richer doc comment).
`parser.rs`: net ≈ flat (recovery branch + unterminated diagnostic + `(end, terminated)`
plumbing added, `next_unescaped_quote` deleted).

**Behavior test change (one, justified as an improvement — not a weakening):**
`nested_interpolation_suggests_params` (`"a ${ "b ${ $#"`) still asserts `params` and
**stays green**, but the *mechanism* changed. The deleted byte scanner had its own
"every `"` toggles string state" nesting rule, divergent from the lexer (which is the
single source of truth for highlighting/parsing). To make the CST classifier resolve the
nested case, the lexer's unterminated-`${` handling now runs to EOF (recursing into nested
strings) instead of closing at the next stray quote. This:
- makes the completion classifier agree with the highlighter/parser (no more divergent
  third interpretation),
- produces *proper nested `STRING`/`EXPR` structure* for nested unterminated
  interpolations (directly serving Step 1's goal), and
- leaves the canonical `"a ${ b` extent and the simple multi-line case **unchanged** (no
  stray quote → same EOF close as before).
No workspace test was weakened, skipped, or deleted to achieve green.

### Maintainability re-do — "add a new pipe rule `| dedup by <tags>`"

Now that completion position-detection is 100% CST-driven, adding a pipe rule needs **no**
byte-scanner edits anywhere: parser gets a `DEDUP_RULE` arm + lowering; the completion
engine's `find_last_pipe`/`count_pipes`/`locate_query_context` classify the new pipe
position structurally off `PIPE` tokens, and `classify_string_context` is rule-agnostic
(it only cares about `STRING` interior). You add the keyword + a `suggest_*` arm and you
are done — versus the pre-CST engine, where a new rule risked interacting with the
brace/literal/quote byte scanners (and the old `classify_string_context` would have had to
keep its `${ }`/backtick assumptions correct by hand).

### Verification

```
cargo test --workspace            → 667 passed; 0 failed
  (mpl-lang 83 + parse.rs 3 + mpl-language-server 492
   [386 completion tests] + wasm 5 + playground 84)
  Δ vs pre-phase 672: −8 deleted white-box helper unit tests, +3 new mid-edit tests
cargo clippy --workspace --all-targets → clean (0 warnings)
cargo fmt --check                 → clean
bash packages/build-mpl-wasm.sh   → ok (wasm-pack, wasm-release)
```

### Verdict

Completions are now **fully CST-driven** — zero byte scanners in
`extra/mpl-language-server/src/completions.rs` (`rg -n 'as_bytes|skip_backtick|
is_char_escaped|skip_string_literal|skip_interpolation' completions.rs` → none). The
completion engine, highlighter, diagnostics and parser all read the **same** recovering
`rowan` tree, so they can no longer drift. Residual #2 is **CLOSED**.

## Addendum — string-interpolation boundary made token-driven (Option B)

The last byte-scanner in the CST *lexer itself* — the two functions that found
where a string literal and each `${ … }` interpolation ended — is gone. Boundary
detection is now **token-driven**, fixing the locked boundary bug.

### The bug (was locked by an ignored test)

`src/cst/parser.rs` found string + `${ }` boundaries with `string_end` /
`find_interp_close`, two mutually-recursive **byte scanners** that only understood
`\` (escape) and `"` (quote) — plus, in `find_interp_close`, a bare `}`. They were
blind to the three MPL constructs that legitimately carry a `}` or `"` *inside* an
interpolation: backtick idents (`` `a}b` ``), `#/regex/` literals, and `// comments`.
So on valid MPL like `ds:cpu | where t == "x ${ `a}b` }"` they stopped at the `}`
*inside the backtick ident* and mis-detected the `${ … }` boundary, yielding an empty
interpolation + an `ERROR_NODE` + **3 spurious errors**. The contract was locked in
`src/cst/tests.rs::interpolation_with_braced_escaped_ident_parses_cleanly` (`#[ignore]`).

### The fix — which Option B variant, and why

**Chosen: the hand-rolled mode stack driven over the `logos` token stream** (the
brief's second, explicitly-equal option), *not* `logos::morph` + an `Extras` mode
stack. Rationale — **fewer moving parts** (the brief's stated tie-breaker):

- The existing lexer already had the exact recursion shape I needed: `expand_string`
  scans literal fragment text, and the interpolation interior is re-lexed by the
  normal `logos` lexer. Only the *boundary-finding* was byte-scanning. So the minimal,
  cleanest change is to replace the boundary finder with **brace-token counting** over
  the same `logos` stream — no second `Logos` enum, no `Extras`/`Into` plumbing, no
  brace-depth-across-morph bookkeeping.
- The "mode stack" is the **Rust call stack**: `expand_string` = *string mode*
  (scanning `STRING_FRAGMENT` text up to `\`/`"`/`${`), `lex_interp` = *interpolation
  mode* (normal `logos` tokens, counting braces); a nested `"` recurses
  `lex_interp → expand_string → lex_interp`. This is a literal mode stack, just
  managed by recursion instead of an explicit `Vec`.
- `morph` would have added a whole second token enum + `Extras` conversions for zero
  behavioural gain, against "prefer fewer moving parts / no new traits/types unless
  they remove duplication."

**How `lex_interp` works:** it lexes the `${ … }` interior with the normal
`SyntaxKind` `logos` lexer and finds the closing `}` by **counting brace tokens** —
`L_BRACE` ⇒ `depth += 1`, `R_BRACE` at `depth == 0` ⇒ that is the close (returned to
`expand_string`, which emits the `R_BRACE`), `R_BRACE` at `depth > 0` ⇒ `depth -= 1`.
Because the interior is lexed by `logos`, `` `a}b` `` / `#/…"…/` / `// …}` each come
out as a **single token**, so the `}` or `"` inside them is part of that token and
never produces a brace/quote token — it can no longer be miscounted. A nested string
literal (which `logos` mis-spans at its inner `"`) is descended into via
`expand_string`, recursing the brace counting through any further interpolations.

### Exactly which byte-scanners were removed / replaced

| Removed (byte scanner)                          | Replaced by (token-driven)                          |
|-------------------------------------------------|-----------------------------------------------------|
| `find_interp_close(slice, start, end)` (~21 LOC) — byte-walked for `}`, blind to backtick/regex/comment | `lex_interp(slice, expr_start, …)` (~40 LOC) — `logos` tokens, brace-depth counting |
| `string_end(slice, start) -> (usize, bool)` (~28 LOC) — byte-walked for the string's end, recursing into `find_interp_close` | folded into `expand_string`'s return value (it now *returns* the true end) + `lex_interp` for interior ends |
| `lex_range(base, slice, tokens)` — re-lexed each interior sub-slice with a `base` offset | `lex(input) -> (tokens, unterminated)` — single pass over the full input; interiors handled by `lex_interp` with absolute offsets (no `base` threading) |

`expand_string` is retained but rewritten: it still byte-scans **literal fragment
text** for `\`/`"`/`${` (correct and unavoidable — in literal string context a
`` ` ``/`#`/`/` is just text, with no special meaning; the bug was never here), but it
now delegates every `${ … }` interior to `lex_interp` and *returns* the literal's true
end (so `lex` can reposition the outer `logos` lexer). The `base`/`terminated`
parameters are gone.

Net `src/cst/parser.rs`: the boundary functions changed from byte-scanning (~49 LOC:
`string_end` + `find_interp_close`) to token-counting (~40 LOC: `lex_interp`); `lex` /
`expand_string` shed their `base`/`terminated`/sub-slice plumbing. **Net ≈ flat**
(a few lines shorter in code, a longer explanatory comment), but the *boundary logic*
is now driven by the single source of truth — the `logos` token stream — instead of a
parallel byte interpretation that could (and did) drift from it.

### How the "unterminated string" diagnostic is now derived

Previously `Parser::string` *recomputed* termination at parse time by calling
`string_end(self.text, start)` (a second byte scan). With `string_end` deleted, the
diagnostic is now derived from the **lexer's own mode state**: when `expand_string` or
`lex_interp` reaches **EOF while still in string / interpolation mode** (no closing `"`
for the string, or `lex_interp` returns `None` because no depth-0 `}` was found before
EOF), `expand_string` records the string's start offset in an `unterminated: Vec<usize>`
that `lex` returns alongside the tokens. `Parser` stores it; `Parser::string` looks up
the current string's start and, if present, emits `error_at(start..EOF, "unterminated
string")` — same message, same full-extent range (`end == input.len()`), same
`compile`-rejects-it behaviour as before, but sourced from the lex pass rather than a
re-scan. Nested unterminated strings are each recorded (matching the prior per-`STRING`
behaviour). Pinned by the unchanged
`cst::tests::unterminated_interpolated_string_recovers_interior_structure`.

### Regression tests added (`src/cst/tests.rs`)

- **(locked, un-ignored)** `interpolation_with_braced_escaped_ident_parses_cleanly`:
  `"x ${ `a}b` }"` → interior is the single `ESCAPED_IDENT` `` `a}b` ``, 0 errors,
  lossless.
- **(a)** `interpolation_with_quoted_escaped_ident_parses_cleanly`:
  `"x ${ `a"b` }"` — an escaped ident carrying a `"` → single `ESCAPED_IDENT`
  `` `a"b` ``, 0 errors, lossless (the old scanner would have read that `"` as the
  closing quote).
- **(b)** `interpolation_with_commented_brace_finds_real_boundary`: a multi-line
  interpolation `"x ${ x // note }\n} y"` whose interior has a `//` comment containing
  `}` before the real `}` on the next line → the `// note }` is one `COMMENT` token,
  the OUTER `STRING` spans to the final `"`, exactly one `DOLLAR_BRACE`/`R_BRACE` pair,
  lossless. (Interior may error on semantics; the boundary is what's asserted.)
- **(c)** all pre-existing interpolation / roundtrip / recovery tests pass unchanged
  (`interpolated_string_roundtrips_losslessly` incl. escaped `\${`, nested, empty,
  adjacent, directive; `unterminated_interpolated_string_recovers_interior_structure`).

### Hard invariants held

- **Same CST token/node kinds** (`STRING`, `STRING_FRAGMENT`, `DOLLAR_BRACE`, `R_BRACE`,
  `EXPR`, `ESCAPED_IDENT`, `REGEX`, `COMMENT`, …): the token *stream* a closed string
  produces is byte-for-byte identical to before, so `tokenize.rs`, `completions.rs`
  (`classify_string_context`) and `lower.rs` consume it unchanged. Proven by the 52
  `tokenize` + 387 `completions` tests passing untouched.
- **Byte-for-byte lossless round-trip**: held for every existing case + the 3 new ones.
- **Unterminated recovery + diagnostic**: interior still structured; diagnostic over
  full extent, now derived from lex-time mode state (above).
- **Escaped `\${` literal; nested interpolation**: both covered by the existing
  round-trip test, unchanged.

### Maintainability note — robustness of brace counting

`lex_interp` counts `{`/`}` defensively, but in practice valid MPL never puts a bare
`{` inside `${ … }` (the interpolation grammar is `expr = const | param | ident |
string`, no braces), so `depth` stays 0 for all real inputs and the very first depth-0
`}` closes — i.e. the common path is "stop at the first `}` token". The counting only
matters as graceful recovery for malformed input, and it follows the brief's specified
semantics exactly. Crucially, a nested string's `{`/`}` are consumed by the recursive
`expand_string` (as fragment text / nested interpolation), never seen by the interior
`logos` lexer, so they can't perturb the depth.

### Verification

```
cargo test --workspace            → 670 passed; 0 failed; 0 ignored  (was 667 + 1 ignored)
  mpl-lang lib 86 (+3: un-ignored locked test now PASSES + tests (a),(b))
  parse.rs 3 · mpl-language-server 492 (387 completions + 52 tokenize) · wasm 5 · playground 84
cargo clippy --workspace --all-targets → clean (0 warnings)
cargo fmt --check                      → clean
bash packages/build-mpl-wasm.sh        → ok (wasm-pack, wasm-release)
```

The previously-`#[ignore]`d test now **passes and is no longer ignored**; no test was
weakened, skipped or deleted to go green.

### Verdict

Boundary detection is now sourced from the same `logos` token stream that drives
parsing/highlighting/completions, eliminating the parallel byte interpretation that
carried the bug. The CST lexer holds **no** byte-level boundary scanner anymore — only
the unavoidable literal-fragment text scan, which has no construct to be blind to.
