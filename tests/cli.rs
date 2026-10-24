use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

fn write_temp_source(source: &str) -> PathBuf {
    let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("swaraj-cli-{}-{id}.sw", std::process::id()));
    fs::write(&path, source).expect("temporary source file should be written");
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swaraj"))
        .args(args)
        .output()
        .expect("Swaraj command should start")
}

#[test]
fn prints_help_and_version() {
    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("swaraj check <FILE.sw>..."));

    let version = run(&["--version"]);
    assert!(version.status.success());
    assert!(
        String::from_utf8_lossy(&version.stdout)
            .trim()
            .starts_with("swaraj ")
    );
}

#[test]
fn checks_multiple_files_in_one_invocation() {
    let first = write_temp_source("package first:\n  version: 1\n");
    let second = write_temp_source("package second:\n  version: 2\n");
    let output = run(&[
        "check",
        first.to_str().expect("temporary path should be UTF-8"),
        second.to_str().expect("temporary path should be UTF-8"),
    ]);
    let _ = fs::remove_file(&first);
    let _ = fs::remove_file(&second);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("valid package `first`"));
    assert!(stdout.contains("valid package `second`"));
}

#[test]
fn checks_packages_with_relative_imports() {
    let imported = write_temp_source("package base:\n  version: \"1.0\"\n");
    let imported_name = imported
        .file_name()
        .expect("temporary file should have a name")
        .to_string_lossy();
    let root = write_temp_source(&format!(
        "let:\n  base: builtins.import \"{imported_name}\"\nin:\npackage app:\n  version: base.version\n"
    ));
    let output = run(&[
        "check",
        root.to_str().expect("temporary path should be UTF-8"),
    ]);
    let _ = fs::remove_file(&root);
    let _ = fs::remove_file(&imported);

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("valid package `app`"));
}

#[test]
fn reports_syntax_errors_and_continues_checking_other_files() {
    let invalid = write_temp_source("package broken:\n  values: [one,,two]\n");
    let valid = write_temp_source("package healthy:\n  version: 1\n");
    let output = run(&[
        "check",
        invalid.to_str().expect("temporary path should be UTF-8"),
        valid.to_str().expect("temporary path should be UTF-8"),
    ]);
    let _ = fs::remove_file(&invalid);
    let _ = fs::remove_file(&valid);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("{}:2:16: error:", invalid.display())));
    assert!(stderr.contains("help:"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("valid package `healthy`"));
}

#[test]
fn rejects_unknown_commands_and_missing_files() {
    let unknown = run(&["build", "package.sw"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("Usage:"));

    let missing =
        std::env::temp_dir().join(format!("swaraj-cli-missing-{}.sw", std::process::id()));
    let output = run(&[
        "check",
        missing.to_str().expect("temporary path should be UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not read file"));
}
