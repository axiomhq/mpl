# PHASE B — CST FEASIBILITY SPIKE (chumsky → rowan)

**Question:** can `chumsky` produce a *lossless, position-addressable* `rowan`
CST as cleanly as RW's hand-written recursive-descent parser
(`depest-pi-rw/src/cst/`)?

**Answer (verdict up front): COMPARABLE, and *cleaner* on the one axis that
actually motivated the RW rewrite — recursive string interpolation.** chumsky
needs a small intermediate tree to drive rowan's imperative builder (RW writes
the builder directly), and lossless trivia takes deliberate work in both. But
the `${ … }` interpolation that forced RW to hand-roll ~115 lines of byte
scanner is *one `recursive` combinator* in chumsky. Net: chumsky is a viable CST
front end, not just an AST one.

Everything below is backed by `tests/cst_spike.rs` (7 tests, all passing),
self-contained and gated as an integration test so it never enters the lib/wasm
build.

---

## What was built

`tests/cst_spike.rs` — a self-contained chumsky→rowan CST for a representative
slice:

| Slice requirement | Covered by |
| --- | --- |
| (a) metric source `ds:metric`            | `SOURCE → METRIC_ID → DATASET/COLON/METRIC_NAME` |
| (b) `\| where <ident> == <number\|string>` | `FILTER_RULE → FILTER_ATOM → VALUE_FILTER → EXPR` |
| (c) string with `${ <ident> }`           | `STRING` node with `STRING_FRAGMENT` / `DOLLAR_BRACE` / `EXPR` / `R_BRACE` |
| (d) trivia: whitespace + `// comment`    | `WHITESPACE` / `COMMENT` tokens retained in the tree |

`SyntaxKind` (24 kinds) and the `rowan::Language` impl are copied in shape from
RW's `mod.rs` so the target tree shape is identical. **Nothing is lowered** — the
CST is the deliverable.

Architecture:

```
chumsky parser over &str
    every combinator returns Vec<Green>      (Green = Token(kind,Range) | Node(kind,Vec<Green>))
    trivia rides along as real Green::Token elements
        │
        ▼  ~12-line recursive post-pass
GreenNodeBuilder  →  GreenNode  →  rowan::SyntaxNode
```

---

## Test results (paste)

```
running 7 tests
test happy_path_is_lossless_and_structured ... ok
test interpolation_interior_is_addressable ... ok
test nested_string_in_interpolation_round_trips ... ok
test recovery_incomplete_where_clause ... ok
test recovery_unterminated_string ... ok
test recovery_trailing_garbage ... ok
test trivia_only_round_trips ... ok

test result: ok. 7 passed; 0 failed
```

Proven properties:

1. **Byte-for-byte lossless round-trip** — every test reconstructs the input two
   independent ways: rowan's own `SyntaxNode::text()` *and* a manual
   token-text concatenation (`concat_tokens`). Trivia (leading comment, interior
   spaces, trailing space) and the interpolation interior are all reproduced.
2. **`${ ident }` interior is addressable** — `interpolation_interior_is_addressable`
   finds the `STRING` node, asserts it is a structured node (not an opaque
   blob), locates the `DOLLAR_BRACE`/`R_BRACE` tokens, and reads the
   interpolated `IDENT "b"` out of a real `EXPR` subtree. `nested_string_in_interpolation_round_trips`
   does the same one level deeper (`"x ${ "y" } z"`).
3. **Error recovery, still lossless** —
   - `ds:metric | where ` → tree still contains a `FILTER_RULE`, a diagnostic is
     recorded, round-trips exactly (trailing space included).
   - `"a ${ b` (unterminated string *and* unterminated interpolation) → tree
     keeps the `STRING` structure, `b` is still addressable, an
     `unterminated string` diagnostic is emitted, round-trips exactly.
   - `ds:metric @@@ ??? ☃` → trailing garbage lands in an `ERROR_NODE`,
     round-trips exactly.

Verification commands run:

```
cargo test --test cst_spike      # 7 passed; 0 failed
cargo fmt --check -- tests/cst_spike.rs   # clean
cargo clippy --test cst_spike    # 0 warnings
cargo build                      # lib unaffected
cargo tree -p mpl-lang --edges normal | grep rowan   # (empty: rowan NOT in lib)
cargo tree -p mpl-language-server-wasm | grep rowan  # (empty: rowan NOT in wasm)
```

`rowan = "0.16"` was added as a **dev-dependency** on purpose: that satisfies
"add rowan as a dependency" while honoring "gated so it doesn't affect the
build" — `cargo tree` confirms it is absent from the library and the
`wasm32-unknown-unknown` crate's dependency tree.

