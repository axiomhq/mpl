# Lexer bake-off — variant **LEXGEN**

Goal: build the best MPL front end achievable with the **lexgen** crate's native
start-states, replacing `logos`. The fixed contract (rowan lossless CST →
highlighting/diagnostics/completions; lower to the same `query.rs` AST;
nasty-interpolation behaviour; stable Rust + `wasm32-unknown-unknown`) is held.

## TL;DR verdict

**lexgen wins this grammar over the logos-hybrid baseline.** The hard part of
MPL lexing is **nested string interpolation** (`"a ${ "b ${ c }" }"`, with `}` /
`"` legally buried inside backtick idents, regexes and comments). logos cannot
express that natively, so the baseline wrapped logos in a hand-rolled
**recursive byte-scanner** (`expand_string` + `lex_interp` + `char_len`,
~90 lines that re-invoked `logos::lexer` on every interpolation interior and
counted braces by hand). lexgen expresses the whole thing **in-engine** with two
rule sets and a user-state stack — that recursion is **deleted**. Costs are
minor and bounded: +1.7 % raw / +0.6 % gzip wasm, a few regex-syntax quirks, and
**one** explicit rule added to paper over a real lexgen-vs-logos behavioural
difference (below). All gates pass with **zero tests rewritten or weakened**.

## What shipped

- `logos` removed from every `Cargo.toml` and from `Cargo.lock`
  (`rg -n logos --type toml` → nothing; `grep -c logos Cargo.lock` → `0`).
  Residual `logos` strings are 4 explanatory code comments + `SKIPPED.md`
  (a historical planning doc).
- `lexgen = "0.16"` + `lexgen_util = "0.16"` added.
- `SyntaxKind` is now a **plain enum** (`#[repr(u16)]`) — its only job is the
  rowan node-kind role. The lexer rules moved out of attributes into the
  `lexer! { … }` macro.
- The lexer↔parser interface (`Vec<(SyntaxKind, Range)>` + `unterminated:
  Vec<usize>`) was **kept**. The recursive-descent parser, the lowering pass and
  the CST shape are untouched, so the whole downstream (AST, visitors, language
  server, editor) is unaffected and every existing test holds. The redesign that
  *mattered* — killing the recursive interpolation scanner — is done.

## Design shape (how lexgen's native states are used)

Two **rule sets** (each compiled to its own DFA by lexgen):

| rule set   | role |
|------------|------|
| `Init`     | normal MPL lexing — **and** the interpolation interior, because a `${ … }` body *is* normal MPL. Lexing the interior with the same DFA is what makes `` `a}b` ``, `#/re/` and `// …}` single tokens, so their `}`/`"` can never be miscounted as delimiters. Catch-all `_ => ERROR` keeps it total/lossless. |
| `InString` | inside a `"…"` literal: maximal `STRING_FRAGMENT` runs until the closing `"` or a `${` opener; catch-all `_ => STRING_FRAGMENT` for stray bytes. |

