# Must

[![CI](https://github.com/must-sh/must/actions/workflows/rust.yml/badge.svg)](https://github.com/must-sh/must/actions)

A low-level, C-like systems programming language compiler built for speed, transparency, and explicit control.

> ⚠️ **Status: Experimental / Early Alpha (v0.1.0)**
> `mustc` is in active early development. Syntax rules, internal IR, and compiler flags are subject to breaking changes between releases. You may encounter missing error diagnostics, codegen edge cases, or compiler panics. Bug reports and contributions are welcome via [GitHub Issues](https://github.com/must-sh/must/issues)!

---

## Quick Start

### Installation

Install the latest compiler release directly via Cargo:

```bash
cargo install mustc
```

### Usage

Compile a `must` source file into a native binary:

```bash
mustc main.must
cc main.o -o main # for linking with system libraries and startup code
./main
```

---

## License

Dual-licensed under either of:

* **Apache License, Version 2.0** ([LICENSE-APACHE](http://www.apache.org/licenses/LICENSE-2.0))
* **MIT License** ([LICENSE-MIT](http://opensource.org/licenses/MIT))

at your option.
