# Package schema

Every package must provide a non-empty string `version`. The package name is
provided by the `package name:` declaration. The checker validates the core
fields below while allowing additional top-level fields for package-manager or
repository-specific metadata.

```sw
package vim:
  version: "9.1"
  source:
    url: "https://github.com/vim/vim/archive/refs/tags/v9.1.0000.tar.gz"
    sha256: "8d2a74e358be9cf260174e15e2e29dba4773f2670718b5bf728db6f54b90e25d"
  dependencies:
    build: ["gcc", "make", "ncurses"]
    runtime: ["ncurses"]
  build:
    commands: [
      "./configure --with-features=huge --enable-multibyte --with-tlib=ncurses",
      "make"
    ]
  install:
    commands: ["make install"]
```

## Core fields

- `version` is required and must be a non-empty string. Swaraj does not impose
  a versioning scheme.
- `description` is optional and, when present, must be a non-empty string.
- `source` is optional. When present, it must be a mapping with a non-empty
  absolute URI `url` and a string `sha256` containing exactly 64 hexadecimal
  characters. Swaraj checks that the URI has a valid scheme syntax, but does
  not restrict which schemes a consumer may support. It validates the
  checksum's shape, not whether it matches downloaded bytes. A consumer must
  verify the content before using it.
- `dependencies` is optional. It may be a list of non-empty strings or a
  mapping containing `build` and/or `runtime`, each a list of non-empty
  strings.
- `build` and `install` are optional mappings. When present, each must have a
  non-empty `commands` list of non-empty strings. Swaraj treats commands as
  data; it does not execute or interpret them.

Additional top-level fields are preserved in the parsed package value.
Consumers may define and validate their own extensions. Swaraj's core checker
does not guarantee that a package can be downloaded, built, or installed.
