# MPL `pest` → `chumsky` slice port — REPORT

## 1. Library & version

[`chumsky`](https://crates.io/crates/chumsky) **0.13.0** (the current stable
release; the `1.0.0-alpha.x` line is a pre-release and was deliberately not
used). Added to the root `Cargo.toml` as:

```toml
chumsky = { version = "0.13", default-features = false, features = ["std"] }
```

`default-features = false` drops chumsky's `stacker` default feature (which pulls
`psm`/`stacker` and does not build on `wasm32-unknown-unknown`). Confirmed:

- Builds on **stable Rust 1.95.0** (`cargo build --workspace`).
- Builds on **`wasm32-unknown-unknown`** (`cargo build -p mpl-language-server-wasm
  --target wasm32-unknown-unknown`) and through `wasm-pack`.
- Only **3** new crates in `Cargo.lock`: `chumsky`, `allocator-api2`,
  `unicode-segmentation`.

## 2. Approach — how the pest grammar maps onto chumsky

The port lives in `src/slice.rs` and produces the **existing** AST from
`src/query.rs` directly from combinators, carrying spans. Mapping is close to
mechanical:

| pest | chumsky |
|---|---|
| `rule = { a ~ b }` (sequence) | `a().then(b())` / `ignore_then` / `then_ignore` |
| `a \| b` (ordered choice) | `choice((a(), b()))` (rewinds on failure → PEG semantics) |
| `x*` / `x+` | `.repeated().collect()` / `.at_least(1)` |
| `x?` | `.or_not()` |
| `@{…}` atomic, `${…}` compound | a single lexeme parser; trivia handled by the `lex()` wrapper |
| silent `WHITESPACE` / `COMMENT` | `trivia()` = `(whitespace \| "//"…)*`, consumed after each token by `lex()` |
| span on a `Pair` | `.map_with(\|v, e\| … e.span() …)` → `to_span()` → `miette::SourceSpan` |
| `param_ident` resolved against declared params (stateful pest walk) | chumsky **state** (`SimpleState<SliceState>`); `param_decl` pushes, `$x` references `.validate(…)` against it |
| stdlib function lookup (`align`) | `.try_map(\| func, span \| STDLIB.align_fn(&func)…)` |

Slice implemented exactly per brief: `file = directive* param* query EOI`;
`param … ;` with all 7 types + `Option<…>`; `source = metric_id time_range? as?`
(relative time only); the full `filter` sub-grammar (`or/and/not`, parens,
`tag (cmp expr | cmp_re regex | is tag_type)`); the `align` pipe rule; comments
+ whitespace as trivia. Everything else is in `SKIPPED.md`.

**The `== #/regex/` vs `== $param` ambiguity** (`filter_atom()`): handled by
ordered choice — the regex-literal branch is tried first, so `== #/x/` →
`Cmp::RegEx(Concrete)`; for `== $p` the regex literal fails, chumsky **rewinds**,
and the value branch yields `Cmp::Eq(Expr::Param)`. Whether that param is a regex
cannot be known at parse time (the param's *type* isn't), so — exactly like pest
— it is left to the existing typecheck pass, which rewrites `Cmp::Eq(Param:Regex)`
→ `Cmp::RegEx(Param)`. Test `regex_param_defers_to_typecheck_like_pest` asserts
the pre-typecheck shape matches pest's and that the full pest pipeline performs
the rewrite.

## 3. Code stats

| item | value |
|---|---:|
| `src/slice.rs` total | 885 lines |
| — lexer/highlighter section | ~188 non-blank lines |
| — parser + AST section | ~584 non-blank lines |
| `src/slice/tests.rs` | 288 lines (27 tests) |
| `src/errors.rs` `from_chumsky()` mapping | ~19 lines |
| old pest `src/parser.rs` (full language) | 1679 lines |
| files touched | `Cargo.toml`, `src/lib.rs`, `src/errors.rs` (+`src/slice.rs`, `src/slice/tests.rs` new); editor: `extra/mpl-language-server/src/tokenize.rs` (199→69), `…/tokenize/tests.rs`, `packages/mpl-codemirror/src/language.ts` (147→85) |
| new deps | `chumsky` (+ `allocator-api2`, `unicode-segmentation`) |

The slice (≈770 LOC parser+lexer) covers a meaningful chunk of grammar that the
pest path spends ~1679 LOC on for the *whole* language; per-rule the chumsky
combinators are noticeably denser and self-contained.

## 4. Error recovery

**Yes** — via chumsky's `recover_with` strategies:

- **`via_parser`** at the clause level (`clause()`): a malformed `| …` clause is
  rewound and re-consumed up to the next `|`/EOF as a `Clause::Skipped`, so one
  bad pipe rule never aborts the whole parse.
- **`nested_delimiters`** for parenthesised filter groups (`filter()`).
- A trailing-junk validator preserves the recovered `Query` while still
  reporting leftover input.

Errors are collected (multi-error): `parse()` returns
`SliceParse { query: Option<Query>, errors: Vec<ParseError>, warnings }`.

Incomplete-input demo (verbatim from a throwaway `--nocapture` run):

```
INPUT: "metric:cpu | filter region == "
  query built: true
  error: SyntaxError { span: SourceSpan { offset: SourceOffset(30), length: 0 },
          message: "found end of input expected any, '/', '#', '$', '\"', '+', '-', or '`'" }
  highlight tokens: [("metric", Variable), ("cpu", Variable), ("|", Punctuation),
                     ("filter", Keyword), ("region", Variable), ("==", Operator)]

