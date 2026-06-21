# LEXER BAKE-OFF — variant **HAND-WRITTEN** (option C)

Goal: the best front end achievable with a **hand-written lexer**, as ONE
coherent model. `logos` is removed entirely; `SyntaxKind` is a plain
`#[repr(u16)]` enum; all token rules live in one modal scanner
(`src/cst/lexer.rs`). The `rowan` lossless CST, the `query.rs` AST lowering, and
the recursive-descent parser are unchanged in shape — only the *token source*
changed.

## TL;DR verdict

For a grammar that has **string interpolation**, the pure hand-written lexer
**beats the logos-hybrid baseline that shipped here** — not on declarativeness
(it loses there), but on coherence. The baseline was *already a hybrid*: logos
lexed the easy tokens, but the hard part (string/`${ }` interpolation boundary
detection) was ~120 lines of hand-written glue (`expand_string`/`lex_interp`)
sitting on top of logos, plus an awkward "lex a raw `STRING`, then re-lex and
re-position the logos cursor" double-pass. Replacing logos with a hand-written
`scan_token` **unifies both halves into one imperative model in one file**,
deletes the impedance-matching glue (`lexer.bump(end - span.end)`,
`res.unwrap_or(ERROR)`, the raw-token interception), and removes a dependency —
at a cost of ~181 net-new lines of explicit byte-matching that a `#[regex]`
attribute would otherwise express in one line each.

- **pest removed?** already gone before this variant (prior phase). **logos
  removed?** **YES** — `rg 'logos' --type toml` → none; `rg -cn 'use logos|logos::' src extra` → none; gone from `Cargo.lock`.
- **Fused lex+parse?** **Partially, deliberately.** Co-located hand-written
  modal lexer + the existing hand-written RD parser sharing **one** token model
  (`SyntaxKind`) and **one** token grammar (`scan_token`, reused for the
  interpolation interior). The "mode stack" is the Rust call stack (recursion).
  I did **not** eliminate the token `Vec` — see §1 for why (it's the
  rust-analyzer choice and the RD parser needs k-token lookahead + total
  recovery).
- **Hand-written lexer LOC:** `src/cst/lexer.rs` = **430 lines total / 299 code
  lines** (110 comment, 21 blank). Of the 299 code lines, ~181 are the
  genuinely-new token grammar (`scan_token` + `scan_*` helpers + `match_rfc3339`)
  that replaces the logos derive; the rest is the string/interpolation machinery
  (~80 code lines) moved out of `parser.rs`, plus the two driver entry points.
- **Tests:** `cargo test --workspace` → **670 passed / 0 failed** — *identical*
  to the pre-change baseline. **No test deleted, weakened, or rewritten**; the
  only test-file edits were stale-comment fixes (s/logos/hand-written/). Editor:
  CodeMirror vitest **62/62**, wasm harness **38/38**.
- **Tricky cases that needed care:** only **one** thing actually broke — the
  unterminated-backtick token shape (6 completion tests). Everything else
  (RFC3339-vs-float ordering, `#/regex/` vs `/`, escaped idents carrying `}`/`"`,
  comments carrying `}`, escaped `\${`, nested + unterminated interpolation) was
  handled correctly first try. See §3.

---

## Verification output (pasted)

```
cargo build --workspace                                   → Finished (ok)
cargo build -p mpl-language-server-wasm \
    --target wasm32-unknown-unknown                       → Finished (ok)
bash packages/build-mpl-wasm.sh                           → ok (wasm-pack, wasm-release)

cargo test --workspace:
  mpl-lang (lib)               86 passed; 0 failed
  mpl-lang tests/parse.rs       3 passed; 0 failed
  mpl-language-server (lib)   492 passed; 0 failed   (incl. 386 completions, 52 tokenize)
  mpl-language-server-wasm      5 passed; 0 failed
  mpl-playground               84 passed; 0 failed
  ───────────────────────────────────────────────
  TOTAL                       670 passed; 0 failed; 0 ignored

cargo clippy --workspace --all-targets                    → Finished, 0 warnings / 0 errors
cargo fmt --check                                         → clean

# locked interpolation contract (run by name)
test cst::tests::interpolation_with_commented_brace_finds_real_boundary ... ok
test cst::tests::interpolation_with_braced_escaped_ident_parses_cleanly ... ok
test cst::tests::interpolation_with_quoted_escaped_ident_parses_cleanly ... ok

# editor end-to-end
node tests/wasm/test-wasm.mjs                             → 38 tests: 38 passed, 0 failed
npm test -w @axiomhq/mpl-codemirror                       → 5 files, 62 passed

# removal proofs
rg -n 'logos' --type toml                                 → (none)
rg -cn 'use logos|logos::' src extra                      → (none)
grep 'name = "logos"' Cargo.lock                          → (none)
```

