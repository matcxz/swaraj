# Swaraj

Swaraj is free software licensed under the GNU General Public License
version 3. See [LICENSE](LICENSE) for details.

Swaraj is an independent, declarative language for describing package
definitions. This repository contains its initial Rust implementation: a
parser, expression evaluator, syntax-checking command, and examples. It does
not download, build, or install software. The language is experimental and
does not claim compatibility with Nix or Guix.

The design aims to keep package definitions readable while supporting reusable
values and predictable evaluation. Fields are indented, lists use square
brackets, and strings use double quotes.

## Example

```text
let:
  major: 9
  minor: 1
  version: "${major}.${minor}"
  optionalFeatures: ["clipboard", "terminal"]
  features: optionalFeatures ++ ["multibyte"]
  useNativeBuild: true
in:
package vim:
  version: version
  features: features
  source:
    url: "https://example.org/vim-${version}.tar.gz"
    sha256: "REPLACE_WITH_A_REAL_HASH"
  dependencies:
    build: ["gcc", "make"]
    runtime: ["ncurses"]
  build:
    commands: [
      "./configure --with-features=huge",
      "make"
    ]
```

The examples in `examples/` demonstrate the current syntax. Their source
hashes are placeholders; they are syntax examples, not verified package
recipes.

## Syntax and evaluation

- A file contains exactly one `package name:` declaration, optionally
  preceded by a `let:` block and terminated by `in:`.
- Package, field, and binding names start with an ASCII letter or `_`;
  subsequent characters may also include digits and `-`.
- Bindings may refer to later bindings. They are evaluated on demand, so
  unused bindings are not evaluated; recursive references are diagnosed.
- Each nesting level uses two spaces. Tabs are not allowed for indentation.
- Blank lines and `#` comments are ignored. A `#` inside a quoted string is
  ordinary text.
- Fields use `name: expression`. A field with no inline value introduces a
  nested mapping. Field names must be unique within their mapping.
- Values include strings, signed 64-bit integers, booleans (`true` and
  `false`), lists, and nested mappings.
- Lists use commas, allow a trailing comma, may span lines, and can contain
  other lists. Continuation items are indented two spaces below the field;
  the closing `]` aligns with the field.
- Strings support `\\`, `\"`, `\n`, `\r`, and `\t` escapes. `${name}`
  interpolates a string, integer, or boolean binding. Strings stay on one
  source line.
- Parentheses group expressions. Operators, from lower to higher precedence:
  `||`, `&&`, `==` and `!=`, `<` `<=` `>` `>=`, list concatenation `++`,
  `+` and `-`, `*` `/` and `%`, then unary `!` and `-`.
- `if condition then value else fallback` is an expression. Boolean
  operators and conditionals short-circuit.
- Arithmetic is checked for 64-bit overflow; division or remainder by zero,
  undefined variables, type mismatches, and recursive bindings produce
  diagnostics.
- Mappings may be nested up to 128 levels.

Functions, imports, attribute-set expressions, floating-point numbers,
built-ins, and multiple package declarations per file are not implemented.
These are possible future language features, not current guarantees.

## Parser API and command line

The `swaraj` Rust crate exposes `parse(&str)`, returning a `Package` syntax
tree or a `Diagnostic`. Parsed values, fields, list entries, and package names
retain one-based source locations. Maps use ordered keys for deterministic
iteration.

```sh
nix run . -- check examples/vim.sw examples/emacs.sw examples/expressions.sw
```

The checker reports syntax and evaluation errors with a file, line, column,
and correction hint. It checks every supplied file and exits unsuccessfully
if any file cannot be read or parsed.

When the binary is already available on `PATH`, use:

```sh
swaraj check examples/vim.sw examples/emacs.sw examples/expressions.sw
```

## Development

The flake provides Rust, Cargo, rustfmt, Clippy, and rust-analyzer. The Rust
crate currently has no third-party dependencies.

```sh
nix develop
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- check examples/vim.sw examples/emacs.sw examples/expressions.sw
```

Flake outputs are available for x86_64 Linux, aarch64 Linux, and Apple Silicon
macOS. The source uses only the Rust standard library.