INPUT: "metric:cpu | align using "
  query built: true
  error: SyntaxError { span: SourceSpan { offset: SourceOffset(25), length: 0 },
          message: "found end of input expected any, or '/'" }
  highlight tokens: [("metric", Variable), ("cpu", Variable), ("|", Punctuation),
                     ("align", Keyword), ("using", Keyword)]

MULTI INPUT: "ds:metric | filter a == | align using nope_fn"  (2 errors)
  error: SyntaxError {  offset 24, message "found '|' expected something else, …" }
  error: SyntaxError {  offset 38, message "Unsupported align function: nope_fn" }
```

Both incomplete inputs still produce a `Query` (source preserved) **and** a
precise error. The malformed query yields **two** independent errors.

## 5. Lossless CST / trivia

**Honest answer: chumsky is AST-oriented and the AST is NOT lossless.** The
combinators throw whitespace and `//` comments away while building `Query`,
exactly like the old pest path. The AST also only carries spans where the
existing types already have a `span` field (e.g. `Expr::Param`, `ParamDeclaration`)
— it is not a green-tree CST.

What *is* preserved, and where the formatter prerequisite is met: the **lexer**
layer (`slice::highlight`, a separate total chumsky parser) emits a span for
every meaningful token **and** for every `//` comment (`HlKind::Comment`). So
the token stream is lossless for comments and could become fully lossless for a
formatter by adding whitespace spans to that lexer (cheap, ~1 combinator) — the
recovery/structure work is already done. The structured AST is the wrong layer
for a formatter here; the lexer is the right one. This split is documented in
`src/slice.rs`'s module docs.

## 6. Diagnostics

Multi-error: yes (§4). chumsky's `Rich<char>` error carries a byte span and a
human "found X expected one of Y" reason. `errors::from_chumsky()` maps each into
the crate's existing `ParseError::SyntaxError { span, label, message, suggestion }`
— the *same* miette variant the pest `From<PestError>` path produces — so editor
diagnostics render identically regardless of parser. Span quality is good
(byte-accurate; zero-length at EOF for "expected more input", single-token for
unexpected tokens). Sample:

```
SyntaxError { span: (offset 38, len 1),
              message: "Unsupported align function: nope_fn" }
```

Semantic errors raised inside combinators (`Unsupported align function`,
`undefined param $x`, `Option<…>` misuse, sliding-window align) are surfaced the
same way via `Rich::custom`. (Not yet wired into `compute_diagnostics`; see
SKIPPED.md.)

## 7. Editor: logic moved into Rust — **headline**

