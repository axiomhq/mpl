# WI (winnow) port — evaluation report

Scope: the representative slice only (file envelope, `param` decls, `source` +
relative `time_range` + `as`, one filter rule, `align`, comments/whitespace as
trivia). See `SKIPPED.md` for everything intentionally left out. The `pest` path
is kept intact and still drives `compile`, `parse_*`, completions, diagnostics,
lints and hover; only **highlighting** and a new `compile_winnow` test path go
through winnow.

## 1. Library & version

- **winnow `1.0.3`** (`Cargo.toml`: `winnow = "1"`). Zero transitive
  dependencies (`cargo tree -p winnow` is a leaf).
- Builds on **stable Rust 1.95.0**; locked to the crate's `rust-version =
  1.92.0` (`cargo add` reported "Locking 1 package to latest Rust 1.92.0
  compatible version"). No nightly, no feature flags beyond winnow defaults
  (`alloc, ascii, binary, parser, std`).

## 2. Approach — how the pest grammar maps onto winnow

| pest concept | winnow realization |
|---|---|
| A `rule` | A `fn name(&mut Input) -> ModalResult<T>` building the AST node directly. |
| `a ~ b` (sequence) | sequential `?`-calls, or a `(a, b)` tuple parser. |
| `a \| b` (ordered choice) | `alt((a, b))` (tuples cap at 10 branches — the lexer nests two). |
| `a?` / `a*` | `opt(a)` / `repeat(0.., a)`. |
| silent `WHITESPACE`/`COMMENT` | a `trivia()` parser consumed before each token (structural side); the lexer keeps them as real tokens (highlight/formatter side). |
| atomic `@{…}` idents | `(one_of(start), take_till(end)).take()`. |
| committed point (no backtrack) | `cut_err(body).context(Label("…"))` after a rule's keyword. |
| `State` (params, stdlib) | winnow `Stateful` stream carrying `Ctx { params, diags }`; `$x` resolves to its `ParamDeclaration` inline, exactly like `parser::State`. |
| spans (`pair.as_span()`) | `LocatingSlice` + `.with_span()`; absolute byte offsets survive the mid-parse stream re-wrap that swaps empty→declared params. |

Two layers (`src/wparser/`):

- **`lex.rs`** — a flat, *total* lexer (`highlight`) for the editor. Branch order
  classifies a token, the final `any` branch guarantees progress, so it never
  fails. Keyword/type tables mirror the old `language.ts` regexes. The one
  non-flat case is double-quoted strings: `lex_string` descends into `${ … }`
  interpolation and re-lexes the embedded expression with the same `single_token`
  classifier (see *§String-interpolation highlighting*).
- **`grammar.rs`** — structural combinators that build the real `query.rs` AST,
  plus pipe-boundary recovery and the `ContextError → ParseError` mapping.

**The `== #/regex/` vs `== $param` ambiguity** is handled exactly as pest does:
`filter_rhs` parses `tag == $p` as `Cmp::Eq(Expr::Param)` and `tag == #/…/` as
`Cmp::RegEx`; the *shared* `ParamTypecheckVisitor` then rewrites `Cmp::Eq(Param)`
to `Cmp::RegEx` when the param is `Regex`-typed. Because `compile_winnow` reuses
the same three visitor passes as `compile`, this falls out for free
(`tests::eq_regex_param_is_rewritten_to_regex_cmp`,
`tests::eq_string_param_stays_value_cmp`).

## 3. Code stats

| File | Lines | Role |
|---|---:|---|
| `src/wparser/grammar.rs` | 1062 | structural parser + AST + recovery + error mapping |
| `src/wparser/lex.rs` | 355 | flat highlight lexer (+ `${ … }` interpolation descent) |
| `src/wparser/mod.rs` | 63 | `HlKind`/`HlToken` + re-exports |
| **parser subtotal (non-test)** | **1480** | post full-port + interpolation highlight fix |
| `src/wparser/grammar/tests.rs` | 300 | parse/recovery/equivalence tests |
| `src/wparser/lex/tests.rs` | 179 | lexer/incomplete-input/interpolation tests |

For comparison, `src/parser.rs` (pest tree-walk) is 1679 lines for the **whole**
grammar. Per feature, winnow is more verbose (the AST build lives in the parser
instead of a separate `.pest` file, and winnow's rustfmt style puts one
combinator arg per line). The win is not LOC; it's recovery + trivia + a single
Rust source of truth for the editor.

- **New deps:** `winnow` only (+ `Cargo.lock`). No transitive deps.
- **Files touched (tracked):** `Cargo.toml`, `src/lib.rs` (+`compile_winnow`),
  `src/parser.rs` (made `unescape`/`unescape_and_trim` `pub(crate)` — reused, no
  duplication), `extra/mpl-language-server/src/tokenize.rs` (199→72),
  `extra/mpl-language-server/src/tokenize/tests.rs` (rewritten),
  `packages/mpl-codemirror/src/language.ts` (147→83). New: `src/wparser/`.

## 4. Error recovery — supported? how?

**Supported, manually.** winnow has no recovery on stable (`unstable-recover` is
off). The pragmatic strategy (`parse_query_body`):

1. Parse `source` once (a hard requirement; failure ⇒ `query = None`, one error).
2. Loop over `|`-delimited clauses. Each clause parser matches its keyword with a
   **backtracking** atomic `keyword()`, then `cut_err`s its body so a malformed
   body is a *committed* failure left at the exact offset.
