# LEXER bake-off — variant **MORPH**

**Goal:** the best front end achievable with a *fully logos-native modal lexer*.
String/interpolation lexing made declarative in `logos` via `Lexer::morph` + a
second `#[derive(Logos)]` string-mode enum, with the brace-depth / mode stack in
`Extras`. **All hand-written byte-scanning of strings removed.**

## TL;DR verdict

Going "fully logos-native" with `morph` + `Extras` **does eliminate the
hand-written byte scanner** for string bodies — the character-level work
(escapes, `${`-vs-`$`, the closing `"`) is now 100% declarative `logos` regex,
and the nasty-interpolation correctness is *structurally* native because the
`${ … }` interior is lexed by the **same** `SyntaxKind` lexer (so a backtick
ident / regex / comment carrying `}` or `"` is one token and never a delimiter).

**But it does NOT collapse to one mental model.** `Extras` + `morph` *is* a
hidden state machine: two `logos` enums, a shared mutable `Extras` brace stack
poked by token callbacks, **and** a hand-written driver loop that owns the morph
transitions, fragment coalescing and unterminated tracking. That is ~3 paradigms,
not one. Versus the committed logos-hybrid baseline (which used the **Rust call
stack as the mode stack** via recursion) MORPH is a **lateral move**: it trades a
hand byte-loop (genuine win) for explicit `Mode`-enum + `Extras` + driver
ceremony (genuine cost). Net LOC is roughly flat; wasm +0.1%.

Result against the FIXED CONTRACT: **green, nothing faked.** Lossless rowan CST
preserved byte-for-byte, same `query.rs` AST, the three locked interpolation
tests **pass unchanged**, builds to `wasm32-unknown-unknown`.

---

## Design shape

