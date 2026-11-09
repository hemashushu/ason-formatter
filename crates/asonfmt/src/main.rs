// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Command-line input/output around the source-preserving ASON formatter.
//!
//! Read and validate all input before touching the destination. File output is
//! always plain text; terminal coloring is a separate decision for standard
//! output. Input and output aliases are rejected rather than formatted in place.

use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use ason_format::{FormatOptions, IndentStyle, LineEnding, format_with_options, highlight_ansi};
use clap::{CommandFactory, Parser, ValueEnum};

/// Command-line settings parsed by clap, which also supplies help and usage errors.
#[derive(Parser)]
#[command(
    name = "asonfmt",
    version,
    about = "Source-preserving formatting and syntax highlighting for ASON documents"
)]
struct Args {
    /// Input file; defaults to standard input when omitted
    input: Option<PathBuf>,
    /// Overwrite the input file with formatted output (not atomic)
    #[arg(
        short,
        long,
        requires = "input",
        conflicts_with_all = ["replace_atomic", "output", "complete"]
    )]
    replace: bool,
    /// Atomically replace the input file with formatted output
    #[arg(
        long,
        requires = "input",
        conflicts_with_all = ["replace", "output", "complete"]
    )]
    replace_atomic: bool,
    /// Output file; defaults to standard output when omitted
    #[arg(
        short,
        long,
        value_name = "OUTPUT",
        conflicts_with_all = ["replace", "replace_atomic", "complete"]
    )]
    output: Option<PathBuf>,
    /// Indentation width: spaces per level, or tab width for line fitting
    // Tabs still emit one tab per nesting level, regardless of this width.
    #[arg(long, default_value_t = 4)]
    indent_width: usize,
    /// Structural indentation style
    #[arg(long, value_enum, default_value_t = CliIndentStyle::Spaces)]
    indent_style: CliIndentStyle,
    /// Structural line ending
    #[arg(long, value_enum, default_value_t = CliLineEnding::Lf)]
    line_ending: CliLineEnding,
    /// Color standard output (output files are always plain text)
    #[arg(short = 'c', long, value_enum, default_value_t = Color::Auto)]
    color: Color,
    /// Print a completion script to standard output
    #[arg(
        long,
        value_enum,
        value_name = "SHELL",
        conflicts_with_all = ["input", "output", "replace", "replace_atomic"]
    )]
    complete: Option<CompletionShell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliIndentStyle {
    Spaces,
    Tabs,
}

impl From<CliIndentStyle> for IndentStyle {
    fn from(style: CliIndentStyle) -> Self {
        match style {
            CliIndentStyle::Spaces => Self::Spaces,
            CliIndentStyle::Tabs => Self::Tabs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliLineEnding {
    Lf,
    #[value(name = "crlf")]
    CrLf,
}

impl From<CliLineEnding> for LineEnding {
    fn from(line_ending: CliLineEnding) -> Self {
        match line_ending {
            CliLineEnding::Lf => Self::Lf,
            CliLineEnding::CrLf => Self::CrLf,
        }
    }
}

/// Requested color policy for stdout; this never affects an output file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Color {
    /// Use colors only on a terminal that has not opted out through its environment.
    Auto,
    /// Emit colors even when stdout is redirected or the environment opts out.
    Always,
    /// Emit plain text regardless of terminal capabilities or environment settings.
    Never,
}

/// Shells supported by the completion-script generator.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "PowerShell", alias = "powershell")]
    PowerShell,
}

impl From<CompletionShell> for clap_complete::Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Zsh => Self::Zsh,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::PowerShell => Self::PowerShell,
        }
    }
}

impl Color {
    /// Resolves the policy using terminal and environment information.
    ///
    /// `terminal` means stdout is a terminal, `no_color` means `NO_COLOR` is set
    /// (even to an empty value), and `dumb` means `TERM` equals `dumb`. Explicit
    /// policies override all three inputs; only automatic mode consults them.
    fn enabled(self, terminal: bool, no_color: bool, dumb: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => terminal && !no_color && !dumb,
        }
    }
}

/// Checks whether a file path argument was supplied.
fn is_file(path: Option<&Path>) -> bool {
    path.is_some()
}

/// Adds a path or stream `label` to an I/O `error` while keeping its error kind.
///
/// Preserving the kind lets the entry point recognize broken pipes later.
fn contextual(error: io::Error, label: &str) -> io::Error {
    io::Error::new(error.kind(), format!("{label}: {error}"))
}

/// Wraps a parser or formatter `error` as invalid data and identifies its input.
fn invalid(error: impl std::fmt::Display, label: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {error}"))
}

/// Checks whether the opened input and output appear to identify the same file.
///
/// `input` and `output` describe the opened handles. On Unix, device and inode
/// identify the underlying file even through hard links or symlinks. Elsewhere,
/// `input_path` and `output_path` are canonicalized as a best-effort fallback;
/// failed canonicalization returns false, and hard-link aliases may go undetected.
fn same_file(input: &Metadata, output: &Metadata, input_path: &Path, output_path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = (input_path, output_path);
        input.dev() == output.dev() && input.ino() == output.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (input, output);
        match (input_path.canonicalize(), output_path.canonicalize()) {
            (Ok(input), Ok(output)) => input == output,
            _ => false,
        }
    }
}

/// Replaces an existing file by renaming a fully written sibling temporary file.
fn atomic_replace(path: &Path, contents: &[u8]) -> io::Result<()> {
    let target = path.canonicalize()?;
    let permissions = fs::metadata(&target)?.permissions();
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let mut replacement = tempfile::NamedTempFile::new_in(parent)?;
    replacement.write_all(contents)?;
    replacement.as_file().set_permissions(permissions)?;
    replacement.as_file().sync_all()?;
    replacement.persist(&target).map_err(|error| error.error)?;
    Ok(())
}

