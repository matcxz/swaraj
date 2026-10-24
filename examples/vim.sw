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