The 4 remaining textual `logos` references were stale **comments** describing
the old implementation; I rewrote them to describe the hand-written lexer
(comments document intent, and the intent changed).

---

## Scoring — honest, on the 8 axes

### (1) Single mental model / paradigm count — **STRONG (the headline axis)**

**Paradigm count: 1.** Everything in the front end is now hand-written
imperative Rust: `scan_token` (the token grammar), `expand_string`/`lex_interp`
(modal string/interpolation scanning), and the recursive-descent `parser.rs`.
There is no embedded DSL, no proc-macro codegen, no regex engine in the token
path, and no second grammar.

The baseline scored *worse* here than its dependency count suggested. It was a
**hybrid of two paradigms**:

1. a *declarative* token DSL — logos's `#[token]`/`#[regex]` attributes, with
   their own implicit longest-match + priority + `allow_greedy` resolution
   semantics, expanded by a build-time proc-macro you cannot read; and
2. *imperative* hand-written glue — `expand_string`/`lex_interp` (~120 lines)
   plus the `lex` loop that intercepted logos's raw `STRING`/`ERROR` tokens,
   re-lexed interpolation interiors, and *repositioned the logos cursor*
   (`lexer.bump(end - span.end)`) to undo logos's wrong span.

So the "single model" was actually *model + sub-model + impedance layer*.
Collapsing it to one imperative scanner is a genuine coherence win: a reader
opens `lexer.rs` and sees the entire token grammar top-to-bottom; the
interpolation interior is lexed by the *same* `scan_token` the top level uses
(no second tokenizer), and the recursion in `expand_string`/`lex_interp`
literally *is* the mode stack. There is exactly one place a token rule can live.

**Fusion, honestly.** The brief invited eliminating the intermediate `Vec` and
pulling tokens on-demand from the parser. I evaluated it and **kept the `Vec`**:
- the RD parser does k-token *non-trivia* lookahead (`nth(0)`, `nth(1)`,
  `nth_text`) and is *total* (error recovery wraps arbitrary token runs in
  `ERROR_NODE`); both are simplest over a materialized stream;
- rust-analyzer — the reference for this exact CST architecture — lexes to a
  token list first, then parses;
- interpolation **boundary** detection (brace-counting across nested strings /
  regex / comments) requires lexing the interior *ahead* anyway, so resolving it
  in the lexer (not on-demand in the parser) is cleaner.

What I *did* fuse: one `SyntaxKind` token model shared by lexer, parser, CST and
all consumers; one `scan_token` grammar reused for top-level **and**
interpolation-interior lexing; the lexer's three modes mirror the parser's
structure. Calling this "fused" would be overselling it — it's a **hand-written
lexer + hand-written RD parser sharing a token model**, co-located in `src/cst/`.

### (2) Constraints & deps explicit vs tribal — **STRONG**

The dependency is *gone* ("one fewer dep" — logos + logos-derive + logos-codegen
+ their build-only transitive crates). The token rules are now explicit Rust
that anyone can read and step through in a debugger. Crucially, the *ordering*
constraints that were **tribal** under logos (encoded implicitly in
longest-match-wins, attribute priority, and the `allow_greedy = true` flag on the
comment regex) are now **explicit control flow**:

- "RFC3339 beats FLOAT beats INT for a leading digit run" is a literal
  `if match_rfc3339() … else { float-or-int }` in `scan_number`, with a comment.
- "`#` always starts a regex (never division)" is the `b'#' => scan_regex`
  arm, adjacent to the `b'/'` slash/comment arms — the disambiguation is visible
  in one screen.
- "a float needs a digit after the `.` so `1747…092..` keeps its `..`" is an
  explicit `bytes.get(i+1).is_some_and(is_ascii_digit)` guard.