Explicit state transitions (lexgen's core strength), via `switch_and_return`:

```
Init  --  "   -->  InString      (open quote; push open_strings)
InString -- "  -->  Init          (close quote; pop open_strings)
InString -- ${ -->  Init          (enter interp body; push interp depth-frame)
Init  --  }   -->  InString       (close interp at brace depth 0; pop frame)
```

Nesting falls out of a **user-state stack** — exactly the "state stack / depth"
the brief asked for:

```rust
struct LexState {
    interp: Vec<u32>,        // brace depth per open `${ … }` (the nesting stack)
    open_strings: Vec<usize> // opening-quote offsets; what's left = unterminated
}
```

`"${ "inner ${ x }" }"` therefore needs no recursion: `${` pushes a frame, a
nested `"` switches to `InString`, its `}`/`"` pop the right frame/state. The
Rust call stack is no longer the interpolation stack — `LexState.interp` is.
`open_strings` is read back after the run to reproduce the exact
`unterminated string` diagnostics (over `start..EOF`) the parser expects.

50 declarative rules; the only imperative glue is the 3 semantic-action closures
for `"`, `{`, `}` that push/pop the stack.

## lexgen MATURITY — concrete signals

| signal | finding |
|--------|---------|
| Resolved version | `lexgen 0.16.0` + `lexgen_util 0.16.0` (latest) |
| Release history | **19 releases** `0.1.0 → 0.16.0`, **0 yanked** (read from the local crates.io cache index) — long-lived, stable line |
| Last release | `0.16.0` is the head; exact date not available offline, but cadence is slow/steady, not abandoned-looking |
| MSRV | **not declared** (`rust_version = None` on every published version); crate `edition = "2021"`, so effectively rustc ≥ 1.56. Built fine under our `rustc 1.95.0` / edition-2024 workspace |
| Author | Ömer Sinan Ağacan (`osa1`) — known Rust/compiler/GC engineer |
| Docs/README | **409-line README**, thorough: full regex grammar, rule syntax, right-context lookahead, built-in unicode classes, stateful-lexer worked example, all 4 constructors, EOF semantics. Good. |
| Real-world test corpus | a full **Lua 5.1 lexer** (15.7 KB), a **Rust lexer** (linked), **right-context** tests (5.4 KB), a **bugs** regression file (10.9 KB), 32 KB main tests — strong evidence it's been pushed hard |
| Runtime deps | **only `unicode-width`** (pure Rust, no_std-friendly) → trivially wasm-safe. Proc-macro deps (`syn`/`quote`/`proc-macro2`/`rustc-hash`) are host-only build deps |
| wasm experience | `cargo build --target wasm32-unknown-unknown` and `wasm-pack` both clean, first try. No nightly, no patches |
| Compile experience | macro accepted the grammar on the **first** compile; lib builds in ~12 s (comparable to `logos-derive`) |

### Rough edges hit (honest)

1. **No `{n}` repetition.** lexgen regex has `* + ?` only, so the RFC3339
   literal `[0-9]{4}-…` is spelled out digit-by-digit. Verbose but mechanical.
2. **Negation is `_ # [set]`, not `[^…]`.** Char-class complement is the
   difference operator (`(_ # ['"' '\\' '$'])`). Documented, just unfamiliar.
3. **lexgen backtracks to the last *accepting* state on a failed match; logos
   emits the failed *greedy prefix* as one error token.** This is the one real
   gotcha. An unterminated backtick `` `my-tag `` under logos was a *single*
   `ERROR` token starting with a backtick — a contract the completion engine
   pins (`is_word_token`). Under lexgen the failed `ESCAPED_IDENT` backtracks to
   the lone `` ` `` (its last accept via the `_` catch-all) and re-lexes `my-tag`
   as separate idents → 6 completion tests failed. Fix: one explicit rule
   `'`' $tick_body* = ERROR` so the unterminated run is one token again (the
   closed `ESCAPED_IDENT` rule is a longer match and still wins). Principled, but
   you *must* know the backtrack semantics.
4. **Non-`Init` rule sets fail (return `Err`) on EOF, not `None`.** So the
   `lex()` wrapper treats `Some(Err)` and `None` identically and reads
   `open_strings` for unterminated info, rather than relying on the iterator.
5. **Generated `state()` is module-private.** Reading user state after the run
   works only because `lexer! { … }` is invoked in the same module as `lex()`.
   Fine here, but it's not a public API.

## Eight-axis scoring (vs the logos-hybrid baseline)

