use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use swaraj::parse_with_path;

const USAGE: &str = "\
Swaraj - validate declarative package definitions

Usage:
  swaraj check <FILE.sw>...
  swaraj --help
  swaraj --version
";

fn run(args: &[OsString]) -> Result<(), (u8, String)> {
    match args {
        [flag] if flag == OsStr::new("--help") || flag == OsStr::new("-h") => {
            print!("{USAGE}");
            Ok(())
        }
        [flag] if flag == OsStr::new("--version") || flag == OsStr::new("-V") => {
            println!("swaraj {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        [command, paths @ ..] if command == OsStr::new("check") && !paths.is_empty() => {
            let mut failed = false;

            for path in paths {
                let path = Path::new(path);
                match fs::read_to_string(path) {
                    Ok(source) => match parse_with_path(&source, path) {
                        Ok(package) => {
                            println!("{}: valid package `{}`", path.display(), package.name)
                        }
                        Err(diagnostic) => {
                            eprintln!("{}", diagnostic.render(&path.display().to_string()));
                            failed = true;
                        }
                    },
                    Err(error) => {
                        eprintln!("{}: error: could not read file: {error}", path.display());
                        failed = true;
                    }
                }
            }

            if failed {
                Err((1, String::new()))
            } else {
                Ok(())
            }
        }
        _ => Err((2, USAGE.to_owned())),
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            if !message.is_empty() {
                eprint!("{message}");
            }
            ExitCode::from(code)
        }
    }
}