3. On clause failure, **resync to the next top-level `|`** (`resync_to`, which
   skips strings/regex/backtick idents so a `|` inside them is not a boundary),
   record one diagnostic, and continue. The trailing valid clauses still parse.

A diagnostics **sink** in `Ctx` lets semantic checks (undefined param, bad regex,
unknown align fn, bad type) record *typed* `ParseError`s mid-parse; the boundary
only synthesizes a generic syntax error when the sink did not already grow, so
errors are never double-reported.

**Effort:** the recovery machinery is ~120 lines (`parse_query_body`,
`resync_to`, `skip_delimited`, `record_*`) plus the discipline of atomic
`keyword`/`symbol` (reset-on-failure) and `cut_err`+`.context()` at each clause.
The single most important trick is distinguishing `ErrMode::Backtrack` ("not this
clause, try the next `alt`") from `ErrMode::Cut` ("this clause, but broken") —
that one distinction is what makes resync land in the right place. Call it a
**Medium** effort: not free, but localized and reusable for every future pipe
rule.

**Demo (incomplete + multi-error), real output:**

```
INPUT: ds:metric | filter region == | align oops | filter env == "p"
errors = 2
  SyntaxError { span: (offset 29, len 1), label: "invalid filter expression", message: "error in filter expression" }
  SyntaxError { span: (offset 36, len 1), label: "invalid align clause",      message: "error in align clause" }
```

Both bad clauses are reported, and the trailing `filter env == "p"` is still
parsed into the AST (`tests::recovers_at_pipe_boundaries_collecting_multiple_errors`
asserts `query.is_some()`, `errors.len() == 2`, `filters == [env]`). Offset 29 is
the empty RHS at the first `|`; offset 36 is `oops`.

## 5. Lossless CST / trivia — preserved? formatter-ready?

- **Highlight lexer = lossless.** `highlight()` returns tokens that *cover the
  input exactly* — no gaps, no overlaps, sorted — including `Whitespace`,
  `Comment`, and `Unknown` kinds. `lex/tests.rs::assert_covers` enforces this as
  a property over normal, incomplete, and garbage inputs. That byte-exact token
  stream is the formatter prerequisite that pest discards (pest's
  `WHITESPACE`/`COMMENT` are silent).
- **Structural parser = trivia-skipping**, on purpose: `trivia()` drops
  whitespace/comments between tokens (same role as pest's silent rules), so the
  AST stays clean. A formatter would consume the lexer's lossless stream, not the
  AST.
- Not yet a full typed CST (no concrete-syntax-tree node hierarchy). The lexer
  gives a flat lossless token list, which is enough for a token-driven formatter
  but not for tree-aware reflow. That's a deliberate slice boundary.

## 6. Diagnostics — multi-error? span quality?

- **Multi-error: yes** (§4) — every clause contributes independently.
- **Spans: byte-accurate**, from `LocatingSlice`. Semantic errors point at the
  offending token (e.g. the unknown function), not the line.
- **Mapping:** winnow `ContextError` (its `StrContext::{Label,Expected}` stack)
  is folded into the repo's existing miette `ParseError::SyntaxError {span, label,
  message, suggestion}` in `record_context_error`, so the new errors render
  through the same miette pipeline as the pest ones. Typed errors
  (`UndefinedParam`, `UnsupportedAlignFunction`, `InvalidRegex`,
  `ParamDefinedMultipleTimes`, …) are emitted directly.

Sample (`tests::error_span_points_into_the_source`):

```
INPUT: ds:metric | align using nope_fn
  Unsupported align function: nope_fn   (span lands on `nope_fn`)
```

Honest limitation: clause-level syntax messages are coarse ("invalid filter
expression") because the hand-rolled `keyword`/`symbol` helpers produce empty
`ContextError`s; only the clause wrapper's `.context(Label(…))` survives. Adding
`.context(Expected(…))` to leaf parsers would sharpen them. The pest path's
bespoke `From<PestError>` formatter (keyword/operation grouping, jaro typo
suggestions) is richer here and is unchanged.

## 7. Editor: logic moved into Rust — THE HEADLINE

**Deleted from `language.ts` (147 → 83 lines):** all 7 grammar regexes —
`MPL_KEYWORDS`, `COMMENT_RE`, `STRING_RE`, `REGEX_RE`, `NUMBER_RE`, `BOOL_RE`,
`TYPE_RE` — plus `findMatches`, `findKeywordsInGaps`, `resolveAndBuild`, the
`TokenEntry` priority model, and the two-path (wasm-tokens-else-regex) fallback.
`grep -c "_RE =\|MPL_KEYWORDS =\|new RegExp" language.ts` → **0**. `language.ts`
now just calls `mpl.tokenize(doc)` and maps token kinds to CodeMirror
decorations.

**Moved into Rust:** `tokenize.rs` (199 → 72 lines) is now a thin adapter over
`mpl_lang::wparser::highlight`. The grammar knowledge lives once, in winnow.

**Proof it works on incomplete input (built wasm, run under Node):**

```
"metric:cpu | filter region == " -> 7 tokens
   variable:"metric" punctuation:":" variable:"cpu" punctuation:"|" keyword:"filter" variable:"region" operator:"=="
"metric:cpu | align using "      -> 6 tokens
   variable:"metric" ... keyword:"align" keyword:"using"
"ds:metric | filter tag == #/a|b/ and x is bool" -> 12 tokens
   ... regexp:"#/a|b/" keyword:"and" variable:"x" keyword:"is" type:"bool"
"// a comment\nds:metric"         -> 4 tokens
   comment:"// a comment" variable:"ds" punctuation:":" variable:"metric"
```

The old `collect_tokens` returned `None` (→ regex fallback) for the first two;
now Rust returns tokens directly. Comments are highlighted from Rust (previously
a TS-only regex). `#/a|b/` is one regex token even though it contains `|`.

**What remains in TS, and why:** only ~20 lines — the `TokenType`→`Decoration`
map and the `RangeSetBuilder` loop. That's CodeMirror glue, not grammar, and
can't move into Rust. **What remains on pest:** completions
(`completions.rs` byte scanner) and language-server diagnostics
(`compute_diagnostics` → `compile`) — out of slice; migrating them would regress
non-slice coverage and is tracked in `SKIPPED.md`.

## 8. Maintainability probe — adding `| dedup by <tags>`

No implementation, just the touch list (mirrors how `align` was added):

1. **AST (`src/query.rs`)** — add a `Dedup { span, tags: Vec<String> }` struct and
   an `Aggregate::Dedup(Dedup)` variant. *(M-ish: the AST is shared, so the
   visitor trait and `Display`/`fmt` must learn it too — same as any new node.)*
2. **`src/wparser/grammar.rs`** — add `fn dedup_rule(&mut Input) -> PResult<…>`:
   `keyword("dedup")` then `cut_err`(`keyword("by")` + a `tags` parser =
   `separated(1.., ident_name, ",")`). Add one branch to the `pipe_clause` `alt`.
   Add a `tags` helper (reused by future `group`/`bucket`). *(S: ~25 lines, one
   new `alt` branch; recovery is automatic — a bad `dedup` already resyncs.)*
3. **`src/wparser/lex.rs`** — add `"dedup"` to the `KEYWORDS` table. *(XS: one
   line; highlighting then works on incomplete `| dedup by ` instantly.)*
4. **`packages/mpl-codemirror/src/language.ts`** — **nothing.** It consumes Rust
   tokens; `dedup` highlights for free once the lexer knows the word.
5. **Tests** — a parse test + a recovery test in `grammar/tests.rs`, a highlight
   test in `lex/tests.rs`. *(S.)*
6. **Old pest path** — still needs the same `mpl.pest` rule + `parser.rs` arm if
   you want `compile`/completions to accept it; that's the duplication this port
   is trying to retire.

Net: the winnow + editor side is **2 keyword/branch edits + an AST node**, and the
TS frontend needs zero changes — the maintainability story the project wants.

## 9. Build & WASM

- **Stable build:** `cargo build`, `cargo test --workspace` (99 lib + 467
  language-server + others, all green), `cargo clippy --workspace --all-targets`
  (0 warnings, lib is `#![deny(warnings, clippy::pedantic)]`), `cargo fmt --check`
  all pass.
- **wasm32:** `cargo build -p mpl-language-server-wasm --target
  wasm32-unknown-unknown` succeeds.
- **wasm-pack:** `bash packages/build-mpl-wasm.sh` (target web, `wasm-release`
  profile) succeeds — "Your wasm pkg is ready to publish".
- **Bundle:** `mpl_bg.wasm` = **1.91 MB raw / 580 KB gzip**. No clean
  before/after baseline exists (no prior `.wasm` committed; `packages/mpl/` is
  gitignored). The bundle currently contains **both** pest and winnow (pest is
  kept for skipped rules); winnow itself is dependency-free, so the incremental
  cost is small and a future pest removal would *shrink* the bundle.
- `tokenize`, `completions`, `diagnostics`, `parse_json` all still export.

## 10. Migration effort & blast radius vs `src/parser.rs`

- **Additive, low blast radius.** `parser.rs` and the `pest`/`Rule` API are
  untouched except making two `unescape*` helpers `pub(crate)` (reused, not
  duplicated). The new parser is a sibling module; `compile` still uses pest.
- **AST unchanged.** `compile_winnow` produces the same `query.rs` types and
  reuses the same typecheck/group/option visitors —
  `tests::matches_pest_parser_on_a_representative_query` asserts the winnow AST
  renders **identically** to the pest AST for a representative query.
- **Editor blast radius is the point:** `tokenize.rs` and `language.ts` changed,
  but their public contract (`tokenize(query) -> Token[]`) is the same shape, so
  downstream CodeMirror code is unaffected. The one behavior change is
  intentional: tokens now come back for incomplete input instead of `null`.
- A full migration (drop pest) means porting the ~14 skipped constructs in
  `SKIPPED.md` and repointing completions/diagnostics — mechanical, gated by the
  `SKIPPED(step2)` markers.

## 11. Risks / unknowns

- **Flat-lexer precision.** Highlighting is token-level, so a tag named `filter`
  is coloured as a keyword. Same as the old regex fallback; acceptable for an
  editor, but a regression vs the *old grammar-aware* wasm-token path that ran
  only on fully-valid input. Net UX is better (works while typing).
- **Coarse syntax messages** (§6) until leaf parsers get `.context()`.
- **Verbosity / combinator ergonomics.** winnow's free `Error` type parameter
  bites whenever a result is discarded (`.is_ok()`), forcing typed helper
  wrappers (`lit`, `at_eof`). The AST-in-parser style is wordier than a
  declarative `.pest` grammar.
- **No typed CST yet** — formatter can do token reflow but not tree-aware reflow.
- **Two parsers to keep in sync** during the transition (pest for skipped rules,
  winnow for the slice). Tracked, but real maintenance until pest is removed.
- **Bundle** carries both parsers for now.

## 12. Verdict — keep

**Keep, and converge onto it.** winnow cleanly meets the two goals pest can't:
robust **incomplete-input highlighting served from Rust** (the headline — proven
end-to-end through wasm, with the entire `language.ts` grammar duplication
deleted) and **multi-error recovery** with byte-accurate spans (resync-at-`|`).
The AST, visitors and the tricky `== $param` regex ambiguity are reused verbatim,
so correctness risk is low and the editor frontend stops re-implementing the
grammar in JS. The costs are honest — more verbose per rule, coarser syntax
messages until leaf `.context()` is added, and a flat (not tree) CST — but they
are incremental polish, not blockers. For "how easy is it to add the next pipe
rule," the answer here is: one `alt` branch + one keyword-table entry + zero TS,
which is exactly the maintainability the project is optimizing for.

## Phase 2 — full pest removal

The slice is finished and **pest is gone**. The hand-written `winnow` grammar in
`src/wparser/` is now the only parser; `compile()` (and therefore every wasm
shim, the language server, completions, diagnostics, lints and hover) runs
through it.

### LEDGER

**pest removed?** **yes.** Proof (all run clean):

```
$ rg -n 'pest' --type toml                 # → (no matches)
$ rg -cn 'use pest|pest::' src extra       # → (no matches)
$ grep -rn 'SKIPPED(step2)' src extra       # → (no matches)
$ rg -n 'MPLParser|Rule::' src extra packages
  # → 3 historical doc-comment mentions only (no code)
```

Deleted: `src/mpl.pest`, `src/parser.rs` (`MPLParser`/`Rule` + the whole pest
tree-walk), `src/parser/tests.rs`, `extra/mpl-language-server/src/visit.rs`
(the `PairVisitor`), the `From<PestError>`→`ParseError` reconstruction in
`src/errors.rs`, and `pest`/`pest_derive` from every `Cargo.toml` (lib,
language-server, **and** the playground, which only carried it transitively).

**Tests: pass count before vs after.**

| crate                      | before | after | note |
|----------------------------|------:|------:|------|
| `mpl-lang` (lib)           |    99 |    95 | −11 pest-tree unit tests deleted with `parser.rs`; +7 new winnow tests |
| `mpl-lang` (`tests/parse`) |     3 |     3 | every `tests/examples/*.mpl` + `tests/errors/*.mpl` still parses through winnow |
| `mpl-language-server`      |   467 |   467 | **all green, unchanged** |
| `mpl-language-server-wasm` |     5 |     5 | unchanged |
| `mpl-playground`           |    84 |    84 | unchanged |
| **workspace total**        | **658** | **654** | |

Plus the wasm artifact integration test (`node tests/wasm/test-wasm.mjs`):
**38/38 pass** against the freshly built `mpl_bg.wasm`.

**Tests adapted (and why):**

- `src/parser/tests.rs` (11 tests) **deleted, not weakened** — they asserted
  pest-tree internals (`MPLParser::parse(Rule::time, …)`, `pair.as_rule()`),
  which cannot exist without pest. Their *behaviour* (relative time, timestamp,
  RFC3339, number parsing, params, group/bucket) is covered by `src/tests.rs`
  (which now runs through winnow) and by **7 new** `wparser::grammar::tests`
  (`parses_timestamp_and_rfc3339_and_modifier_times`,
  `parses_map_eval_and_map_fn_and_group_and_bucket`,
  `join_and_replace_are_not_supported`, `parses_compute_query`,
  `parses_ifdef_else_and_extend_and_directives`,
  `parses_string_interpolation_into_fragments`,
  `param_value_parses_each_declared_type`).
- `wparser::grammar::tests::matches_pest_parser_on_a_representative_query` →
  renamed `representative_query_structure`: it compared the winnow AST against
  the pest AST; with pest gone that is a tautology, so it now asserts the
  parsed structure directly (a round-trip is impossible because the `Display`
  impl PII-redacts string constants — a pre-existing property).
- `mpl-playground` `compile_errors_include_location_and_expected_tokens`: not
  adapted; instead the winnow `map` clause now attaches an
  `Expected("a number")` context so the missing-operand error reads
  `= expected a number`, matching the test's `"= expected"` assertion. (Faithful
  to pest's behaviour, sharpens the message.)
- The span-sensitive `mpl-language-server` diagnostics tests
  (`dataset_no_colon_error_highlights_dataset` → `(0,2)`, `ds:` → EOF,
  `ds:[1h..]` → the `[`, …) pass **unchanged**: `metric_id`/`metric_name`
  reproduce pest's farthest-failure spans exactly (captured the pest ground
  truth first, then matched it).

