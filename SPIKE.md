# PHASE B — CST FEASIBILITY SPIKE (winnow → rowan)

**Question:** can `winnow` build a *lossless, position-addressable* `rowan` CST
as cleanly as RW's hand-written recursive-descent parser
(`depest-pi-rw/src/cst/parser.rs`)?

**Answer (verdict):** **Yes — feasible, and it round-trips and recovers — but it
is *comparable-to-slightly-more-awkward*, not cleaner.** Once you need a
side-effecting `GreenNodeBuilder`, winnow's headline feature (backtracking
combinators) becomes a liability you must actively avoid, so you end up
re-implementing recursive descent *inside* winnow. The combinators earn their
keep only at the leaves (span tracking + escape-aware token lexers). At the
structural level, winnow ≈ RW's hand-written parser, with one extra footgun.

Everything below is backed by the spike: **`tests/cst_spike.rs`** (self-contained
integration-test crate; `rowan = "0.16"` added as a **dev-dependency** so the
production build and its dependency tree are untouched — confirmed:
`cargo tree -e no-dev | grep -c rowan` = `0`, `cargo build` green).

---

## What was built (the representative slice)

A winnow→rowan CST for:

* metric source `ds:metric`
* one pipe clause `| where <ident> (== | != | < | …) <number | string>`
* a string literal with a `${ <ident> }` interpolation (the recursive construct
  that broke `logos` in RW), including nested/adjacent/escaped variants
* trivia: leading/trailing whitespace + `// line comments`

Lowering: **none** — the CST is the deliverable, exactly as asked.

Real tree (`dump()` output, captured from the spike) for
`ds:cpu | where tag == "Hi ${ name }!"` — note the interpolation is *not* an
opaque blob:

```
QUERY
  SOURCE
    METRIC_ID
      DATASET
        IDENT "ds"
      COLON ":"
      METRIC_NAME
        IDENT "cpu"
  WHITESPACE " "
  FILTER_RULE
    PIPE "|"
    WHITESPACE " "
    KEYWORD "where"
    FILTER_ATOM
      WHITESPACE " "
      IDENT "tag"
      WHITESPACE " "
      CMP_OP "=="
      WHITESPACE " "
      VALUE
        STRING
          STRING_FRAGMENT "\"Hi "
          DOLLAR_BRACE "${"
          EXPR
            WHITESPACE " "
            IDENT "name"
          WHITESPACE " "
          R_BRACE "}"
          STRING_FRAGMENT "!\""
```

---

## Properties proven (tests)

All run via `cargo test --test cst_spike` → **6 passed; 0 failed.**

| # | Property                                   | Test                                                   | Result |
|--:|--------------------------------------------|--------------------------------------------------------|:------:|
| 1 | byte-for-byte lossless round-trip          | `property1_lossless_roundtrip_preserves_every_byte`    | pass   |
| 2 | `${ ident }` interior is addressable       | `property2_interpolation_interior_is_addressable`      | pass   |
| 3a| recovery: incomplete `ds:metric \| where ` | `property3a_recovers_from_incomplete_clause`           | pass   |
| 3b| recovery: unterminated `"a ${ b`           | `property3b_recovers_from_unterminated_string`         | pass   |
| – | slice shape sanity                         | `slice_structure_is_shaped_correctly`                  | pass   |
| – | unknown pipe → `ERROR_NODE` (not dropped)  | `unknown_pipe_becomes_error_node`                      | pass   |

* **Property 1** asserts `parse(input).syntax().text() == input` over 16 inputs:
  trivia-heavy, all interpolation variants (leading, adjacent, escaped
  `\${`, nested), **and every recovery case** (incomplete clause, unterminated
  string, unknown pipe, `{{{}}}`, `|||`, `(`, `ds:`). Concatenating all token
  text reproduces the source byte-for-byte, trivia and interpolation interior
  included. No fake: the round-trip is real and the test fails loudly (it dumps
  the tree) if a single byte is dropped.
* **Property 2** walks the red tree: the `STRING` node has > 1 child, a
  `DOLLAR_BRACE` token, a real `EXPR` subtree holding `IDENT "name"`, and the
  boundary `STRING_FRAGMENT`s carry the quotes (`"Hi `, `!"`).
* **Property 3** — both recovery inputs still produce a tree, still round-trip,
  carry an error marker, and *keep the recognised prefix structured*
  (`FILTER_RULE` present; the unterminated string still exposes its `EXPR`/`IDENT`
  interior and is diagnosed `unterminated string` over its full extent to EOF).

---

## Ledger

### 1. How does winnow output map to rowan — does it fit, or need an intermediate?

**It fits with no intermediate AST — but only because the builder is threaded as
parser state, and only because the grammar is written to never backtrack after
emitting.** Two viable shapes exist; the spike took the first deliberately to
feel the friction:

* **(taken) Builder-as-state, direct emit.** The `GreenNodeBuilder` (+ an error
  sink) lives in `winnow::stream::Stateful` state behind a `RefCell`; combinators
  call `builder.token()` / `start_node()` / `finish_node()` as they consume. No
  intermediate tree. This is the most direct mapping and exactly what RW does,
  minus RW's separate lex pass.
* **(not taken) Event-list intermediate.** winnow returns a flat
  `Vec<Event::{Open,Token,Close}>` (rust-analyzer style), replayed into the
  builder afterward. This *removes* the backtracking footgun (events can be
  truncated on a failed branch) but *adds* a layer rowan already gives you via
  `checkpoint`/`start_node_at`. For this grammar it was unnecessary.

**The catch that forces the design:** `Stateful<I, S>` only implements
`winnow::Stream` when `S: Debug`, and — more importantly — **winnow's `alt`/`opt`
backtrack the input position but NOT side-effects on `S`.** If a branch emits
tokens into the builder and then fails, those tokens are stranded and the tree is
corrupt. rowan offers no "un-emit." So the builder-as-state mapping is only sound
if you **decide before you emit** (LL-style `peek`/lookahead), which the spike
does throughout (`at_kw`, `at_cmp`, `peek_is`). That is the real finding, not the
plumbing.

### 2. How hard is threading the green-tree builder through combinators?

* **Plumbing: easy.** `RefCell<GreenNodeBuilder>` + `RefCell<Vec<SyntaxError>>`
  behind shared refs in a `Copy` `State`, mirroring the production
  `wparser::grammar::Ctx` one-to-one. `emit/start/finish/wrap` are 1–3 line
  helpers. No `&mut`-threading pain across `alt`/`repeat` closures because the
  interior mutability sidesteps borrow conflicts.
* **Two concrete winnow papercuts** (both worked around, both documented in the
  file):
  1. **`S: Debug` bound** → had to hand-write `impl Debug for State` (the builder
     refs have nothing to print).
  2. **Lifetime-generic parser fns don't satisfy method-call `.parse_next()`.**
     `fn word<'s>(&mut Input<'s>) -> PResult<&'s str>` can't be used as
     `word.parse_next(input)` or `peek(word)` by method call — Rust won't pin
     `'s`. Fix: call them as plain functions (`word(input)`, `number(input)`) and
     hand-roll `peek` for them via `checkpoint`/`reset`. winnow's own
     non-borrowing parsers (`literal`, `multispace1`, `any`) are unaffected.

### 3. Recovery quality (be honest: winnow recovery is weaker than chumsky's)

**Honest take: winnow gives you *nothing* here; the recovery in the spike is 100%
hand-rolled, identical in spirit to what RW hand-writes.** winnow 1.x has no
built-in error-recovery combinator on stable. The spike recovers by:

* never failing the top level — `parse` is total;
* a `peek`-based pipe dispatch: an unrecognised `| …` is wrapped in `ERROR_NODE`
  and bytes are consumed up to the next `|`/EOF (`bump_any`), so nothing is
  dropped;
* incomplete clauses (`| where ` with no atom) record a diagnostic and stop,
  leaving the recognised prefix structured;
* unterminated strings close implicitly at EOF, keep their `${…}` interior, and
  are diagnosed over their full extent.

This matches RW's `ERROR_NODE` strategy and the production `wparser`'s
"resync-at-`|`" loop. It is **weaker than chumsky's** declarative recovery (no
`recover_with`, no automatic re-sync / delimiter pairing, no multi-candidate
recovery) — you write every resync by hand. It is **on par with RW's** hand-written
recovery, because RW also hand-writes every `ERROR_NODE`. So for a CST whose whole
point is "never fully fail," winnow neither helps nor hurts versus hand-rolling.

### 4. Trivia & span handling

* **Trivia:** clean. A `trivia()` combinator emits `WHITESPACE`/`COMMENT` tokens
  into the currently-open node. Emitting trivia eagerly is always sound (which
  node it lands in never affects losslessness), so it needs no lookahead — the
  one place winnow's "just consume" model is frictionless. Mirrors RW's
  `eat_trivia`.
* **Spans:** clean. `LocatingSlice` gives `current_token_start()` (absolute byte
  offset) for free; the unterminated-string diagnostic is `str_start..EOF`, and
  string fragments are emitted as source slices (`emit_slice(lo, hi)`) so boundary
  quotes glue onto their fragment exactly like RW. `.take()` returns the matched
  `&str` directly, so most tokens need no offset arithmetic at all — marginally
  nicer than RW indexing `&self.text[range]`.

### 5. Net code delta (this experiment only — production untouched)

| Artifact                                   | Lines |
|--------------------------------------------|------:|
| `tests/cst_spike.rs` total                 |   799 |
| ├─ kinds + lexer + parser + CST machinery  |   427 (code-only, ex-comments/blanks) |
| └─ tests (3 properties + 3 sanity)         |   167 (code-only) |
| `Cargo.toml`                               |    +3 (rowan dev-dep + comment) |

