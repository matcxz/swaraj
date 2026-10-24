package emacs:
  version: "30.2"

  source:
    url: "https://ftp.gnu.org/gnu/emacs/emacs-30.2.tar.xz"
    sha256: "b3f36f18a6dd2715713370166257de2fae01f9d38cfe878ced9b1e6ded5befd9"

  dependencies:
    build: [
      "gcc",
      "make",
      "pkg-config",
      "gnutls",
      "ncurses",
      "libx11",
      "gtk3",
      "tree-sitter",
      "libgccjit"
    ]
    runtime: [
      "gnutls",
      "ncurses",
      "libx11",
      "gtk3",
      "tree-sitter",
      "libgccjit"
    ]

  build:
    commands: [
      "./configure --with-x-toolkit=gtk3 --with-tree-sitter --with-native-compilation",
      "make"
    ]

  install:
    commands: ["make install"]