**Net code delta** (winnow is wordier per feature — real numbers):

| path | Δ lines | what |
|---|---:|---|
| `src/parser.rs` (deleted) | −1680 | pest tree-walk + lowering |
| `src/mpl.pest` (deleted) | −138 | PEG grammar |
| `src/parser/tests.rs` (deleted) | −167 | pest-internal unit tests |
| `src/errors.rs` | −318 | −`From<PestError>` / `friendly_rule*` / `rules_keywords` / suggestion scanner (~360) ; +`UnsupportedRule`+`ParseParamError` (~45) |
| `extra/…/visit.rs` (deleted) | −62 | `PairVisitor` |
| `extra/…/lints.rs` | −30 | pest tree-walk → lexer token scan |
| `extra/…/completions.rs` | −23 | pest `Rule::source` extraction → `wparser::parse_file` |
| `src/wparser/grammar.rs` | **+877** | all remaining constructs + lowering + `unescape`/`param_value` re-homed |
| `src/wparser/grammar/tests.rs` | +132 | new construct tests |
| `src/query.rs` | −8 | `param_value` rewire |
| **NET** | **≈ −1.42k** | despite winnow's per-rule verbosity |

The headline: pest's grammar+lowering cost **1818** lines (`mpl.pest` 138 +
`parser.rs` 1680, split across two files and a generated `Rule` enum); the
winnow equivalent *added* **+877** lines to `grammar.rs` to absorb the entire
remaining grammar **and** its AST lowering in one Rust source — net −941 on the
parse path alone, before counting the −318 of error-reconstruction machinery the
new parser makes redundant.