```
                       ┌─────────────────────────── shared Extras: LexState ───────────────────────────┐
                       │   interp: Vec<u32>   (brace depth per open ${…}, innermost last)               │
                       │   closed_interp: bool (set by `}` callback when it closed an interpolation)     │
                       └───────────────▲───────────────────────────────────────────▲───────────────────┘
                                       │ `{`/`}` callbacks mutate                    │ carried across every morph
   normal mode                         │                                            │
   #[derive(Logos)] SyntaxKind ────────┘                                            │
        │   `"`  ──────────────── lx.morph() ───────────▶  string mode              │
        │   `}` (closed_interp) ◀─ lx.morph() ───────────  #[derive(Logos)] StrToken ┘
        │                                                       │  Text | Open(${) | Dollar($) | Quote(")
        └───────── interior of ${…} (expr) lexed here ◀─ lx.morph() on `${` ─────────┘
```

* **Two `logos` enums, one `Extras`.** `SyntaxKind` (unchanged role) lexes MPL
  code; the new private `StrToken` lexes string bodies. Both
  `#[logos(extras = LexState)]`, so `morph` (which requires
  `Extras: Into<Extras>`) carries the brace-depth stack across mode switches.
* **Mode switching is `Lexer::morph`,** not interception/re-lexing. `morph`
  preserves byte position, so on `"` we morph *forward* into the string body, on
  `${` we morph *back* to the code lexer for the embedded expression, and on the
  depth-0 `}` we morph back into the string. The opening `"` is matched as
  `SyntaxKind::STRING` (now `#[token("\"")]`, a one-char morph trigger) and
  folded into the leading fragment by the driver.
* **Brace depth lives in `Extras`** and is maintained *declaratively at the
  token level* by `brace_open`/`brace_close` callbacks on `{`/`}`. This is the
  one genuine use of `Extras`: it must survive `morph`, because the `}` that
  closes an interpolation is lexed in *code* mode, several morphs away from the
  `${` that opened it. The callback sets `closed_interp`; that single bool is
  the only signal the driver needs to know "morph back to string now".
* **The driver (`parser::lex`)** owns an `enum Mode { Normal(Lexer<SyntaxKind>),
  Str(Lexer<StrToken>) }`, reassigned each loop step (morph consumes the lexer
  by value). It coalesces literal text into `STRING_FRAGMENT` runs **by byte
  range** (not by re-scanning) and records unterminated-string starts. Its
  output is the **same flat `Vec<(SyntaxKind, Range)>`** the recursive-descent
  parser already consumed — so the parser, lowering, highlighter, completions
  and every white-box test are untouched.

**Token model: deliberately unchanged.** `STRING` (node) wraps
`STRING_FRAGMENT` (text runs, boundary fragments keep their `"`), `DOLLAR_BRACE`
(`${`), the embedded `EXPR` subtree, and `R_BRACE`. Keeping this identical is
why **no downstream consumer and no existing test had to change** — the redesign
is contained entirely in *how* the lexer produces the stream.

**What I did NOT keep:** the baseline's `expand_string` byte loop, its
`lex_interp` brace-token counter, and `char_len` (manual UTF-8 stepping) are
deleted. The `lex() -> Vec` *interface* is kept (it is idiomatic for a rowan
parser and keeps churn off the parser); the streaming-morph option would have
forced a full parser rewrite for no contract benefit and is rejected on the
brief's own "fewer moving parts / churn counts" grounds.

---

## Scorecard (honest)

### 1. Single mental model / paradigm count — **~3, did not collapse**

The brief's hypothesis is **falsified**. "Fully logos-native" still requires:
(a) two `#[derive(Logos)]` token grammars; (b) imperative `Extras` mutation in
token callbacks; (c) a hand-written morph-driver state machine (the `Mode` enum
+ the reassign-each-step loop + fragment/unterminated bookkeeping). The
declarative part (token *shapes*) is one model; the structural part (transitions
+ depth + boundaries + recovery) is still an imperative machine — it merely moved
from a byte cursor into a morph loop. The committed hybrid baseline expressed the
*same* structure with **recursion** (the call stack = mode stack), which is
arguably one fewer explicit moving part than `Mode`-enum + `Extras` + driver.

### 2. Constraints & deps explicit vs tribal — **mostly explicit, some tribal**

`morph`'s `Extras: Into<Extras>` requirement is satisfied by sharing one
`LexState` (identity `Into`) and is documented. The genuinely *tribal* knowledge
this design adds, now written down in comments but easy to break:
* `SyntaxKind::STRING` is a **one-char** `#[token("\"")]`, not a full-string
  regex — the morph won't work if it eats the body. (`parse_param_value` had to
  move onto the modal `lex` because of this.)
* the **opening `"` is folded** into the first `STRING_FRAGMENT` by the driver
  (not its own token) to preserve CST shape;
* `closed_interp` is only valid **immediately after** an `R_BRACE` (re-set by
  every `}`), and the driver must read it there.
None of these are checked by the type system; they are convention + comments.

### 3. Native interpolation handling — **strong / genuinely native**

The headline win. Because the `${ … }` interior is lexed by the real
`SyntaxKind` lexer (reached by `morph`, not re-lexing), a backtick ident
`` `a}b` `` / `` `a"b` ``, a `#/regex/`, or a `// comment }` is a single token —
its inner `}`/`"` is part of that token and can never be miscounted as a
delimiter. The three locked tests
(`interpolation_with_braced_escaped_ident_parses_cleanly`,
`…_quoted_escaped_ident_…`, `…_commented_brace_finds_real_boundary`) pass
**unchanged**. Nested and unterminated interpolation also handled
(`Mode` enum recurses via the morph loop; EOF in string mode records every open
string). The `${`-vs-lone-`$` distinction is fully declarative (`StrToken::Open`
vs `StrToken::Dollar`, longest-match).

### 4. Declarativeness — **partial (character-level: yes; structural: no)**

* **Declarative now:** escapes (`\"`, `\$`, `\\`), the closing `"`, `${` vs `$`,
  literal text runs — all `logos` regex/tokens. The baseline hand-scanned these
  byte-by-byte in `expand_string`. This is a real improvement.