Reference target (RW, *whole* grammar, not the slice): `src/cst/parser.rs` 1127 +
`src/cst/mod.rs` 368. The spike's **~427 code lines cover one slice**; a
like-for-like RW slice (lexer + the `source`/`metric_id`/`filter`/`string`/`expr`
subset of `parser.rs`) is in the same order of magnitude. **No production line was
added or deleted** — this is purely additive experiment code.

### 6. Reuse

* Reused the **exact winnow idioms** already proven in this repo's production
  `wparser`: `Stateful<LocatingSlice<&str>, State>`, `current_token_start()`,
  `checkpoint()/reset()`, and the escape-aware string-run combinator
  (`wparser::lex::string_run_body` → spike `string_run`, byte-identical logic).
  This is why the plumbing was low-risk: it's the house style.
* Reused RW's **`SyntaxKind` shape** (screaming-case, `#[repr(u16)]`, trivia as
  tokens, parser-assigned relabels `KEYWORD`/`CMP_OP`) and its **transmute-based
  `Language` round-trip**, so the tree is directly comparable to RW's.
* No duplicate helpers introduced into production; the spike is standalone.

---

## Maintainability walkthrough: add a new pipe rule `| dedup by <tags>`

*(Same exercise the pest/winnow comparisons used, redone on the spike.)*

Under this winnow→rowan spike you would:

1. Add `DEDUP_RULE` (+ a `TAGS` node, `COMMA` token) to `SyntaxKind`. **One enum
   line each.**
2. In `pipe_clause`, add a branch next to the `where`/`filter` arm:
   ```text
   } else if at_kw(input, "dedup") {
       wrap(input, cp, DEDUP_RULE);
       bump_word(input, KEYWORD);     // dedup
       if at_kw(input, "by") { bump_word(input, KEYWORD); tags(input); }
       finish(input);
   }
   ```
3. Write `fn tags(input)` (a `bump_word` loop separated by `bump_lit(",", COMMA)`)
   — ~10 lines, structurally identical to RW's `tags()`.

**Is it simpler than under pest?** Yes, decisively — same as RW. There's no `.pest`
grammar to keep in sync with a tree-walk, no `Rule` enum churn, and the `KEYWORD`
relabel keeps highlighting correct for free.

**Is it simpler than RW's hand-written parser?** **No — it's a wash.** The branch
above is line-for-line what you'd write in RW's `pipe_rule`; `wrap`/`bump`/`finish`
map 1:1 to RW's `start_node_at`/`bump_as`/`finish_node`. The *only* difference a
maintainer feels is the standing rule "**decide with `peek`/`at_kw` before you
emit, or you corrupt the tree on a backtrack**" — a rule RW's design never has to
state because it never backtracks the builder in the first place. That single
invariant is the whole maintainability tax of choosing winnow here.

---

## Verdict (blunt)

**Comparable, leaning slightly more awkward than RW's hand-written parser** for
CST construction specifically.

* It **works**: lossless round-trip ✔, addressable `${…}` interior ✔, total
  error recovery ✔ — all proven, none faked.
* winnow's value-add (composable backtracking combinators, `cut_err`, context
  stacks) is **mostly wasted** when the output is a side-effecting green-tree
  builder, because you must suppress backtracking-after-emit. You keep `winnow`
  for **leaf lexing + spans** (genuinely nice: `.take()`, `LocatingSlice`,
  escape-aware runs) and **hand-write the structural layer + all recovery** —
  which is exactly what RW already does without the extra footgun.
* Net: choosing winnow over a hand-written recursive-descent rowan parser buys
  you tidy leaf lexers at the cost of one sharp, ever-present invariant
  (emit-only-after-commit) and two API papercuts (`S: Debug`, lifetime-generic
  fns can't be method-called). For a **lossless CST**, RW's hand-written approach
  is the cleaner baseline; winnow is a viable but not superior alternative.

### Top 3 ergonomics findings

1. **Backtracking vs. a mutable builder is the whole game.** `alt`/`opt` roll back
   the cursor, not your `GreenNodeBuilder` emissions. You must decide via
   `peek`/lookahead *before* emitting, which means re-implementing LL recursive
   descent inside winnow — eliminating most of winnow's structural advantage.
   (Sound, but it's a standing invariant a maintainer must never forget.)
2. **Two real API papercuts.** `Stateful<I,S>: Stream` needs `S: Debug` (hand-write
   it), and lifetime-generic parser fns (`fn p<'s>(&mut Input<'s>) -> &'s str`)
   can't be used via `.parse_next()`/`peek(p)` method calls — call them as plain
   functions and hand-roll `peek` with `checkpoint`/`reset`.
3. **Leaves are where winnow shines.** `LocatingSlice` spans, `.take()` returning
   the matched `&str`, and reusing the production escape-aware string-run
   combinator made the lossless string/interpolation lexer (the part that broke
   `logos` in RW) genuinely pleasant — and recursion through `${…}` was trivial.
