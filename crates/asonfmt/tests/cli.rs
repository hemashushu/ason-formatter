// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! End-to-end checks against the built CLI, including streams and real files.
//!
//! Each child gets a predictable color environment, and each file-based test gets
//! its own temporary directory so the test runner can execute cases in parallel.

use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

/// Creates a command for Cargo's built binary with known color-related defaults.
///
/// Clear the inherited opt-out and set a non-dumb terminal name. Individual tests
/// can override either setting without depending on the developer's environment.
fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asonfmt"));
    command.env_remove("NO_COLOR").env("TERM", "xterm");
    command
}

/// Runs the binary with `args`, feeds raw `source` bytes, and captures both streams.
///
/// Bytes rather than a string let tests send invalid UTF-8. Taking and dropping
/// the stdin handle after writing signals EOF, allowing the child to finish its
/// read-to-end operation before we wait for its exit status.
fn stdin(args: &[&str], source: &[u8]) -> Output {
    let mut child = command()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(source).unwrap();
    child.wait_with_output().unwrap()
}

/// Owns a test directory and removes it when it leaves scope.
struct Temp(
    /// Absolute path of the directory containing this test's files and aliases.
    PathBuf,
);

impl Temp {
    /// Creates a directory named uniquely for this process and test invocation.
    fn new() -> Self {
        // The process ID separates concurrent test processes; the counter keeps
        // parallel tests within this process apart. It does not publish shared
        // data, so relaxed ordering is enough to obtain distinct counter values.
        /// The next directory suffix within this test process.
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "ason-format-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    /// Returns a child path for `name` without creating a file at that path.
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Temp {
    /// Removes the directory and its contents, surfacing cleanup failures in tests.
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
/// Keeps the public executable name consistent in help, version, and diagnostics.
fn executable_name() {
    let help = command().arg("--help").output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("Usage: asonfmt"));
    assert!(help.contains("--replace"));
    assert!(help.contains("--replace-atomic"));
    assert!(help.contains("-c, --color <COLOR>"));
    assert!(help.contains("--indent-style <INDENT_STYLE>"));
    assert!(help.contains("--line-ending <LINE_ENDING>"));
    assert!(help.contains("--complete <SHELL>"));
    assert!(help.contains("PowerShell"));
    let version = command().arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("asonfmt "));
    let error = stdin(&[], b"[");
    assert!(String::from_utf8_lossy(&error.stderr).starts_with("asonfmt:"));
}

#[test]
/// Checks omitted-path stream defaults, indentation, and forced colors.
fn stdin_stdout_and_color() {
    let output = stdin(&[], b"{a:1}");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{a: 1}\n");
    assert!(output.stderr.is_empty());
    let output = stdin(&["--indent-width", "2"], b"[1 2]");
    assert_eq!(output.stdout, b"[1 2]\n");
    let output = stdin(
        &[
            "--indent-style",
            "tabs",
            "--indent-width",
            "2",
            "--line-ending",
            "crlf",
        ],
        b"{/* comment */a:1}",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"{\r\n\t/* comment */\r\n\ta: 1\r\n}\r\n");
    let output = stdin(&["--color", "always"], b"1");
    assert!(output.status.success());
    assert!(output.stdout.contains(&0x1b));
    let output = stdin(&["--color", "never"], b"1");
    assert_eq!(output.stdout, b"1\n");
    let output = stdin(&["-c", "never"], b"1");
    assert_eq!(output.stdout, b"1\n");
}

#[test]
/// Treats a path spelled `-` as a file for both input and output.
fn dash_is_a_file_path() {
    let temp = Temp::new();
    let dash = temp.file("-");
    fs::write(&dash, "{a:1}").unwrap();
    let result = command()
        .current_dir(&temp.0)
        .arg("-")
        .args(["--color", "never"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    assert_eq!(result.stdout, b"{a: 1}\n");
    assert_eq!(fs::read(&dash).unwrap(), b"{a:1}");

    let input = temp.file("input.ason");
    fs::write(&input, "[1 2]").unwrap();
    let result = command()
        .current_dir(&temp.0)
        .args(["input.ason", "-o", "-"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(result.stdout.is_empty());
    assert_eq!(fs::read(&dash).unwrap(), b"[1 2]\n");
}

#[test]
/// File output stays plain, and invalid input must leave an existing file intact.
fn files_and_output_protection() {
    let temp = Temp::new();
    let input = temp.file("input.ason");
    let output = temp.file("output.ason");
    fs::write(&input, "[0xff, 0b10]").unwrap();
    let result = command()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .args(["--color", "always"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(result.stdout.is_empty());
    assert_eq!(fs::read_to_string(&output).unwrap(), "[0xff 0b10]\n");
    let result = command().arg(&input).output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, fs::read(&output).unwrap());
    // Reuse a populated destination to detect accidental early truncation.
    fs::write(&input, "[broken").unwrap();
    let before = fs::read(&output).unwrap();
    let result = command()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!result.stderr.is_empty());
    assert!(result.stdout.is_empty());
    assert_eq!(fs::read(&output).unwrap(), before);
}

#[test]
/// Applies the documented non-atomic and atomic in-place replacement modes.
fn in_place_replacement_modes() {
    let temp = Temp::new();
    let input = temp.file("input.ason");
    fs::write(&input, "{a:1}").unwrap();
    let result = command().arg(&input).arg("--replace").output().unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(result.stdout.is_empty());
    assert_eq!(fs::read(&input).unwrap(), b"{a: 1}\n");

    fs::write(&input, "[1 2]").unwrap();
    let result = command()
        .arg(&input)
        .arg("--replace-atomic")
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(result.stdout.is_empty());
    assert_eq!(fs::read(&input).unwrap(), b"[1 2]\n");

    let result = command()
        .args(["--replace"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
}

#[test]
/// Emits completion scripts for each shell named by the README.
fn completion_scripts() {
    for shell in ["bash", "zsh", "fish", "PowerShell", "powershell"] {
        let result = command().args(["--complete", shell]).output().unwrap();
        assert!(result.status.success(), "{shell}: {:?}", result.stderr);
        assert!(!result.stdout.is_empty(), "{shell}");
        assert!(String::from_utf8_lossy(&result.stdout).contains("asonfmt"));
    }
}

#[test]
/// Rejects the original path and filesystem aliases without changing input bytes.
fn rejects_aliases_without_truncating() {
    let temp = Temp::new();
    let input = temp.file("input.ason");
    let hard = temp.file("hard.ason");
    fs::write(&input, "[1 2]").unwrap();
    fs::hard_link(&input, &hard).unwrap();
    // Hard links share file identity even though their canonical paths differ.
    // Unix also lets us check the symlink case without platform-specific setup.
    let aliases = vec![input.clone(), hard];
    #[cfg(unix)]
    let aliases = {
        let mut aliases = aliases;
        let link = temp.file("link.ason");
        std::os::unix::fs::symlink(&input, &link).unwrap();
        aliases.push(link);
        aliases
    };
    for output in aliases {
        let result = command()
            .arg(&input)
            .arg("-o")
            .arg(output)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(3));
        assert_eq!(fs::read(&input).unwrap(), b"[1 2]");
    }
}

#[test]
/// Separates data/I/O failures from argument errors and keeps stdout clean.
fn errors_are_on_stderr() {
    for source in [b"[".as_slice(), b"1 2", b"\xff"] {
        let result = stdin(&[], source);
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("stdin"));
    }
    let result = command().arg("--unknown").output().unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    let temp = Temp::new();
    let result = command().arg(temp.file("absent.ason")).output().unwrap();
    assert_eq!(result.status.code(), Some(3));
    assert!(!result.stderr.is_empty());
    // A directory is not a writable output file, even when the input is valid.
    let result = stdin(&["-o", temp.0.to_str().unwrap()], b"1");
    assert_eq!(result.status.code(), Some(3));
}

#[test]
/// Forced color modes take priority over both common environment opt-outs.
fn explicit_colors_override_environment() {
    let temp = Temp::new();
    let input = temp.file("input.ason");
    fs::write(&input, "1").unwrap();
    for mode in ["auto", "always", "never"] {
        let result = command()
            .arg(&input)
            .args(["--color", mode])
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout.contains(&0x1b), mode == "always");
    }
}

#[test]
/// A consumer closing stdout early is treated as normal pipeline termination.
fn broken_pipe_exits_successfully() {
    let mut child = command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Close the pipe's read end before sending input, making the child's write
    // fail with a broken pipe instead of relying on timing with a real consumer.
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(b"[1 2]").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
}