| Axis | lexgen | logos-hybrid | Note |
|------|:------:|:------------:|------|
| 1. Single mental model | **A** | C | lexgen = one artifact (DFA rule-sets + explicit states). Baseline = logos DFA **plus** a hand-rolled recursive byte-scanner because logos can't nest. |
| 2. Explicit constraints | **A−** | B | Rules ordered, longest-match + rule-order tie-break, explicit `switch`, explicit `_` catch-all. Backtrack-vs-greedy nuance must be understood (edge #3). |
| 3. Native interpolation | **A** | D | lexgen's headline: nested `${ }` is native (state switch + `LexState.interp`/`open_strings` stack). Baseline re-invoked `logos::lexer` recursively per interior and counted braces by hand. |
| 4. Declarativeness | **A−** | B | ~85 % declarative one-liners; string transitions need 3 imperative closures + RFC3339 verbosity. Baseline tokens were declarative but the entire string/interp machinery was imperative Rust. |
| 5. Spans + trivia | **A** | A | `Loc{line,col,byte_idx}` for free (only `byte_idx` used for rowan). WS/COMMENT emitted as real tokens → lossless. Parity. |
| 6. Recovery | **A−** | A− | `_ => ERROR` makes `Init` total; unterminated strings tracked via `open_strings`. Lost logos's "free" greedy error span (edge #3), regained with one rule. |
| 7. Maturity | **B+** | A− | lexgen: 19 releases, 0 yanks, great docs, real lexers, wasm-clean — but smaller adoption, no MSRV, slower cadence than logos. logos is more battle-tested by sheer usage, but deliberately *can't* do stateful nesting. |
| 8. LOC + build/wasm cost | **A−** | B | Deletes the ~90-line recursive scanner + ~32 attribute lines; net parser.rs ≈ flat (declarative rules replace fiddly scanning). wasm +1.7 % raw / +0.6 % gzip. Build time comparable. |

## Results (pasted)

```
$ cargo build --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo fmt --check
fmt: CLEAN

$ cargo clippy --workspace --all-targets
    Finished `dev` profile        # 0 warnings, 0 errors

$ cargo test --workspace
  mpl-lang (lib)              86 passed; 0 failed
  mpl-lang  tests/parse.rs     3 passed; 0 failed
  mpl-language-server (lib)  492 passed; 0 failed   # incl. all tokenize/completion/lints/hover
  mpl-language-server-wasm     5 passed; 0 failed
  mpl-playground              84 passed; 0 failed
  ----------------------------------------------
  TOTAL                      670 passed; 0 failed   (== baseline 670)

$ cargo build -p mpl-language-server-wasm --target wasm32-unknown-unknown
    Finished `dev` profile        # lexgen + lexgen_util compile to wasm32 ✓

$ bash packages/build-mpl-wasm.sh
    Finished `wasm-release` profile [optimized]
    [INFO]: ✨  Done            # wasm-pack package built ✓

$ rg -n 'logos' --type toml      # (no matches)
$ grep -c logos Cargo.lock       # 0
```

**Tests rewritten:** none. **Tests weakened/deleted/ignored:** none. The three
locked interpolation tests
(`interpolation_with_braced_escaped_ident_parses_cleanly`,
`…_quoted_escaped_ident_…`, `…_commented_brace_finds_real_boundary`) and
`unterminated_interpolated_string_recovers_interior_structure` pass **unchanged**
— the CST shape is identical, so only behaviour-irrelevant *comments* above them
were updated to describe the lexgen mechanism instead of logos.

**Editor features:** all 492 language-server tests (tokenize/highlighting,
completions incl. the ~2 000-line corpus, diagnostics, lints, hover) pass, and
the wasm package builds, so highlighting/completions/diagnostics still work.

### wasm bundle (`mpl_bg.wasm`, `wasm-pack … --profile wasm-release --no-opt`)

| build | raw bytes | gzipped |
|-------|----------:|--------:|
| logos-hybrid baseline (pest already removed) | 1,767,088 | 554,316 |
| **lexgen** | **1,796,751** | **557,594** |
| Δ | **+29,663 (+1.7 %)** | **+3,278 (+0.6 %)** |

The delta is `lexgen_util` + `unicode-width` vs logos's runtime — small.

## Net code delta

| File | change | what |
|------|--------|------|
| `src/cst/parser.rs` | **−~90 / +~120** (net ≈ +24 lines incl. doc) | Deleted the recursive byte-scanner (`expand_string` + `lex_interp` + `char_len`); added the `lexer! { Init / InString }` macro (50 declarative rules), `LexState`, and a 30-line coalescing wrapper. |
| `src/cst/mod.rs` | **−~32** | Dropped `#[derive(Logos)]`, `use logos`, and all `#[token]`/`#[regex]` attributes; `SyntaxKind` variants now bare. |
| `src/cst/lower.rs` | ~0 | `parse_param_value` lexes via the CST `lex()` instead of `logos`; `STRING` arm → `STRING_FRAGMENT`. |
| `Cargo.toml` | −1 / +2 | `logos` out; `lexgen` + `lexgen_util` in. |

**Net:** roughly flat LOC, but the *character* improved — a hand-rolled
recursive scanner (the part most likely to harbour boundary bugs, and which the
locked tests exist to guard) became declarative DFA rules plus a small explicit
state stack.

## Maintainability — re-do "add `| dedup by <tags>`" at the lexer layer

For the lexer specifically: **zero** new lexer rules. `dedup`/`by` already lex
as `IDENT`; tags reuse existing tokens. (Parser + lowering add a `DEDUP_RULE`
arm, same as today.) Adding a brand-new *token shape* (say a `@`-prefixed
macro-ident) is a **one-line declarative rule** in `Init` — e.g.
`'@' $id_start $id_cont* = SyntaxKind::AT_IDENT,` — versus, under the logos
hybrid, a logos attribute **plus** auditing the hand-rolled `expand_string`/
`lex_interp` scanner if the new shape can appear inside an interpolation. Fewer
places to keep in lockstep.

## Blunt verdict

For a lexer whose defining difficulty is **stateful, nesting string
interpolation with hostile contents**, lexgen is the better-fit tool than the
logos hybrid: it makes the hard case native and deletes the recursive
byte-scanner that the locked tests were written to police, at the price of a
slightly larger wasm bundle, a couple of regex-syntax quirks, and one
backtrack-semantics gotcha you must learn once. If the grammar were *flat*
(no nesting), logos's bigger ecosystem and "free" greedy error spans would make
it the safer default — but this grammar is not flat. **Adopt lexgen.**