**Reuse (kept the slice's discipline):**

- `parser::unescape` / `unescape_and_trim` were the canonical leaf helpers the
  slice already reused. With `parser.rs` gone they are now `pub(crate)` in
  `wparser/grammar.rs` — **moved, not re-handrolled**; every string/regex/
  backtick leaf still routes through the one `unescape`.
- All three post-parse passes (`ParamTypecheckVisitor`, `GroupCheckVisitor`,
  `OptionCheckVisitor`) and the whole `visitor.rs` walker are **reused verbatim**
  — including the `== $param` vs `== #/regex/` rewrite, so that ambiguity still
  falls out for free.
- The AST (`query.rs`), `linker` stdlib lookups (`map_fn`/`align_fn`/`group_fn`/
  `compute_fn`/`bucket_function`), `EncodableRegex`, `Metric`/`Dataset`,
  `BucketSpec`/`BucketType`/`ConversionMethod`: all reused unchanged.
- `param_value` external entry point is now one winnow function reusing the same
  leaf parsers (`relative_time`, `string_raw`, `ident_name`, regex), replacing
  the pest `Rule::param_value` + `parse_param_value` pair.

**Duplicates removed:** the slice phase did **not** duplicate helpers (it
imported `parser::unescape_and_trim`), so there was nothing to dedupe — the
discipline held. The redundant *pest-only* machinery deleted: the entire
`From<PestError>` formatter (keyword/operation grouping, jaro typo-suggestion
scanner, `friendly_rule` table, `token_length` expander), the
`ParseError::{Unexpected, UnexpectedTokens}` variants (only the pest tree-walk
ever produced them), and the `PairVisitor`.

**Simplifications enabled by leaving pest:**

- **Diagnostics now run on the recovering parser.** `compute_diagnostics` →
  `compile` → `wparser::parse_file`, which collects *every* error in one pass
  (the slice already demonstrated multi-error recovery; it is now the production
  path). The 467 language-server tests — including the byte-exact syntax-error
  spans — pass on it.
- **Lints dropped the tree-walk.** `detect_hints` no longer builds a pest tree
  and walks it with a `PairVisitor`; it gates on a clean `parse_file` and scans
  the flat, lossless lexer token stream for `filter`-keyword and unnecessary-
  backtick hints. `visit.rs` is gone.
- **Error type is pest-free.** `NotSupported` carries a small `UnsupportedRule`
  enum instead of the generated `Rule`; the ~360-line `PestError` reconstruction
  is deleted because winnow emits `ParseError` (typed or `SyntaxError`) directly.

**Completions byte-scanner — the honest WI tradeoff:**

I did **not** replace the `completions.rs` byte-scanner with the recovered parse,
and that is the right call *for winnow*. Completions are fundamentally
**cursor-addressed**: at an arbitrary offset inside half-typed text they must
answer "which construct am I in, and what is the partial word" (after `|`,
inside `ifdef(`, after `using`, mid-`group by a, ▮`). winnow's recovery yields a
best-effort **AST**, not a position-addressable **CST** — it discards trivia and
the exact offsets of the incomplete fragment, so it cannot map a byte cursor to
a node. (This is exactly where RW's concrete-syntax-tree would have an edge:
a CST with node spans could host a `node_at(offset)` lookup; WI's manual-recovery
AST cannot, and building a parallel position-mapped CST would cost *more* than
the existing scanner.) So the cursor scanner stays. The **one** place pest was
actually load-bearing in completions — `extract_source_via_parser`, which parsed
the already-isolated source substring to pull out dataset+metric with correct
backtick/escape handling — is now a 12-line call into `wparser::parse_file`
(parse the small substring, read `Query::Simple.source.metric_id`). Net: the
scanner is unchanged, the pest dependency is gone, and the honest verdict is
"manual recovery makes a parse-driven completion engine *more* expensive than
the scanner, so keep the scanner".

**Maintainability note — re-doing `| dedup by <tags>` now that the full parser
exists:**

1. **AST (`src/query.rs`)** — add `Dedup { span, tags: Vec<String> }` + an
   `Aggregate::Dedup(Dedup)` variant; teach `visitor.rs` and `query/fmt.rs`
   about it (same as any new node — shared-AST cost, unchanged by the port).
2. **`src/wparser/grammar.rs`** — add `fn dedup_rule(&mut Input) -> PResult<Aggregate>`:
   `keyword("dedup")` then `cut_err`(`keyword("by")` + the **already-existing**
   `tags(input)` helper). Add one branch to the `pipe_clause` `alt`. ~12 lines;
   recovery is automatic (a bad `dedup` already resyncs at the next `|`).
3. **`src/wparser/lex.rs`** — add `"dedup"` to the `KEYWORDS` table (one line);
   highlighting then works on incomplete `| dedup by ` instantly.
4. **`packages/mpl-codemirror/`** — **nothing** (consumes wasm tokens).
5. **Tests** — a parse test + a recovery test.

The decisive difference from the slice-era walkthrough: step 6 ("also add the
`mpl.pest` rule + `parser.rs` arm") **no longer exists** — there is a single
grammar source of truth. Adding a pipe rule is now *one `alt` branch + one
keyword-table entry + the shared AST node*, with the `tags` helper already
written. The port deleted exactly the duplication this walkthrough used to call
out.

### String-interpolation highlighting (post-removal correctness fix)

**The gap.** The grammar already split interpolated literals correctly: at the
*AST* level, `grammar::string_expr` (`src/wparser/grammar.rs`) breaks
`"…${ expr }…"` into `StringFragment`s (`Text` / `Expr`). But the *highlight*
path did not. The flat lexer's old `string_token` matched the whole literal
(quote-to-quote, escape-aware) and `highlight()` emitted it as **one opaque
`String` token**, so `"host ${ $h } end"` highlighted as a single string and the
embedded `$h` lost its variable colour. This regressed vs the old pest
highlighter's 3-token model.

**The fix (highlighting only).** Replaced the opaque `string_token` with a
modal `lex_string` that descends into `${ … }`. It reuses — does **not**
duplicate — two existing things: (1) the same run-splitting logic as
`string_expr` (escape-aware literal runs, stop at `${`, a lone `$` is literal),
and (2) the existing `single_token` word/keyword/number classifier to lex the
embedded expression (so an embedded ident/param is `Variable`, a number is
`Number`, …). The interpolation delimiters `${`/`}` and the interior whitespace
carry no colour and are emitted as trivia, so the meaningful highlight stream is
fragment / expr / fragment. Both the lexer entry point and the embedded-expr
loop route strings through the one `lex_string`, so there is a single string
lexer (no second grammar). The lexer stays *total* — every byte is still covered
and mid-edit input (`"a ${ $b`, `"${`, `"${x}${y}`) never panics.

**Token sequence** (verified via a throwaway `cargo run --example`, since WI has
no `node_at` dump):

```
source: "host ${ $h } end"
  String("\"host ")   0..6      ┐ meaningful (trivia dropped):
  Whitespace("${")    6..8      │   String("\"host ")
  Whitespace(" ")     8..9      │   Variable("$h")
  Variable("$h")      9..11     │   String(" end\"")
  Whitespace(" ")     11..12    │
  Whitespace("}")     12..13    │  → the 3-token String / Variable / String model
  String(" end\"")    13..18    ┘
```

**Tests updated.** `mpl-language-server` `whole_string_is_one_token` (which
*pinned* the single-token regression) → `string_interpolation_splits_into_subtokens`,
now asserting the full `String("\"host ") / Variable("$h") / String(" end\"")`
sub-token sequence. Added three lexer tests in `src/wparser/lex/tests.rs`
(`string_interpolation_descends_into_braces`, `string_interpolation_embeds_number`,
`unterminated_interpolation_never_panics`). No test weakened; counts: lib
95 → 98, workspace 654 → 657, all green.

**Honest scope — NOT a formatter win.** This is a *highlighting* correctness fix,
nothing more. WI still has **no position-addressable CST**: the AST carries the
embedded `Expr` but not trivia or the literal's exact byte offsets, so this does
**not** enable the trivia-preserving-formatter benefit a CST library (RW) would
get. `lex_string` rebuilds the fragment spans directly from the byte stream for
highlighting; it is not a reusable concrete-syntax tree. The completions
byte-scanner residual below is unchanged and unaffected.

Net delta for this fix: `lex.rs` 252 → 355 (+103; −10 for the deleted
`string_token`, +113 for `lex_string`/`lex_interpolation_body`/`string_run_body`
+ small helpers), tests +~38. This is an *addition* (highlight precision), not a
removal — stated plainly so the ledger is not misread as more pest deletion.

### Verification (pasted)

```
cargo build --workspace                         → Finished (clean)
cargo build -p mpl-language-server-wasm \
  --target wasm32-unknown-unknown               → Finished (clean)
bash packages/build-mpl-wasm.sh                 → "Your wasm pkg is ready to publish"
                                                   mpl_bg.wasm = 1.76 MB raw / 539 KB gzip
                                                   (was 1.91 MB / 580 KB with pest+winnow both in;
                                                    dropping pest shrank it, as predicted)
node tests/wasm/test-wasm.mjs                    → 38 tests: 38 passed, 0 failed

cargo test --workspace   (654 → 657: +3 interpolation highlight tests)
  mpl-lang lib ............ 98 passed; 0 failed   (was 95; +3 lex interpolation tests)
  mpl-lang tests/parse ....  3 passed; 0 failed
  mpl-language-server .... 467 passed; 0 failed   (1 test renamed/rewritten, count unchanged)
  mpl-language-server-wasm   5 passed; 0 failed
  mpl-playground ......... 84 passed; 0 failed

cargo clippy --workspace --all-targets          → 0 warnings (lib is deny(warnings, pedantic))
cargo fmt --check                               → clean

rg -n 'pest' --type toml                        → (no matches)
rg -cn 'use pest|pest::' src extra              → (no matches)
```

### Verdict — full scope: **converged onto winnow; pest fully removed.**

The full grammar (map/group/bucket/join/replace/extend/sample/ifdef-else,
compute queries, all time variants incl. RFC3339/timestamp/`±` modifier,
signed `inf`, string interpolation, directive lowering, escaped param idents,
the `param_value` entry point) is ported; every pre-existing test passes; the
editor highlights/diagnoses from Rust→WASM with no JS grammar; and the bundle
got *smaller*. The one principled non-removal — the completions cursor scanner —
is documented as cheaper to keep than to drive from a recovery AST, which is the
honest WI-vs-RW boundary: winnow gives a great recovering parser and a lossless
lexer, but not a position-addressable CST, so cursor-context completion stays
hand-rolled. Per-feature winnow is more verbose, yet removing pest's parallel
grammar + lowering + error reconstruction still nets **≈ −1.4k lines**.

## Phase A — Tier-1 parity with the RW reference

Goal: mirror the RW worktree (`/Users/heinzgies/Projects/depest-pi-rw`) exactly
for three Tier-1 items — a build-bug fix, moving the last editor-side grammar
parsing into Rust→WASM, and a hygiene pass. WI is AST-only (`mpl_lang::wparser`,
winnow); RW is CST-only (rowan). The deliverables are architecture-independent,
so the Rust source, the TS adapters and the test doubles are now **byte-identical
to RW** (verified by `diff`), while the WASM is backed by WI's own winnow grammar.

### (1) `query_spec` build bug — fixed

`extra/mpl-language-server/src/lib.rs` imported `mpl_lang::stdlib::STDLIB`, but
`stdlib` is a private module (`mod stdlib;`); `STDLIB` is re-exported at the crate
root (`pub use stdlib::STDLIB;`). Under `--features examples` this was a hard
`E0603: module 'stdlib' is private`. Changed the import to `use mpl_lang::STDLIB;`
(one line), exactly as RW.

```
# before
$ cargo build -p mpl-language-server --features examples
error[E0603]: module `stdlib` is private  --> extra/mpl-language-server/src/lib.rs:53:19
# after
$ cargo build -p mpl-language-server --features examples              → Finished
$ cargo build -p mpl-language-server-wasm --features examples         → Finished
$ cargo build -p mpl-language-server-wasm --target wasm32-unknown-unknown --features examples → Finished
```

### (2) Editor-side grammar parsing → Rust/WASM (single source of truth)

Two JS hand-parsers that duplicated grammar were deleted and replaced by WASM
exports backed by the parser's own definitions:

**`param_declarations(query)` — replaces `PARAM_LINE_RE` / `OPTION_RE` in
`hover.ts`.**
- Rust: added `ParamType::canonical_name`, an editor-facing `ParamDeclaration`
  (`{name, type, optional}`, serde), and `pub fn declared_params(query)` in
  `extra/mpl-language-server/src/completions.rs`. `declared_params` **reuses the
  existing `extract_declared_params`** (the Rust param scanner the completion
  engine already uses) and projects it into the canonical-spelling shape — no new
  scanner. Re-exported from the language-server `lib.rs`.
- WASM: `pub fn param_declarations(query) -> JsValue` in
  `extra/mpl-language-server-wasm/src/lib.rs`.
- TS: `hover.ts::parseParamDeclarations` now calls `mpl.param_declarations(doc)`
  (try/catch → empty map when WASM unready). `PARAM_LINE_RE`/`OPTION_RE` deleted.
  `KEYWORD_DOCS` kept (presentation, not grammar).

**`is_plain_ident(name)` — replaces `PLAIN_IDENT_RE` in `completions.ts`, and
fixes a real drift bug.**
- Rust: added `pub fn is_plain_ident(name)` in `src/query.rs` (the single source
  of truth for the `IDENT` rule `[A-Za-z_][A-Za-z0-9_]*`), re-exported at the
  crate root (`pub use query::{Query, is_plain_ident};`). Routed `query/fmt.rs::
  escape_ident` through it (was re-spelling the char class inline → deduped).
- WASM: `pub fn is_plain_ident(name) -> bool` delegating to `mpl_lang::is_plain_ident`.
- TS: `completions.ts::needsEscape` now calls `mpl.is_plain_ident` (try/catch →
  escape conservatively when WASM unready); `escapeIdent`/`applyTextForIdent` route
  through `needsEscape`. `PLAIN_IDENT_RE` deleted.
- **Drift fix:** the old JS regex was `^[A-Za-z][A-Za-z0-9_]*$` — it **rejected
  leading-underscore idents the grammar accepts** (`_foo`, `_`). The Rust rule
  matches WI's actual winnow `plain_ident` (`grammar.rs`: first char
  `is_ascii_alphabetic() || '_'`, rest `is_ascii_alphanumeric() || '_'`). Proven
  end-to-end against the built WASM: `is_plain_ident("_foo") = true`,
  `is_plain_ident("1foo") = false`, `is_plain_ident("metrixs-dev") = false`.

**Test double:** `packages/mpl-codemirror/src/__mpl-stub__.ts` now exports
`param_declarations` (returns `undefined`) and `is_plain_ident` (mirrors the Rust
class `/^[A-Za-z_][A-Za-z0-9_]*$/`), matching RW.

`rg 'PARAM_LINE_RE|OPTION_RE|PLAIN_IDENT_RE' packages` → **NONE**.
`language.ts` already drives highlighting entirely from `mpl.tokenize` (Rust→WASM)
— no JS grammar regexes there to remove; its only divergence from RW is the
defensive token-consumption style reflecting WI's total winnow lexer vs RW's
rowan CST, not duplication.

### (3) Hygiene audit (rust-analyzer driven)

Used `scripts/ra references` + `ra diagnostics` (preferred over grep).

- **Dead code, FIXED — deleted `extra/mpl-language-server/src/parser.rs` (570
  lines) + its `mod parser;` declaration.** An abandoned hand-rolled tokenizer,
  entirely `pub(crate)`, masked by a blanket `#![allow(dead_code)]` (forbidden by
  the repo's own standards). `ra references` on `tokenize` returned **only**
  in-file hits: the definition (line 85) and 10 callers, all inside its own
  `#[cfg(test)] mod tests` — a textbook *circular test-cfg-test loop* (dead
  production code kept alive only by tests of itself). Nothing in production,
  other modules, or other tests referenced it (`mpl_lang::wparser::*` is a
  different module). RW does not carry this file at all. Deleting it removed the
  last `allow(dead_code)` in the tree (`rg 'allow(dead_code)' src extra packages`
  → none) and 10 dead-tokenizer self-tests.
- **No improperly-gated helpers found.** `cargo build --workspace` is fully
  warning-free, so no production symbol is reachable only from tests (in
  `mpl-lang`/`mpl-playground`, `#![deny(warnings)]` would turn any such case into
  a build error; the language-server crate emitted zero `dead_code` warnings).
  Existing test-only helpers (`completions.rs::cursor_in_interpolation`, the
  `CompletionResult::{kind,option_labels,keyword_apply_texts}` methods,
  `compute_completions`) are already correctly `#[cfg(test)]`-gated.
- **Reported, NOT changed (intentional):** `#![allow(unused_assignments)]` in
  `src/lib.rs` and `src/errors.rs` carry explanatory comments and gate a genuine
  type-error workaround — not dead code, left as-is.

### Tests — before vs after

Rust (`cargo test --workspace`):

| binary                      | before | after | delta |
|-----------------------------|------:|------:|-------|
| `mpl-lang` (lib)            |    98 |    99 | +1 `is_plain_ident_matches_ident_grammar` |
| `mpl-lang` (`tests/parse`)  |     3 |     3 | — |
| `mpl-language-server`       |   467 |   459 | +2 `declared_params_*`, −10 deleted `parser.rs` dead-code self-tests |
| `mpl-language-server-wasm`  |     5 |     5 | — |
| `mpl-playground`            |    84 |    84 | — |
| **workspace total**         | **657** | **650** | net −7 |

The −10 only exercised the deleted dead tokenizer (no shipping feature); **no
feature test was deleted or weakened**. Feature coverage was *strengthened* (+3).

TypeScript (`vitest run`): **65 → 62** (5 files, all pass).
- `hover.test.ts`: the 7 `parseParamDeclarations` parser-detail tests (simple,
  `Option<T>` unwrap, multiple, whitespace, missing `;`, comments, empty) →
  **migrated to Rust** and replaced by 3 thin wasm-adapter tests (maps shape /
  empty / WASM-unavailable), with the WASM boundary mocked via `vi.spyOn`. The
  migrated detail is covered by WI's pre-existing Rust `parse_param_decl_*` /
  `extract_params_*` suites **plus** the 2 new `declared_params_*` tests. Net −4.
- `completions.test.ts`: **+1** `treats a leading underscore as plain` — the
  drift-fix regression test (`needsEscape("_foo") === false`,
  `needsEscape("_") === false`). Net +1.
- All 5 codemirror `src/*.ts` files (incl. tests, stub) are now byte-identical to
  RW; `tsc` type-checks clean.

### Verification (pasted)

```
cargo build --workspace                                  → Finished (clean)
cargo build -p mpl-language-server --features examples   → Finished (was E0603)
cargo build -p mpl-language-server-wasm --features examples              → Finished
cargo build -p mpl-language-server-wasm --target wasm32-unknown-unknown --features examples → Finished
bash packages/build-mpl-wasm.sh                          → "MPL WASM package built successfully"
cargo test --workspace                                   → 650 passed; 0 failed (99+3+459+5+84)
cargo clippy --workspace --all-targets                   → 0 warnings
cargo clippy -p mpl-language-server -p mpl-language-server-wasm --features examples --all-targets → 0 warnings
cargo fmt --check                                        → clean
npm test -w @axiomhq/mpl-codemirror  (vitest)            → 5 files, 62 passed
npm run build -w @axiomhq/mpl-codemirror  (tsc)          → OK
node tests/wasm/test-wasm.mjs                            → 38 tests: 38 passed
rg 'PARAM_LINE_RE|OPTION_RE|PLAIN_IDENT_RE' packages     → (no matches)
rg 'allow(dead_code)' src extra packages                 → (no matches)

# new WASM exports, called against the freshly built mpl_bg.wasm:
is_plain_ident("_foo")        = true      # drift fix (old JS regex returned escape-needed)
is_plain_ident("1foo")        = false
is_plain_ident("metrixs-dev") = false
param_declarations("param $env: Option<string>;\nds:metric")
                              = [{"name":"$env","type":"string","optional":true}]
```

### Verdict — Tier-1 parity achieved

The build bug is fixed; the last two JS grammar duplications (`PARAM_LINE_RE`/
`OPTION_RE`/`PLAIN_IDENT_RE`) are gone, replaced by `param_declarations` /
`is_plain_ident` WASM exports backed by the parser's own rules — a single source
of truth that also fixes the leading-underscore drift the old regex silently
carried. Hygiene removed the last `allow(dead_code)` (a dead tokenizer that was
only testing itself). Everything is green across cargo build/test/clippy/fmt, the
wasm build + integration test, and vitest + tsc; the Rust and TS surface mirrors
RW exactly.
