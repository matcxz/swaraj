# Swaraj

Swaraj is a declarative language for package definitions, implemented in Rust.
It provides a parser, expression evaluator, and command-line checker.

```text
let:
  major: 9
  minor: 0
  increment: value: value + 1
  nextMinor: increment minor
  version: "${major}.${nextMinor}"
  metadata: {
    name = "Vim";
    version = version;
  }
in:
package vim:
  version: version
  metadata: metadata // { editor = true; }
  source:
    url: "https://github.com/vim/vim/archive/refs/tags/v9.1.0000.tar.gz"
    sha256: "8d2a74e358be9cf260174e15e2e29dba4773f2670718b5bf728db6f54b90e25d"
  dependencies:
    build: ["gcc", "make"]
    runtime: ["ncurses"]
```

## Language

Swaraj supports package declarations, indented fields, nested maps, lists,
`let:` bindings, curried functions, strings with interpolation,
integers, booleans, conditionals, and arithmetic and boolean operators.
Attribute sets support field selection (`set.name`), existence checks
(`set ? name`), fallback selection (`set.name or default`), and merging
(`left // right`).
Recursive attribute sets use `rec { name = value; }`; fields are evaluated on
demand, and circular references are reported.
A small set of pure built-ins provides `length`, `head`, `tail`, `attrNames`,
`attrValues`, `toString`, `stringLength`, `substring`, `replaceStrings`,
`baseNameOf`, `dirOf`, `concatStringsSep`, `concatLists`, `elem`, `hasAttr`,
`getAttr`, `intersectAttrs`, `removeAttrs`, `map`, `filter`, `foldl'`,
`genList`, `zipListsWith`, `mapAttrs`, `filterAttrs`, `listToAttrs`,
`isString`, `isInt`, `isBool`, `isList`, `isAttrs`, `isFunction`, `typeOf`,
`seq`, and `deepSeq`.

Bindings may reference later bindings and are evaluated on demand. Functions
use `parameter: expression` and application syntax `function argument`;
functions are first-class, capture lexical bindings, and support currying.
Local package imports use `builtins.import "relative/path.sw"` and expose the
imported package fields, along with `packageName`. `builtins.readFile` and
`builtins.pathExists` resolve paths relative to the source file. Diagnostics
include source locations and correction hints. The parser API preserves
locations in the resulting syntax tree; `parse_with_path` enables relative
file operations.

Package field requirements and accepted types are described in
[the package schema](docs/package-schema.md).

Swaraj has its own syntax and semantics and does not claim full Nix or Guix
language compatibility. Floating-point values and multiple package
declarations per file are not supported. Expression nesting and recursive
evaluation are bounded to prevent unbounded stack use. The checker validates
definitions but does not download, build, or install software.

## Check definitions

```sh
nix run . -- check \
  examples/vim.sw \
  examples/emacs.sw \
  examples/expressions.sw \
  examples/imported-base.sw \
  examples/imports.sw
```

Or, if `swaraj` is on `PATH`:

```sh
swaraj check \
  examples/vim.sw \
  examples/emacs.sw \
  examples/expressions.sw \
  examples/imported-base.sw \
  examples/imports.sw
```

## Development

The Nix flake provides Rust, Cargo, rustfmt, Clippy, and rust-analyzer.

```sh
nix develop
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The crate has no third-party dependencies. Flake outputs target x86_64 Linux,
aarch64 Linux, and Apple Silicon macOS.

## License

Swaraj is licensed under the GNU General Public License version 3. See
[LICENSE](LICENSE).