### (3) Native interpolation handling — **STRONG**

Handled by the lexer itself with explicit modes; the locked behavior tests pass
**unchanged**. `expand_string` scans literal-fragment text (recognising the
`${` opener and the `\$` escape that is *not* one); `lex_interp` lexes the
`${ … }` interior with `scan_token` and finds the closing `}` by **counting
brace tokens**. Because the interior is tokenised, a backtick ident
(`` `a}b` ``), a `#/regex/` or a `// comment` is a single token, so a `}`/`"`
inside one of them can never be miscounted as a delimiter. Nested strings recurse
(`expand_string → lex_interp → expand_string`). Unterminated strings /
interpolations are detected at EOF and recorded so the parser diagnoses them
while keeping the interior structured for mid-edit highlighting.

### (4) Declarativeness — **WEAK (honest: it's imperative)**

This is the real cost and the axis logos wins. Each token is a hand-written
`match` arm / scan loop, not a one-line regex. Adding `STAR` is one line; adding
a *shaped* token (a new number/quote/regex form) means writing a scanner helper
and slotting it into `scan_token`. The whole token grammar is ~181 code lines
that logos expressed in ~24 attribute lines. There is no grammar artifact to
diff; correctness lives in imperative code and is guarded by tests, not by a
declarative spec. If this grammar had **no** interpolation, logos's
declarativeness would make it the better choice and this variant would look like
needless re-implementation.

### (5) Spans + trivia fidelity — **STRONG / par**

Byte-exact spans (offsets computed directly from scan positions). Whitespace and
`//` comments are real trivia tokens; every unclassifiable byte becomes a
one-char `ERROR` token, so **no byte is ever dropped**. Lossless round-trip
(`syntax().text() == input`) holds for all inputs incl. comment-laden, nested,
escaped, and unterminated strings — proven by the unchanged
`lossless_roundtrip_preserves_every_byte` and
`interpolated_string_roundtrips_losslessly` tests.

### (6) Recovery quality — **STRONG / par**

