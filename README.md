# Swaraj

Swaraj is an independent, declarative language for describing package
definitions. This repository contains its initial Rust parser and syntax
checker. It does not download, build, or install software.

The language is being designed to be readable without sacrificing useful
structure: fields are indented, lists use square brackets, and strings use
double quotes. The syntax and parser API are an initial foundation, not a
claim of long-term compatibility.

## Example

```text
package vim:
  version: "9.1"

  source:
    url: "https://example.org/vim.tar.gz"
    sha256: "REPLACE_WITH_A_REAL_HASH"

  dependencies:
    build: [gcc, make, ncurses]
    runtime: [ncurses]

  build:
    commands: [
      "./configure --with-features=huge",
      "make"
    ]
```

The examples in `examples/` demonstrate the current syntax. Their source
hashes are placeholders; the files are syntax examples, not verified package
recipes.

## Initial syntax

- A file contains exactly one `package name:` declaration and its fields.
- Package names and field names start with an ASCII letter or `_`; subsequent
  characters may also include digits and `-`.
- Field names must be unique within their mapping; the same name may appear in
  different nested mappings.
- Each nesting level uses two spaces. Tabs are not allowed for indentation.
- Blank lines and `#` comments are ignored. A `#` inside a quoted string is
  ordinary text.
- Fields use `name: value`. A field with no inline value introduces a nested
  mapping.
- Values are either unquoted atoms, double-quoted strings, lists of scalar
  values, or nested mappings. Atoms and quoted strings are represented as
  strings in the syntax tree; the initial language does not infer numbers or
  booleans.
- Atoms contain ASCII letters, digits, `_`, `.`, `/`, `:`, `+`, and `-`.
  Quote values containing spaces or other punctuation.
- Strings support `\\`, `\"`, `\n`, `\r`, and `\t` escapes. A string stays
  on one source line.
- Lists contain scalar values, use commas between items, allow an optional
  trailing comma, and may span lines. Continuation items are indented two
  spaces below the field; the closing `]` aligns with the field.
- Mappings may be nested up to 128 levels.
- Nested lists, inline mappings, expressions, variables, and multiple
  package declarations per file are not part of this initial syntax.

For example, these are both valid:

```text
dependencies: [gcc, make, ncurses]
```

```text
commands: [
  "./configure --enable-feature",
  "make",
]
```

## Parser API and command line

The `swaraj` Rust crate exposes `parse(&str)`, returning a `Package` syntax
tree or a `Diagnostic`. Parsed values, fields, and list entries retain
one-based source locations. The current AST uses ordered maps so its
iteration order is deterministic.

```sh
nix run . -- check examples/vim.sw examples/emacs.sw
```

The checker reports syntax errors with a file, line, column, and correction
hint. It checks every supplied file and exits unsuccessfully if any file
cannot be read or parsed.

When the binary is already available on `PATH`, the equivalent command is:

```sh
swaraj check examples/vim.sw examples/emacs.sw
```

## Development

The flake pins the Nix development environment and provides Rust, Cargo,
rustfmt, Clippy, and rust-analyzer. The Rust parser currently has no
third-party crate dependencies. Flake outputs are available for x86_64 Linux,
aarch64 Linux, and Apple Silicon macOS; the Rust source itself uses only the
standard library.

```sh
nix develop
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- check examples/vim.sw examples/emacs.sw
```

## Current scope

This is the initial language front end: a documented syntax, a reusable Rust
parser API, a syntax-checking command, examples, and regression tests. Package
schema validation and all package-management behavior are deliberately outside
this repository's current scope.