* **Still imperative:** mode transitions, brace nesting, fragment boundaries,
  string nesting, unterminated recovery — the driver loop + `Extras` callbacks.

### 5. Spans + trivia fidelity — **identical to baseline (excellent)**

Byte-for-byte lossless round-trip preserved
(`interpolated_string_roundtrips_losslessly`, including escaped `\${`, nested,
empty, adjacent, directive interpolation). Same `STRING`/`STRING_FRAGMENT`/
`DOLLAR_BRACE`/`R_BRACE`/`EXPR` shape, same trivia attachment. The morph driver
emits exact byte ranges; no span drift.

### 6. Recovery quality — **identical to baseline (excellent)**

`lex` is total; the parser never fails. An *unterminated* string (mid-edit, no
closing `"`) still descends into the same `STRING_FRAGMENT`/`${`/embedded-`EXPR`
shape (`unterminated_interpolated_string_recovers_interior_structure` passes
unchanged), with the `unterminated string` diagnostic over the full extent —
sourced from the lexer reaching EOF in string mode, not a re-scan. Highlighting,
diagnostics and the completion string-classifier keep working on incomplete
input.

### 7. Maturity / maintenance — **good, with a sharp edge**

`morph` is a documented, stable `logos` 0.16 API; the second derive is trivial.
The maintenance hazard is the **morph-ownership dance**: `morph` consumes the
lexer, so the driver must own a `Mode` enum and reassign it each iteration —
non-obvious to a first reader, and `break`-from-`else`-let is needed for the EOF
arms. Adding a token to either grammar is a one-liner; changing the *string
structure* means touching the driver, the callbacks and the `Extras` invariants
together (they are coupled). The baseline's recursion expressed the coupling more
locally.

### 8. Lexer + interface LOC & build cost

| Region (string/interpolation lexer)        | baseline (hybrid) | MORPH | note |
|--------------------------------------------|------------------:|------:|------|
| `parser.rs` byte-scanner block (incl. doc) |               181 |     — | deleted: `lex` + `expand_string` + `lex_interp` + `char_len` |
| `parser.rs` modal lexer block (incl. doc)  |                 — |   148 | `StrToken` enum + `Mode` enum + `lex` driver |
| `mod.rs` `Extras` + `{`/`}` callbacks      |                 — |   ~31 | `LexState` + `brace_open` + `brace_close` (was inline in `lex_interp`) |
| **total lexer machinery**                  |          **~181** | **~179** | flat |

So **net ≈ flat** (~−2 lines). The change is *where the logic lives* — a hand
byte-loop (`expand_string`, 64 lines) + a token-counting loop (`lex_interp`, 40
lines) + manual UTF-8 stepping (`char_len`) become a 23-line `StrToken`/`Mode`
declaration + 24 lines of callbacks + a driver that reacts to 4 string tokens —
not a reduction in size. `parse_param_value` (lowering) moved onto the one
canonical `lex` (−1 ad-hoc lexer; the `String` arm now reads `STRING_FRAGMENT`).

**Build cost:** one extra `logos` derive (`StrToken`) → negligible extra
codegen. **wasm bundle:** `mpl_bg.wasm` 1,769,211 raw / 554,450 gzip vs the
pest-removed baseline 1,767,088 / 554,316 = **+2,123 raw / +134 gzip (+0.1%)** —
the second derive, essentially free.

---

## Verification (all run; outputs pasted)

```
$ cargo build --workspace                                  → Finished (ok)
$ cargo build -p mpl-language-server-wasm \
      --target wasm32-unknown-unknown                      → Finished (ok)
$ bash packages/build-mpl-wasm.sh                          → "MPL WASM package built successfully"
$ cargo clippy --workspace --all-targets                   → 0 warnings / 0 errors
$ cargo fmt --check                                        → clean (exit 0)
```