The lexer is *total* (never panics, never returns `Err` — unrecognised input
becomes `ERROR`). The parser's recovery is unchanged. One deliberate match with
the old behavior: an **unterminated backtick** lexes as a *single*
`ERROR`-to-EOF token (not split into `` ` `` + ident fragments), which the editor's
`is_word_token`/source-extraction relies on mid-edit. Unterminated strings keep
full interior structure + a full-extent diagnostic.

### (7) Maturity — **n/a (hand-written); judged on readability/onboarding — GOOD**

No library to be mature. Onboarding: the entire tokenizer is one 430-line file,
read top-to-bottom, one `match` for the token grammar and two short modal
scanners for strings. A contributor needs **zero** knowledge of a third-party
lexer's attribute DSL or its longest-match/priority semantics. The tradeoff: a
contributor who *already* knows logos has more lines to read than the old ~24
attributes (but they also had to read the 120 lines of interpolation glue, so
net it's a wash-to-better).

### (8) Real LOC + build cost

| Item | value |
|------|------:|
| Hand-written lexer (`src/cst/lexer.rs`), total lines | 430 |
| …of which code lines (non-blank, non-comment) | 299 |
| …genuinely-new token grammar (`scan_token` + `scan_*` + `match_rfc3339`) | ~181 |
| …string/interp machinery moved out of `parser.rs` | ~80 |
| logos token attributes removed from `mod.rs` (`#[token]`/`#[regex]`) | ~24 |
| `parser.rs` shed (old `lex`/`expand_string`/`lex_interp`/`char_len` + comment) | ~182 |
| `lower.rs` `parse_param_value` simplified (logos loop → `tokenize_raw`) | ~−12 |
| Dependencies removed | `logos` (+ `logos-derive`, `logos-codegen`, build deps) |
| wasm bundle (`mpl_bg.wasm`) raw | **1,756,771** (was 1,767,088 with logos) |
| wasm bundle gzipped | **552,241** (was 554,316 with logos) |

**Build cost:** one fewer dependency and **no proc-macro codegen** for the
lexer (logos-codegen ran at build time); marginally faster clean builds and a
~10 KB smaller wasm bundle. Net source LOC is roughly flat-to-slightly-up: the
430-line `lexer.rs` largely *absorbs* the ~182 lines removed from `parser.rs`
(the interpolation glue that already existed) plus the ~24 logos attributes; the
true *additional* cost of going hand-written is the ~181 lines of explicit token
scanning.

---

## §3 — tricky cases, and which needed care

All of these are pinned by existing tests that pass unchanged.

| Case | How it's handled | Needed care? |
|------|------------------|:------------:|
| RFC3339 vs FLOAT vs INT (longest-match) | `scan_number` tries `match_rfc3339` first, then FLOAT (requires `.` + digit), else INT | clean first try |
| `..` not swallowed after integer timestamp (`1747…092..+1h`) | FLOAT requires a digit *after* the `.`; `092..` → INT + `DOT_DOT` | clean first try |
| exponent only on a fractional float (`1e5`→INT+IDENT, `1.5e5`→FLOAT) | `scan_exponent` only runs after a fractional part is consumed | clean first try |
| `#/regex/` vs `#s/…/…/` vs `/` division vs `//` comment | `b'#'`→`scan_regex` (regex-replace if `#s/`, else regex); `b'/'`→ `//` comment guard before bare `SLASH` | clean first try |
| escaped ident carrying `}` (`` `a}b` ``) inside `${ }` | `lex_interp` lexes interior by tokens + brace-counting; backtick ident is one token | clean (locked test) |
| escaped ident carrying `"` (`` `a"b` ``) inside `${ }` | same — the `"` is inside the single ESCAPED_IDENT token, never a delimiter | clean (locked test) |
| `// comment` carrying `}` before the real `}` | the comment is one COMMENT token (to EOL); its `}` is not a brace token | clean (locked test) |
| escaped `\${` is **not** interpolation | `expand_string` skips `\`+char before testing for `${` | clean first try |
| nested + unterminated interpolation | recursion + an `unterminated: Vec<usize>` recorded at EOF | clean first try |
| multi-char operator maximal munch (`::`,`..`,`==`,`!=`,`<=`,`>=`) | two-char arm guarded before the one-char arm | clean first try |
| lone `.`/`!`/`$`/`#`/unknown byte | one-char `ERROR` token (lossless) | clean first try |
| **unterminated backtick** (`` `my-tag `` mid-edit) | **must be ONE `ERROR`-to-EOF token**, because the editor's `is_word_token` + source/partial-word extraction key off "a single backtick-led `ERROR` token". My first cut split it into `` ` ``+IDENT+MINUS+IDENT, breaking 6 completion tests. Fixed by making `scan_escaped_ident` return `ERROR` to EOF when unterminated (the body class matches everything but a backtick, so there is no earlier boundary anyway). | **YES — the only thing that broke** |

The `$param` vs division and `#/regex/` vs division ambiguities are resolved at
the token level (a `#` always opens a regex; a `$` always opens a param), so the
parser never has to disambiguate.

`parse_param_value` (the external runtime-value entry point) now calls
`lexer::tokenize_raw`, a raw token loop that keeps a `"…"` literal as a single
`STRING` token (it inspects only the leading literal tokens of a value), instead
of the old `SyntaxKind::lexer(value)` logos call.

---

## Blunt verdict vs the logos-hybrid baseline

**Keep the hand-written lexer for this project.** The decisive fact is that the
baseline was never a clean "declarative lexer" — it was logos *plus* a
hand-written interpolation engine *plus* a cursor-repositioning impedance layer,
because string interpolation is not expressible in logos's per-token regexes. So
the honest comparison isn't "one elegant regex DSL vs 300 lines of hand code";
it's "(regex DSL + 120 lines of glue + a double-lex hack) vs 300 lines of one
coherent scanner." The hand-written version wins on **coherence** (paradigm
count 2→1, one file, one token grammar reused for interpolation), **explicitness**
(ordering constraints become visible control flow), and **deps/build** (one fewer
dependency, no codegen, smaller wasm), while *losing* on **declarativeness** —
adding a plain new token is a `match` arm instead of a one-line attribute.

If MPL had no string interpolation, I would not recommend this: logos's
declarativeness would dominate and the hand-written scanner would be
re-implementing a solved problem. With interpolation in the grammar, the
hand-written lexer is the cleaner single model. It is not a landslide — it is a
deliberate, defensible trade that this grammar happens to favour.