---

## How chumsky output maps to rowan — does it fit, or need an intermediate?

**It needs a (tiny) intermediate.** The two libraries pull in opposite
directions:

- `rowan::GreenNodeBuilder` is **imperative/streaming**: you call
  `start_node` → `token`* → `finish_node` in tree order, top-down.
- `chumsky` is **bottom-up/value-returning**: a combinator hands its caller a
  finished value; there is no "currently open node" cursor to write into.

You cannot cleanly thread a `&mut GreenNodeBuilder` *through* chumsky combinators
(it would have to live in parser state and you'd fight ownership and
backtracking — a half-applied builder write cannot be un-done when chumsky
backtracks). So the spike has every combinator return a value — a trivial
`enum Green { Token(kind, Range), Node(kind, Vec<Green>) }` — and a **12-line
recursive `emit`** walks it into the builder afterwards:

```rust
fn emit(b, g, src) {
  match g {
    Token(k, r)  => b.token(k.into(), &src[r]),
    Node(k, ch)  => { b.start_node(k.into()); for c in ch { emit(b, c, src) } b.finish_node() }
  }
}
```

This is the same shape as rust-analyzer's `Event` list (which also exists
precisely because its parser can't write the builder directly), just expressed
as a tree instead of a flat event vector. The intermediate is cheap and, because
it carries byte `Range`s rather than owned strings, the only place that touches
source text is `emit`.

**Verdict on the mapping:** fits well, with a mandatory but trivial adapter. RW
skips this adapter only because a *hand-written* RD parser already runs
top-down, so it can call the builder inline. That is the single structural
advantage hand-RD has here.

---

## How hard is threading the green-tree builder?

Not hard, because (per above) you *don't* thread it through the parser — you
thread it through one 12-line post-pass. The real work moved into how each
combinator shapes its `Vec<Green>`:

- A **leaf** is `trivia().then(inner).map(prepend)` — leading trivia, then the
  token. Relabeling (RW's `bump_as`) is just "pick the `SyntaxKind`" when
  building the `Green::Token`; spans come from `map_with(|_, e| e.span())`.
- A **node** is `children.map(|v| node(KIND, v))` — concatenate child
  `Vec<Green>` and wrap once.
- **`start_node_at(checkpoint)`** (RW's `wrap`, used for left-extending a pipe
  rule to capture the already-consumed `|`) has **no equivalent need** in
  chumsky: because parsing is bottom-up, you simply parse `|` first and include
  it as the node's first child. One fewer concept.

---

## Recovery quality

chumsky's recovery is **good enough to match RW for this slice, with a different
idiom**:

- **Soft/"expected X" recovery** — make the optional piece `.or_not()` and emit
  a diagnostic via `.validate(|v, e, emitter| …)`. This is exactly how the spike
  keeps `| where ` producing a `FILTER_RULE` with a recorded error instead of
  aborting — identical end result to RW's `expect()` + `error()`.
- **Totality / catch-all** — a trailing `any().repeated().to_slice()` mops up
  anything the structured grammar didn't consume into a single `ERROR_NODE`.
  This is the direct analogue of RW's "anything left over is unparseable: keep
  it in the tree" block, and is what makes the whole parse total.
- **Errors as a side-channel** — `parse(src).into_output_errors()` returns
  `(Some(tree), Vec<Rich>)` even when diagnostics fired, so output is always
  present. Diagnostics carry spans (`Rich::span()`), converted 1:1 to RW-style
  `SyntaxError { message, range }`.

Caveat: to carry a human-readable recovery message, the error context type must
be set to `String` (`Rich<'a, char, SimpleSpan, String>`); the default `C = ()`
only accepts `()` in `Rich::custom`. Minor, but a non-obvious gotcha.

What chumsky does *not* give for free that a hand-RD parser controls precisely:
**fine-grained re-sync points**. RW decides, per construct, exactly where to stop
skipping on error (e.g. `replace_body` skips to the next `|`). In chumsky you get
that via `recover_with(skip_then_retry_until(...))` / `via_parser(...)`, which
works but is fiddlier to tune than an inline `while !at(PIPE) { bump() }`. For
the slice the `.or_not()`+catch-all combination was sufficient and never
hard-failed.

---

## Trivia & span handling

- **Spans:** free and accurate. `map_with(|_, e| e.span())` gives a `SimpleSpan`
  on every match; `span.start()..span.end()` is the token's byte range. No
  manual offset bookkeeping anywhere. (Contrast RW's byte scanner, which threads
  a `base` offset by hand through `lex_range`/`expand_string`.)
- **Trivia:** this is the deliberate-work axis, and it's a wash with RW.
  chumsky's *natural* behaviour is to drop trivia (the production
  `src/slice.rs` uses `trivia().ignored()` and is therefore non-lossless). To go
  lossless you must (a) lex trivia as real tokens and (b) decide where it
  attaches. The spike attaches leading trivia to the *following* leaf, so it
  lands inside the nearest enclosing node — file-level leading/trailing trivia is
  hoisted to `ROOT`. RW makes the same choice imperatively via `eat_trivia()`
  before `start_node`. One honest difference: RW attaches a node's leading
  trivia to the *parent* (it eats trivia before `start_node`), whereas the
  spike's first leaf pulls it *inside* the node. Both are byte-lossless; the
  attachment differs cosmetically. Either convention is a few lines to change.

