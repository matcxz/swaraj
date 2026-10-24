package vim:
  version: "9.1"

  source:
    url: "https://github.com/vim/vim/archive/refs/tags/v9.1.0000.tar.gz"
    sha256: "HASH_A_DEFINIR"

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