/// Parses arguments and rejects replacement requests that target standard input.
fn parse_args() -> Result<Args, clap::Error> {
    let args = Args::try_parse()?;
    if (args.replace || args.replace_atomic) && !is_file(args.input.as_deref()) {
        return Err(Args::command().error(
            clap::error::ErrorKind::InvalidValue,
            "in-place replacement requires an input file; stdin cannot be replaced",
        ));
    }
    Ok(args)
}

/// Executes one formatting request using the parsed `args`.
///
/// The whole source is read as UTF-8 and formatted before opening an output file,
/// so invalid input cannot truncate an existing destination. An opened output
/// handle is checked for aliases before truncation. Write failures can still
/// leave partial file output: this is not an atomic file-replacement operation.
/// Errors include the relevant input or output label when available.
fn run(args: Args) -> io::Result<()> {
    if let Some(shell) = args.complete {
        let mut script = Vec::new();
        let shell: clap_complete::Shell = shell.into();
        clap_complete::generate(shell, &mut Args::command(), "asonfmt", &mut script);
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        stdout.write_all(&script)?;
        stdout.flush()?;
        return Ok(());
    }

    let input_path = args.input.as_deref();
    let input_label = if is_file(input_path) {
        input_path.unwrap().display().to_string()
    } else {
        "stdin".into()
    };
    let mut source = String::new();
    let input_metadata = if is_file(input_path) {
        let mut file =
            File::open(input_path.unwrap()).map_err(|error| contextual(error, &input_label))?;
        // Inspect the same handle we read, rather than looking the path up again.
        // Its identity is needed later to reject output aliases before truncation.
        let metadata = file
            .metadata()
            .map_err(|error| contextual(error, &input_label))?;
        file.read_to_string(&mut source)
            .map_err(|error| contextual(error, &input_label))?;
        Some(metadata)
    } else {
        io::stdin()
            .lock()
            .read_to_string(&mut source)
            .map_err(|error| contextual(error, &input_label))?;
        None
    };
    // Finish validation before opening a destination that might already exist.
    let formatted = format_with_options(
        &source,
        &FormatOptions {
            indent_width: args.indent_width,
            indent_style: args.indent_style.into(),
            line_ending: args.line_ending.into(),
            ..FormatOptions::default()
        },
    )
    .map_err(|error| invalid(error, &input_label))?;
    let output_path = args.output.as_deref();
    if args.replace_atomic {
        atomic_replace(input_path.unwrap(), formatted.as_bytes())
            .map_err(|error| contextual(error, &input_label))?;
    } else if args.replace {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(input_path.unwrap())
            .map_err(|error| contextual(error, &input_label))?;
        file.write_all(formatted.as_bytes())
            .map_err(|error| contextual(error, &input_label))?;
    } else if is_file(output_path) {
        let path = output_path.unwrap();
        let label = path.display().to_string();
        // Do not truncate on open: the destination may be a hard link or symlink
        // to the input. Check the opened handle first, then explicitly truncate.
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| contextual(error, &label))?;
        if let Some(input_metadata) = input_metadata {
            let output_metadata = file.metadata().map_err(|error| contextual(error, &label))?;
            if same_file(&input_metadata, &output_metadata, input_path.unwrap(), path) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Input and output refer to the same file; in-place formatting is not supported.",
                ));
            }
        }
        file.set_len(0).map_err(|error| contextual(error, &label))?;
        file.write_all(formatted.as_bytes())
            .map_err(|error| contextual(error, &label))?;
    } else {
        let stdout = io::stdout();
        // Terminal detection belongs at the CLI boundary. The library renderer
        // deliberately leaves environment variables and output capabilities alone.
        let color = args.color.enabled(
            stdout.is_terminal(),
            std::env::var_os("NO_COLOR").is_some(),
            std::env::var_os("TERM").is_some_and(|term| term == "dumb"),
        );
        let output = if color {
            highlight_ansi(&formatted).map_err(|error| invalid(error, &input_label))?
        } else {
            formatted
        };
        // Hold one lock for the write and flush so buffered errors reach `main`.
        let mut stdout = stdout.lock();
        stdout.write_all(output.as_bytes())?;
        stdout.flush()?;
    }
    Ok(())
}

/// Parses arguments, reports failures on stderr, and chooses the process exit code.
///
/// A broken pipe is a successful early stop, as when a downstream command has
/// read enough output. Clap handles its own argument errors before `run` starts.
fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            let code = if error.use_stderr() { 1 } else { 0 };
            let _ = error.print();
            return ExitCode::from(code);
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            let code = if error.kind() == io::ErrorKind::InvalidData {
                2
            } else {
                3
            };
            eprintln!("asonfmt: {error}");
            ExitCode::from(code)
        }
    }
}

#[cfg(test)]
/// Tests policy decisions without depending on the test runner's actual terminal.
mod tests {
    use super::*;

    #[test]
    /// Checks every environment combination and both explicit color overrides.
    fn color_policy() {
        for terminal in [false, true] {
            for no_color in [false, true] {
                for dumb in [false, true] {
                    assert_eq!(
                        Color::Auto.enabled(terminal, no_color, dumb),
                        terminal && !no_color && !dumb
                    );
                    assert!(Color::Always.enabled(terminal, no_color, dumb));
                    assert!(!Color::Never.enabled(terminal, no_color, dumb));
                }
            }
        }
    }
}
