# MPL [![CI](https://github.com/axiomhq/mpl/actions/workflows/ci.yaml/badge.svg)](https://github.com/axiomhq/mpl/actions/workflows/ci.yaml) [![codecov](https://codecov.io/github/axiomhq/mpl/graph/badge.svg?token=WCHISH068G)](https://codecov.io/github/axiomhq/mpl) [![docs.rs](https://img.shields.io/docsrs/mpl-lang)](https://docs.rs/mpl-lang) [![Crates.io Version](https://img.shields.io/crates/v/mpl-lang)](https://crates.io/crates/mpl-lang) [![NPM Version](https://img.shields.io/npm/v/%40axiomhq%2Fmpl)](https://www.npmjs.com/package/@axiomhq/mpl)

MPL is the language [Axiom](https://axiom.co) designed to query metrics. It takes a piped aproach that is inspired by [APL](https://axiom.co/docs/apl/introduction) and [OxQL](https://rfd.shared.oxide.computer/rfd/0463).

The primary design goal is to achive a intuitive and readable language that is optimized for both human and machine consumption.

This repository contains:
- The parser
- The AST generation
- The linter
- The WASM libary
- A simple CLI 'compiler'
- A binary to generate the library documentation
- Typescript bindings
- The typescript language server 
- The Codemirror integration

We provide a playground to try out those parts.

## Getting Started

```bash
just playground
```

## Shift

```mpl
metrics:cpu | shift -1h | align to 60s using last
```

Shift reads data at a signed offset while preserving output timestamps: `shift -1h`
reads one hour earlier and displays it at the current output times. Positive offsets
read later data, and zero leaves the source times unchanged. Offsets are literal
durations that must resolve exactly to signed 64-bit whole seconds; overflow and
fractional seconds are errors. The metrics service executes the shifted source read.

## License

[MIT](LICENSE-MIT) or [Apache](LICENSE-APACHE)