---

## Net effort vs RW (concrete)

| Axis | RW (logos + hand-RD) | chumsky → rowan spike |
| --- | --- | --- |
| Lexing | `logos` derive on `SyntaxKind` | chumsky char combinators (`any().filter`, `just`, `none_of`) |
| **Recursive string interpolation** | **~115 LOC hand byte-scanner** (`lex_range`, `expand_string`, `string_end`, `find_interp_close`, `char_len`, manual `base` offsets, nested-string + escape handling) because `logos` regexes can't recurse | **one `recursive` combinator** (`str_char` + a ~15-line `${…}` `interp` rule); nested strings & escapes fall out for free |
| Tree building | direct `GreenNodeBuilder` calls inline | `Vec<Green>` intermediate + 12-line `emit` post-pass |
| `start_node_at` checkpoints | needed (`wrap` for pipe rules) | not needed (bottom-up) |
| Spans | manual `base + span` arithmetic | automatic via `map_with`/`e.span()` |
| Trivia | manual `eat_trivia()` | `trivia()` retained as tokens (deliberate, ~6 LOC) |
| Recovery | `expect`/`error` + manual skip loops | `.or_not()` + `.validate` emitter + `any()` catch-all |

The standout line item is interpolation: it is the specific thing the task flags
("the recursive construct that broke logos in RW"), and it is where chumsky is
*unambiguously* cleaner — RW spends ~115 lines of error-prone manual byte
offsetting to do what chumsky expresses as a self-referential combinator. The
spike's `nested_string_in_interpolation_round_trips` test exercises exactly the
case (`"${ "y" }"`) that motivated RW's scanner.

---

## Blunt verdict vs RW's hand-written parser

- **Cleaner:** recursive constructs (string interpolation, nested strings),
  span handling, and not needing checkpoints/`start_node_at`.
- **Comparable:** trivia retention (deliberate work either way), soft-error
  recovery quality, overall LOC for the structured grammar.
- **More awkward:** (1) you must introduce an intermediate tree because rowan's
  builder is imperative and chumsky is bottom-up — a real, if small, impedance
  mismatch; (2) precise, per-construct error re-sync is more ergonomic in
  hand-RD; (3) the `Rich` custom-context `String` gotcha and the type-inference
  ceremony around `recursive::<_, Vec<Green>, E, _, _>` are papercuts.

**Bottom line:** chumsky can produce a lossless, position-addressable rowan CST.
It is not strictly *cleaner* than a competent hand-RD parser across the board —
the builder impedance mismatch is genuine — but it is *clearly cleaner exactly
where RW hurt most* (recursive interpolation), and *comparable* everywhere else.
A full port is feasible; budget the work for trivia attachment policy and
per-construct recovery tuning, not for the parser core.

---

## Top 3 ergonomics findings

1. **rowan's imperative builder forces a small intermediate.** chumsky returns
   values bottom-up; `GreenNodeBuilder` wants top-down streaming writes. Bridge
   with a `Green` tree + 12-line `emit`. Cheap, mandatory, and the only
   structural reason hand-RD looks "more direct".
2. **`recursive` makes the interpolation that broke logos trivial.** The single
   biggest pain in RW (~115 LOC of hand byte-scanning for `${…}` + nested
   strings + escapes) collapses to one self-referential combinator, with correct
   spans for free. This alone is a strong argument for chumsky as the CST front
   end.
3. **Trivia is opt-in, not free — but no worse than RW.** chumsky discards
   whitespace/comments by default; losslessness requires lexing trivia as tokens
   and choosing an attachment policy (the spike attaches leading trivia to the
   following leaf, hoisting file-level trivia to `ROOT`). Spans are automatic;
   trivia is the one place you pay attention. Honorable-mention papercut: set
   the `Rich` context type to `String` or `Rich::custom` won't take a message.