**The JS regex grammar in `packages/mpl-codemirror/src/language.ts` is deleted
in full.** Before, `language.ts` re-implemented the MPL grammar as regexes —
`MPL_KEYWORDS`, `COMMENT_RE`, `STRING_RE`, `REGEX_RE`, `NUMBER_RE`, `BOOL_RE`,
`TYPE_RE` — plus a "find keywords in the gaps between WASM tokens" pass, used as
a **fallback whenever the parser failed, i.e. on every incomplete/mid-edit
query** (pest's `collect_tokens` returned `None` on any parse failure).

After: `language.ts` (147 → 85 lines) calls `mpl.tokenize(doc)` and maps spans
to decorations. **Zero regexes, zero keyword lists, zero grammar in TS.**

What moved into Rust:
- `extra/mpl-language-server/src/tokenize.rs` (199 → 69 lines) is now a thin
  adapter over `mpl_lang::slice::highlight` — the chumsky **lexer**, which is
  *total* (a trailing `any()` catch-all). `collect_tokens` changed from
  `Option<Vec<Token>>` to `Vec<Token>`: it **always** returns spans, including
  for incomplete input, so the fallback's reason to exist is gone.
- Comment highlighting moved into Rust (`HlKind::Comment`), deleting `COMMENT_RE`.
- The old pest tree-walk tokenizer (`PairVisitor`-based) is removed.

Proof on incomplete input (test `keyword_survives_incomplete_filter`,
`incomplete_input_still_tokenizes`, and §4 demo): `metric:cpu | filter region == `
still returns `filter`→Keyword, `region`→Variable, `==`→Operator, etc. — no
panic, no empty result.

**What remains in TS and why:** only the `TokenType → CSS class` mapping (the
`decos` table) and the CodeMirror `ViewPlugin` plumbing — presentation, not
grammar. That is irreducibly editor-side.

**What remains pest-based (not part of this slice):** completions
(`completions.rs` hand-rolled byte scanner) and diagnostics/lints. They are
unchanged and still correct; migrating them is tracked in `SKIPPED.md`.

## 8. Maintainability probe — adding `| dedup by <tags>`

Walking the chumsky slice (not implemented; estimate **S**):

1. **`src/query.rs`** — add `Aggregate::Dedup(Dedup)` and
   `struct Dedup { tags: Vec<String> }`. (AST is shared, so this also touches
   exhaustive `match`es on `Aggregate` in `visitor.rs` / `query/fmt.rs` /
   runtime — the only cross-cutting cost, identical for *any* parser.)
2. **`src/slice.rs`** — add a `tags()` combinator (`ident().separated_by(sym(","))`,
   ~3 lines, reusable by group/bucket/join too) and a `dedup()` combinator
   (`kw("dedup").ignore_then(kw("by")).ignore_then(tags())`, ~4 lines); add one
   `choice` arm to `clause()`. **~10 lines, one place.**
3. **Highlighting** — add `"dedup"` to the `KEYWORDS` slice in `classify_word()`
   (`by` is already a keyword). **1 line.** Highlighting then works immediately,
   including on incomplete `… | dedup by `.
4. **`src/slice/tests.rs`** — an `equiv_*`/recovery test. **~5 lines.**

No regex to touch in TS (it's gone). Contrast the pest path: edit `mpl.pest`
(new `dedup` rule + extend `pipe_rule`), regenerate, then hand-write a
`parse_dedup` in `src/parser.rs` walking `Pairs` with `.n()`/`assert_type`/
`assert_empty` (~25 lines of defensive pair-shuffling), and — previously — also
update the `MPL_KEYWORDS` regex in `language.ts`. The chumsky version localizes
the change and the grammar *is* the code.

## 9. Build & WASM

- Stable build: ✅ `cargo build --workspace`, `cargo fmt`, `cargo clippy
  --workspace --all-targets` all clean (the crate denies `warnings`,
  `clippy::pedantic`, `clippy::unwrap_used`, `missing_docs`).
- Tests: ✅ full workspace green — `mpl-lang` lib **93** tests (incl. **27** new
  slice tests), `mpl-language-server` **84** (incl. **28** rewritten tokenizer
  tests).
- `wasm32-unknown-unknown`: ✅ compiles.
- `wasm-pack … --target web`: ✅ `bash packages/build-mpl-wasm.sh` succeeds.

Bundle-size delta (release `wasm-release` profile, no wasm-opt, as configured):

| | raw | gzip |
|---|---:|---:|
| baseline (pest only) | 1,897,102 | 575,159 |
| + chumsky slice | 1,919,730 | 581,989 |
| **delta** | **+22,628 (+1.2%)** | **+6,830 (+1.2%)** |

This is *additive* cost — both pest **and** chumsky are currently linked (pest
still drives `compile()` + skipped rules). A full migration that removes pest
would likely make this delta neutral-to-negative.

## 10. Migration effort & blast radius vs current `src/parser.rs`

- **Strategy used:** additive, zero blast radius. `src/parser.rs`, `compile()`,
  every visitor/typecheck, and all existing tests are **untouched**; the slice is
  a new module producing the same AST. The only public-surface change is in the
  editor crate (`collect_tokens` return type `Option<Vec> → Vec`), which is the
  intended improvement.
- **Full migration estimate:** porting the remaining grammar (SKIPPED.md) is the
  bulk — mostly **S/M** per rule, the heavy ones being `bucket_by`,
  `compute_query` (recursive), string interpolation (recursive), and rewiring
  `ProvidedParams::parse_and_validate` off `Rule::param_value`. The AST is
  reused verbatim, so downstream (typecheck, runtime, serde) is unaffected — the
  blast radius stays confined to the parser layer.
- **Risk to the equivalence guarantee:** low — 14 `equiv_*` tests assert the
  chumsky AST is byte-for-byte identical (via serde) to the pest+typecheck AST,
  which is the migration's safety net.

## 11. Risks / unknowns

- **chumsky state gotcha (resolved, but a footgun):** `SimpleState`'s rewind is a
  no-op, but `ignore_then`/`then_ignore` parse the *ignored* side in **check
  mode**, which silently skips `map_with`/`validate` side effects. A
  state-mutating parser (param/directive declarations) placed on an ignored side
  *appears* to "not persist" state. Fixed by keeping the stateful preamble on the
  emit path (`.then(...).map(...)`), documented inline in `file()`. Future rule
  additions must respect this.
- **Highlighting precision:** keyword/type classification is lexical (set-based),
  matching the deleted JS behavior but *less* precise than pest's position-aware
  tree-walk in two cosmetic cases (a tag named like a keyword; `<`/`>` inside
  `Option<…>`). Acceptable for an editor; documented in SKIPPED.md. Could be made
  position-aware by driving highlight off the recovered AST parse, at the cost of
  robustness on incomplete input.
- **Compile-time / type complexity:** deeply combinator-heavy parsers can produce
  enormous types and slow builds; the slice compiles fine, but a full port may
  want `.boxed()` in hot spots.
- **Semantic-vs-syntax error shape:** semantic failures (unknown function,
  undefined param) currently map to `SyntaxError`, not the dedicated pest
  variants (`UnsupportedAlignFunction`, `UndefinedParam`). A fuller port would
  carry a richer custom error type through `extra::Err`.

## 12. Verdict — **keep**

chumsky delivers the one thing this whole exercise is about: it **eliminated the
TypeScript grammar duplication entirely** and put resilient, incomplete-input
highlighting in Rust behind WASM, because its lexer is total and its parser
recovers. Adding the next pipe rule is ~10 lines in one file where the grammar
*is* the code, versus pest's edit-grammar-then-hand-walk-pairs loop plus the
(now-deleted) JS regex. The honest cost is that chumsky is AST-oriented, so
trivia/formatter support lives at the lexer layer rather than in a free CST, and
the state/check-mode footgun needs respecting. Both are manageable and
documented. For the stated priorities — maintainability first, editor
integration mandatory — chumsky is the right replacement; recommend proceeding
with the full migration tracked in `SKIPPED.md`.

---

# Phase 2 — full pest removal

This phase completes the CH (chumsky) port: every `// SKIPPED(step2):` construct
is implemented, **pest is gone entirely**, and the new parser drives every
former pest consumer. All pre-existing tests stay green.

## LEDGER

### pest removed? **YES**

- `rg -n 'pest' --type toml` → **nothing**. `pest`/`pest_derive` dropped from the
  root `Cargo.toml`; `pest` dropped from `extra/mpl-language-server/Cargo.toml`
  and `extra/mpl-playground/Cargo.toml`. No pest packages remain in `Cargo.lock`.
- `rg -cn 'use pest|pest::' src extra` → **nothing**.
- `src/mpl.pest`, `src/parser.rs` (the 1679-line `MPLParser`/`Rule` tree-walk),
  `src/parser/tests.rs`, and `extra/mpl-language-server/src/visit.rs` (the
  `PairVisitor`) are **deleted**.
- Remaining `pest` hits in `rg -n 'pest' src extra packages` are **comments,
  doc-strings, and test names** only (e.g. `assert_matches_pest`, a `pest_q`
  local) — no code path touches pest.
- All `SKIPPED(step2)` markers are gone (`rg 'SKIPPED\(step2\)'` → nothing): map,
  group_by, bucket_by (incl. cumulative-conversion form), replace, join,
  compute queries (recursive), ifdef/else, sample, extend, string interpolation,
  all time variants (relative/timestamp/RFC3339/modifier), signed `inf`, full
  directive lowering, and the `param_value` external entry point are implemented.

### Tests: before vs after

| suite | before | after |
|---|---:|---:|
| `mpl-lang` lib | 93 | 89 |
| `mpl-lang` `tests/parse.rs` | 3 | 3 |
| `mpl-language-server` | 483 | 483 |
| `mpl-language-server-wasm` | 5 | 5 |
| `mpl-playground` | 84 | 84 |
| **total** | **668** | **664** |

**No test was deleted or weakened to go green.** The −4 are the only tests that
asserted *pest internals*: `test_relative_time`, `test_timestamp`, `test_number`,
`test_number_float` matched on the pest `Rule` parse-tree shape (`Rule::time`,
`Rule::time_unit_hour`, …), which no longer exists. They were removed; the 7
**compile-driven** tests that shared the same (now-deleted) `src/parser/tests.rs`
file were preserved verbatim by moving them into `src/tests.rs`
(`compute_query_post_compute_aggregates`, the `optional_*`/`ifdef` set).

Other minimal adaptations (called out):
- `ParseError::NotSupported { rule: Rule }` → `{ rule: String }`. `tests/parse.rs`
  destructures `NotSupported { span, rule }` and `Debug`-prints `rule` — unchanged.
- `ParseParamError::TypeMismatch { rule: Rule }` → `{ found: String }`; no test
  inspects that field. `ParseParamError` itself moved out of the deleted
  `parser.rs` into `query.rs` (its natural home next to `ProvidedParams`).
- `src/slice/tests.rs` still names its equivalence helper `assert_matches_pest`
  and a local `pest_q`. These now assert **parse-vs-`compile()` agreement**
  (`compile()` *is* the chumsky parser) — still a meaningful invariant (the AST
  survives typecheck unchanged); the names are historical and left as-is.

### Net code delta (phase-2 diff vs the staged slice baseline)

Deleted from the pest path:

| file / region | lines |
|---|---:|
| `src/parser.rs` (pest tree-walk) | −1679 |
| `src/mpl.pest` | −172 |
| `src/errors.rs` `PestError`→`ParseError` reconstruction (+`friendly_rule`, `pair_to_source_span`, suggestion scanners) | −411 (698→287) |
| `src/parser/tests.rs` | −167 |
| `extra/mpl-language-server/src/visit.rs` (`PairVisitor`) | −55 |
| `…/completions.rs` pest source-extraction (net) | −25 (2277→2252) |
| `…/lints.rs` pest walk → lexer (net) | −24 (113→89) |
| `src/lib.rs` (pest exports/`compile`) | −5 (458→453) |
| **deleted total** | **−2538** |

Added for the new parser / lowering:

| file / region | lines |
|---|---:|
| `src/slice.rs` (full grammar + typed-error carrier + `parse_query` + `parse_param_value`) | +960 (893→1853) |
| `src/tests.rs` (relocated compile tests) | +103 |
| `src/query.rs` (`ParseParamError` relocation) | +19 |
| **added total** | **+1082** |

**NET: ≈ −1456 lines.** Leaving pest collapsed a 1679-line defensive
`Pairs`-walk + a `.pest` grammar + 411 lines of `PestError` reconstruction into a
denser combinator grammar where the grammar *is* the code. The chumsky parser is
larger than the phase-1 *slice* (because it is now the whole language plus a
typed-error layer), but far smaller than the pest machinery it replaces.

### Reuse / dedupe

- **Reused unchanged**: the entire AST (`src/query.rs`), `linker` stdlib lookups
  (`STDLIB.{align,map,group,compute}_fn`), `enc_regex::EncodableRegex`,
  `tags::TagValue`, `types::{Parameterized, Dataset, Metric, BucketSpec,
  BucketType, ConversionMethod}`, and — crucially — every post-parse pass
  (`ParamTypecheckVisitor`, `GroupCheckVisitor`, `OptionCheckVisitor`,
  `ProvidedParams`, the `query/fmt.rs` `Display`). They are parser-agnostic and
  needed **zero** changes; the chumsky parser produces byte-identical AST.
- **Deduped**: the slice phase had its own `unescape` copy. There is now exactly
  **one** `unescape` in the crate (in `src/slice.rs`); the `pest` path's copies
  died with `parser.rs`. (The original instruction said "reuse
  `parser::unescape_and_trim`", but full removal deletes `parser.rs`, so the
  slice's copy becomes the single canonical one.)

### Simplifications enabled by leaving pest

- **`errors.rs` lost ~411 lines.** The whole `From<PestError> for ParseError`
  reconstruction — `friendly_rule`/`friendly_rules` (a 100-line `Rule`→prose
  table), `pair_to_source_span`, `generate_suggestion`/`extract_token`/
  `token_length`/`rules_keywords`/`join_with_or` — is gone. chumsky already
  carries byte-accurate spans and a human reason, so `to_parse_error` is ~20
  lines, and **typed** semantic errors (`UnsupportedAlignFunction`,
  `UndefinedParam`, `ParamDefinedMultipleTimes`, …) are carried verbatim in the
  `Rich` *context* slot rather than reverse-engineered from a `Rule` set.
- **`tokenize` `PairVisitor` is gone** (already in phase 1): `collect_tokens` is a
  thin adapter over the total `slice::highlight` lexer.
- **Lints no longer need a pest tree-walk.** `detect_hints` is now lexer-driven
  (parse-success gate via `slice::parse`, then scan `highlight` tokens for the
  `filter` keyword and redundant backtick idents). `visit.rs` (the `PairVisitor`
  trait + `walk_pairs`) was deleted outright — it had no other users.
- **Completions byte-scanner: kept, but thinner — honest answer.** The
  cursor-context engine (`locate_query_context`, `classify_string_context`,
  `extract_partial_word`, `skip_literal`, …) *cannot* be deleted: it answers
  "what is under / before the cursor in this **incomplete, mid-edit** string",
  which an AST-oriented parser does not model (chumsky recovers a tree but has no
  notion of cursor-relative position). What I **did** delete is the only place
  completions actually invoked pest — `extract_source_via_parser` +
  `extract_ident_name` (~52 lines walking `Rule::source`/`metric_id`/`dataset`
  pairs) — replaced by ~17 lines that run `slice::parse(src)` and read the
  `Query::Simple { source }` (correct escaping/param handling for free).
- **Editor JS grammar duplication: gone.** `packages/mpl-codemirror/src/language.ts`
  carries **zero** grammar (no keyword lists, no `STRING_RE`/`REGEX_RE`/… — those
  died in phase 1); it just maps `mpl.tokenize` spans to decorations. Highlighting,
  completions, and diagnostics all flow Rust→WASM. What remains in TS is
  presentation only: a `decos` CSS table, `KEYWORD_DOCS` hover copy, an
  insertion-escaping one-liner (`PLAIN_IDENT_RE`), and a hover-only param-decl
  scan (documented as intentionally non-critical) — none of it is lexer/parser
  grammar.

### Maintainability — re-running "add `| dedup by <tags>`"

1. **`src/query.rs`** — `Aggregate::Dedup(Dedup)` + `struct Dedup { tags }`
   (cascades through the exhaustive `match`es in `visitor.rs`/`query/fmt.rs`/
   runtime — the only cross-cutting cost, identical for *any* parser).
2. **`src/slice.rs`** — one combinator
   `kw("dedup").ignore_then(kw("by")).ignore_then(tags())` (`tags()` already
   exists, reused by group/bucket/join) + one arm in `pipe_rule_body()`'s
   `choice`. **~6 lines, one place.** A bad function/arg would be a 3-line
   `try_map` returning a typed `Sem`.
3. **Highlighting** — add `"dedup"` to `classify_word`'s `KEYWORDS` (`by` is
   already there). **1 line**, works immediately including on `… | dedup by `.
4. **Tests** — an `equiv_*`/recovery test, ~5 lines.

Versus the pest loop: edit `mpl.pest` (new rule + extend `pipe_rule`), regenerate,
then hand-write `parse_dedup` walking `Pairs` with `.n()`/`assert_type`/
`assert_empty` (~25 lines of defensive pair-shuffling) **and** previously the
`language.ts` regex. The chumsky version localizes the change and the grammar
*is* the code — now even more so, because it is all in one file with no separate
grammar artifact to regenerate.

### chumsky footguns respected

- **Check-mode side effects**: the stateful preamble (`directive`/`param_decl`
  mutate `SimpleState`) is kept on the **emit** path of `file()` (`…then(query)`),
  never on an `ignore_then`/`then_ignore` ignored side, so state pushes are not
  silently skipped. Param duplicate-detection and the `OldDuration`/system-prefix
  warnings live in `.validate`/`.map_with` on that emit path.
- **Error-span fidelity on adversarial input**: the diagnostics suite pins exact
  spans (`ds` → 0..2, `ds:` → 3..3, `ds:[1h..]` → 3..4, `ds: |` → 4..5, …). A
  small `Tail` enum in `metric_id` lets a missing `:` pin its error on the
  dataset token while a missing metric name surfaces chumsky's natural
  next-token span — reproducing the old pest spans without pest's furthest-error
  heuristics.

### Verdict — **keep, at full scope**

chumsky carried the whole MPL grammar with recovery + multi-error, deleted the
pest machinery (~1450 net lines, incl. the entire error-reconstruction layer),
removed the editor's JS grammar, and kept the AST + every downstream pass
byte-identical. The honest residual cost is the completions cursor byte-scanner
(irreducible: it is cursor-context, not grammar) and a deliberately verbose
typed-error carrier in the `Rich` context slot (the price of matching pest's
typed `ParseError` variants and exact spans). Both are localized and documented.
For maintainability-first + mandatory editor integration, chumsky is the right
replacement and pest is fully removed.

### Verification (pasted)

```
$ cargo build --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s)

$ bash packages/build-mpl-wasm.sh
    Finished `wasm-release` profile [optimized] target(s)
    MPL WASM package built successfully
$ cargo build -p mpl-language-server-wasm --target wasm32-unknown-unknown
    Finished `dev` profile

$ cargo test --workspace
  mpl-lang lib ............... ok. 89 passed; 0 failed
  mpl-lang tests/parse.rs ... ok.  3 passed; 0 failed
  mpl-language-server ....... ok. 483 passed; 0 failed
  mpl-language-server-wasm .. ok.  5 passed; 0 failed
  mpl-playground ............ ok. 84 passed; 0 failed
  (total 664 passed; 0 failed)

$ cargo clippy --workspace --all-targets     # 0 warnings, 0 errors
$ cargo fmt --check                          # clean

$ rg -n 'pest' --type toml                   # (none)
$ rg -cn 'use pest|pest::' src extra         # (none)
```

---

# Phase 2.1 — string-interpolation highlighting (surgical fix)

**Scope: highlighting only.** A targeted fix to the highlight lexer
(`src/slice.rs` `highlight()` / `highlighter()`). Nothing else in the parser,
AST, runtime, or pest-removal status changed.

## The bug

The parser (`string_expr`, `src/slice.rs`) already descended into `${ … }` and
built `StringFragment::{Text,Expr}` for the AST. The **highlight** path did not:
`highlighter()`'s `string` branch matched the whole `"…${ expr }…"` literal and
emitted it as a **single** `String` token. That regressed vs the correct 3-token
model — the embedded expression was painted as opaque string text instead of
being classified (param/ident → `Variable`, number → `Number`, …).

## The fix

`highlighter()` is now `recursive`: a single highlight *token* yields a
`Vec<Highlight>` (zero, one, or — for an interpolated string — several), and the
`string` branch re-enters the same token set inside `${ … }`. It reuses the
**same** text/interpolation split the parser's `string_expr` uses
(`str_char` = escape | `$` not followed by `{` | non-`"`/`\`/`$`; `${` opens an
interpolation) and the existing `classify_word`/`param`/`number` token parsers —
**no second grammar**. A small `assemble_string` helper merges the surrounding
quotes into the adjacent text fragments and leaves the `${`/`}` delimiters and
inner whitespace unclassified.

Because the lexer stays **total** (the trailing `any()` catch-all now returns an
empty `Vec`), it still survives incomplete / mid-edit input: `"${ $h` (no close)
highlights `"` + `$h` and never panics. Nested strings inside interpolation are
handled by the recursion (proven by a test), exactly like the parser.

## Token dump (pasted)

```
$ # highlight("host ${ $h } end")  →  3-token model
"\"host "  String
"$h"       Variable
" end\""   String

$ # highlight("n ${ 42 } m")  →  embedded number classified
"\"n "     String
"42"       Number
" m\""     String

$ # nested: highlight("a ${ \"b ${ $c } d\" } e")
"\"a "     String
"\"b "     String
"$c"       Variable
" d\""     String
" e\""     String
```

## Tests

- `extra/mpl-language-server/src/tokenize/tests.rs`: the
  `interpolated_string_is_one_token_skipped` test (which *pinned* the old
  single-token behavior) was **renamed and rewritten** to
  `interpolated_string_sub_tokenizes`, asserting the `String / Variable / String`
  sub-sequence and that the whole literal is no longer one opaque token.
- `src/slice/tests.rs`: added `highlight_string_interpolation_sub_tokens` and
  `highlight_string_interpolation_number_and_nested` (number classification +
  nested-string recursion).
- Counts: lib `89 → 91` (+2 new highlight tests), all other suites unchanged.
  **Workspace total 664 → 666, all green.** No test was deleted or weakened.

## Honest limitation — NOT a formatter win

CH (this `chumsky` port) has **no lossless CST**. The AST already carries the
embedded `Expr` (via `StringFragment::Expr`), but this change does **not** enable
a trivia-preserving / CST-driven formatter. This is purely a **highlighting
correctness** fix to match the 3-token model. (The RW port's CST-formatter
benefit does not transfer here.)

## Verification (pasted)

```
$ cargo build --workspace                    # Finished, 0 errors
$ bash packages/build-mpl-wasm.sh            # MPL WASM package built successfully
$ cargo test --workspace
  mpl-lang lib ............... ok. 91 passed; 0 failed
  mpl-lang tests/parse.rs ... ok.  3 passed; 0 failed
  mpl-language-server ....... ok. 483 passed; 0 failed
  mpl-language-server-wasm .. ok.  5 passed; 0 failed
  mpl-playground ............ ok. 84 passed; 0 failed
  (total 666 passed; 0 failed)
$ cargo clippy --workspace --all-targets     # 0 warnings, 0 errors
$ cargo fmt --check                          # clean
```

# Phase A — Tier-1 RW parity (editor grammar → Rust, build fix, hygiene)

Mirrors the RW reference worktree (`depest-pi-rw`). Three deliverables; all
workspace gates stay green. Changes are left **unstaged** (slice state is the
staged baseline).

## Deliverable 1 — `query_spec` build fix (E0603)

`extra/mpl-language-server/src/lib.rs` imported `mpl_lang::stdlib::STDLIB`, but
`stdlib` is a **private** module; `STDLIB` is re-exported at the crate root
(`pub use stdlib::STDLIB;` in `src/lib.rs`). One-line fix:

```rust
- use mpl_lang::stdlib::STDLIB;
+ use mpl_lang::STDLIB;
```

Before: `cargo build -p mpl-language-server --features examples` → `error[E0603]:
module 'stdlib' is private`. After: builds clean (both the LS crate and the wasm
crate with `--features examples`, native and `wasm32-unknown-unknown`).

## Deliverable 2 — editor grammar parsing moved into Rust (single source of truth)

Two JS grammar fragments in `packages/mpl-codemirror/` were re-implementing rules
the Rust parser already owns. Both are now Rust + wasm, exactly as RW did.

### (a) `param $x : T;` hover scan → wasm `param_declarations`

- **Rust core unchanged**: reuses the *existing* completion-engine scanner
  `extract_declared_params` / `parse_param_decl` (already in `completions.rs`,
  identical to RW) — **no new scanner written**.
- **New** `mpl_language_server::declared_params(query) -> Vec<ParamDeclaration>`
  (editor-facing projection: `$`-prefixed name + canonical type spelling +
  `optional`), backed by a new `ParamType::canonical_name()` and a new
  editor-facing `ParamDeclaration` struct (distinct from the AST's).
- **New wasm export** `param_declarations(query) -> [{name,type,optional}]`.
- **`hover.ts`**: deleted `PARAM_LINE_RE` + `OPTION_RE`; `parseParamDeclarations`
  now delegates to `mpl.param_declarations` (try/catch → empty map when wasm
  unavailable). `KEYWORD_DOCS` kept (presentation, not migrated).

### (b) backtick-escape decision → wasm `is_plain_ident`

- **New** `mpl_lang::is_plain_ident(name)` in `src/query.rs`, **reusing the
  chumsky lexer's own char classes** `slice::is_ident_start` /
  `slice::is_ident_continue` (made `pub(crate)`) — the grammar is the single
  source of truth, not a fresh char class.
- `src/query/fmt.rs::escape_ident` now routes through `super::is_plain_ident`
  (was an inline duplicate of the class), so `Display` and editor tooling agree.
- Re-exported `pub use query::{Query, is_plain_ident};`.
- **New wasm export** `is_plain_ident(name) -> bool`.
- **`completions.ts`**: deleted `PLAIN_IDENT_RE`; `needsEscape` now calls
  `mpl.is_plain_ident` (try/catch → escape conservatively when wasm unavailable).

### Drift bug fixed (the reason this matters)

The JS `PLAIN_IDENT_RE = /^[A-Za-z][A-Za-z0-9_]*$/` rejected **leading-underscore**
idents that the grammar (`[A-Za-z_]…`) accepts. The Rust rule accepts `_foo`/`_`.
Proven by:
- Rust `src/query/tests.rs::is_plain_ident_matches_ident_grammar` (asserts `_foo`,
  `_` accepted; `1foo`, `dev.metrics`, … rejected).
- TS `completions.test.ts` "treats a leading underscore as plain".

### Test double

`__mpl-stub__.ts` gains `param_declarations` (returns `undefined`) and
`is_plain_ident` (regex `^[A-Za-z_][A-Za-z0-9_]*$`, leading-underscore-correct),
matching RW — used only by vitest (production uses the real wasm exports).

## Deliverable 3 — hygiene audit (rust-analyzer skill)

Method: `ra diagnostics` + `ra references` over the workspace.

- **Genuine dead code removed**: `extra/mpl-language-server/src/parser.rs`
  (**571 lines**) — a superseded hand-rolled lexer (`tokenize`/`Token`/`Type`,
  all `pub(crate)`) whose **only** references (verified via `ra references`) live
  in its own `#[cfg(test)] mod tests`: a classic test-cfg-test loop. It carried
  the workspace's **only** `#![allow(dead_code)]`, which masked the compiler's
  dead-code warning. Production tokenizing is `tokenize.rs::collect_tokens`, a
  distinct module with its own suite. RW deleted this file too. Removed the file
  and its `mod parser;` declaration. This drops 10 self-tests that only exercised
  the dead lexer (no production reach) — not a weakening.
- **`#[allow(dead_code)]` audit**: after removal, `rg 'allow\(dead_code\)' src
  extra` → **none**. AGENTS.md "no `#[allow(dead_code)]`" now holds tree-wide.
- **`#[cfg(test)]` blocks**: all remaining inactive-code warnings are legitimate
  test / `feature="examples"` gates — nothing test-only leaks into production.
- **Reported, not touched (ambiguous / out of scope)**: RA flags
  `unresolved macro 'command'`/`clap::ValueEnum` errors in `src/bin/mplc.rs` &
  `mplstdlib.rs` — these are rust-analyzer proc-macro-expansion false positives
  (clap derives); `cargo build/clippy` compile them cleanly, so left untouched.
  A `remove-unnecessary-else` weak-hint in `diagnostics.rs:191` is a pre-existing
  RA assist (not a clippy lint) — out of scope, not touched.

## Net code delta (Phase A only — isolated from the prior pest-removal phase)

The prior phase's deltas (`slice.rs +1621`, `src/parser.rs −1679`, `mpl.pest
−172`, `errors.rs −427`) are NOT Phase A. My contribution:

| Bucket                                            | +added | −removed |
| ------------------------------------------------- | -----: | -------: |
| Editor-grammar → Rust/wasm (purely-mine files)    |    171 |       75 |
| `is_plain_ident` + reuse wiring (query.rs/lib/fmt) |    ~21 |       ~8 |
| `declared_params`/`canonical_name`/struct (compl) |    ~44 |        0 |
| **Feature migration subtotal**                    | **~236** |  **~83** |
| Hygiene: delete dead `parser.rs`                  |      0 |      571 |

- **Feature migration net: ≈ +153 lines.** Moving two JS regexes (~5 lines) into
  Rust is *deliberately* net-positive: it buys a documented single-source-of-truth
  implementation + wasm shim + migrated parser-detail tests, and kills the
  leading-underscore drift bug. Line count is the wrong metric; drift-proofing is
  the win.
- **Including hygiene: net ≈ −418 lines** (the 571-line dead lexer dominates).

## Reuse (no re-handrolling)

- `declared_params` reuses the existing `extract_declared_params` /
  `parse_param_decl` scanner verbatim — only adds a projection map.
- `is_plain_ident` reuses the lexer's `slice::is_ident_start` /
  `is_ident_continue` predicates — the grammar's own definition.
- `escape_ident` de-duplicated: its inline char-class is gone, replaced by a call
  to `is_plain_ident`. One definition of "plain ident" now serves the parser,
  `Display`, the LS, and the editor.

## Test migration (parser-detail tests → Rust, never weakened)

- `hover.test.ts`: the 7 regex-driven `parseParamDeclarations` parser-detail tests
  (Option<T> unwrap, whitespace, missing `;`, comments, multiple) were replaced by
  3 thin-adapter tests that **mock** `mpl.param_declarations` (map-shaping, empty,
  wasm-unavailable). The parser detail is now covered Rust-side by
  `declared_params_*` (new) + the existing `extract_declared_params` /
  `parse_param_decl` suites. The `mergeSystemParamsInto` "does not overwrite"
  test builds its map directly instead of round-tripping the (now-mocked) parser.
- `completions.test.ts`: +1 test (leading-underscore is plain).

## Maintainability note — "add a new pipe rule `| dedup by <tags>`" (unchanged by Phase A)

Phase A doesn't touch grammar rules, but it does remove one editor-side burden:
adding a rule whose idents need escaping, or a new `param` type, no longer risks
JS drift — the editor reads `is_plain_ident` / `param_declarations` from the same
Rust the parser uses. Previously you'd have had to remember to update the JS
regexes in lock-step (and the leading-underscore bug shows that wasn't happening).

## Verification (pasted)

```
$ cargo build --workspace                                   # Finished, 0 errors
$ cargo build -p mpl-language-server --features examples    # Finished (was E0603)
$ cargo build -p mpl-language-server-wasm \
      --target wasm32-unknown-unknown --features examples   # Finished, 0 errors
$ bash packages/build-mpl-wasm.sh                           # built successfully
$ cargo test --workspace
  mpl-lang lib ............... ok.  92 passed   (+1 is_plain_ident)
  mpl-lang tests/parse.rs .... ok.   3 passed
  mpl-language-server ........ ok. 475 passed   (+2 declared_params, −10 dead parser.rs tests)
  mpl-language-server-wasm ... ok.   5 passed
  mpl-playground ............. ok.  84 passed
  (total 659 passed; 0 failed)
$ cargo clippy --workspace --all-targets                    # 0 warnings, 0 errors
$ cargo fmt --check                                         # clean
$ (cd packages/mpl-codemirror && npx vitest run)            # 5 files, 62 passed
$ (cd packages/mpl-codemirror && npx tsc --noEmit)          # OK (real @axiomhq/mpl types)
$ rg -n 'PARAM_LINE_RE|OPTION_RE|PLAIN_IDENT_RE' packages   # (no matches)
$ rg -n 'allow\(dead_code\)' src extra                      # (no matches)
```

### Test count reconciliation

| Suite                    | Baseline | Phase A | Δ                                         |
| ------------------------ | -------: | ------: | ----------------------------------------- |
| mpl-lang lib             |       91 |      92 | +1 `is_plain_ident_matches_ident_grammar` |
| mpl-lang tests/parse.rs  |        3 |       3 | —                                         |
| mpl-language-server      |      483 |     475 | +2 `declared_params`, −10 dead-lexer self-tests |
| mpl-language-server-wasm |        5 |       5 | —                                         |
| mpl-playground           |       84 |      84 | —                                         |
| **cargo total**          |  **666** | **659** | net −7 (all from dead-code removal)       |
| TS vitest (5 files)      |       65 |      62 | −7 hover parser-detail, +3 adapter, +1 underscore |