```
$ cargo test --workspace
     Running unittests src/lib.rs            (mpl-lang)              test result: ok. 88 passed; 0 failed
     Running tests/parse.rs                  (mpl-lang)              test result: ok.  3 passed; 0 failed
     Running unittests src/lib.rs            (mpl-language-server)   test result: ok. 492 passed; 0 failed
     Running unittests src/lib.rs            (mpl-language-server-wasm) test result: ok. 5 passed; 0 failed
     Running unittests src/lib.rs            (mpl-playground)        test result: ok. 84 passed; 0 failed
   TOTAL: 672 passed; 0 failed; 0 ignored
```

**Test count: 670 → 672.** No existing test was rewritten, weakened, skipped or
deleted — the unchanged corpus (incl. the three locked interpolation tests and
the unterminated-recovery test) is the equivalence proof that the morph lexer
produces a byte-identical CST. **+2 new white-box tests** added to exercise the
morph-specific machinery:
* `lone_dollar_in_string_is_literal_not_interpolation` — pins the declarative
  `${`-vs-`$` split (`StrToken::Open` vs `StrToken::Dollar`).
* `interpolation_brace_depth_tracked_in_extras` — pins the `Extras` brace-depth
  stack (`{}` inside `${…}` stays balanced; only the depth-0 `}` closes).

Stale comments referencing the deleted internals (`expand_string`,
`lex_interp`, `string_end`, `find_interp_close`, "Option B", "byte scanner")
were updated to describe the morph mechanism — assertions untouched.

### Proof the byte scanner is gone

```
$ grep -n 'as_bytes|\.chars()|char_len|bytes\[|lexer\.bump' src/cst/parser.rs   → NONE in the lexer
```

The only `[i]`/`text[range]` accesses left are the recursive-descent parser
indexing its **token buffer** (not bytes); the only `.bump()` calls are the
parser's token-consume method. String character scanning is 100% `logos`.

---

## "Add a new pipe rule `| dedup by <tags>`" — does MORPH change this?

**No — and that's the point.** The lexer redesign is orthogonal to grammar
rules. `dedup`/`by` lex as `IDENT` (no lexer change); add a `DEDUP_RULE`
`SyntaxKind`, one `at_kw("dedup")` arm in `parser::pipe_rule` (reusing `tags()`),
one `lower_aggregate` arm, `"dedup"` in `keyword_syntax_kind`, two tests.
Identical to the baseline. The morph machinery only touches **strings**, so it
neither helps nor hinders pipe-rule work — a fair outcome to report, not a win.

The relevant maintainability question MORPH *does* change is **"add a new string
escape / interpolation delimiter"**: that is now a one-line edit to the
`StrToken` regex/tokens (declarative) instead of a new branch in a hand byte
loop. That is the dimension where MORPH genuinely improves over the baseline.

---

## Blunt verdict vs the committed logos-hybrid baseline

**Keep the approach only if the explicit goal is "zero hand-written string
byte-scanning."** MORPH achieves that cleanly and keeps every contract (lossless
CST, same AST, locked interpolation behavior, wasm). On the dimension the brief
most wanted tested — *does fully logos-native collapse to one mental model?* —
the honest answer is **no**: `Extras` + `morph` is a hidden state machine
(2 grammars + callback-mutated shared state + a driver loop), and it carries
*more* explicit structural ceremony than the baseline's recursion-as-mode-stack.

So this is a **lateral move, not an upgrade**: clearly better at the
character-declarative level, modestly worse at the structural-simplicity level,
flat on LOC/spans/recovery/wasm. If I were optimizing purely for fewest moving
parts I would prefer the baseline's recursion; if I were optimizing for "all
lexing rules expressed in one declarative place," MORPH wins on the string half
but cannot pull the *structural* logic (nesting, boundaries, recovery) into
`logos` — `logos` has no native push/pop-mode, so that logic must live in the
driver regardless. `morph` + `Extras` is the most native expression available,
and it is still a state machine.
