//! Command-line input/output around the source-preserving ASON formatter.
//!
//! Read and validate all input before touching the destination. File output is
//! always plain text; terminal coloring is a separate decision for standard
//! output. Input and output aliases are rejected rather than formatted in place.

use std::{
    fs::{File, Metadata, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use ason_format::{FormatOptions, format_with_options, highlight_ansi};
use clap::{Parser, ValueEnum};

/// Command-line settings parsed by clap, which also supplies help and usage errors.
#[derive(Parser)]
#[command(
    name = "asonfmt",
    version,
    about = "Format ASON while preserving literals and comments"
)]
struct Args {
    /// Input file; omit or use - to read standard input
    // Keep this help text short. `None` and the literal path `-` both select stdin.
    input: Option<PathBuf>,
    /// Output file; omit or use - to write standard output
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Spaces per indentation level
    // Zero retains structural newlines without adding indentation spaces.
    #[arg(long, default_value_t = 4)]
    indent_width: usize,
    /// Color standard output (output files are always plain text)
    #[arg(long, value_enum, default_value_t = Color::Auto)]
    color: Color,
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

/// Distinguishes a real file argument from an omitted path or the `-` stream marker.
fn is_file(path: Option<&Path>) -> bool {
    path.is_some_and(|path| path != Path::new("-"))
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

/// Executes one formatting request using the parsed `args`.
///
/// The whole source is read as UTF-8 and formatted before opening an output file,
/// so invalid input cannot truncate an existing destination. An opened output
/// handle is checked for aliases before truncation. Write failures can still
/// leave partial file output: this is not an atomic file-replacement operation.
/// Errors include the relevant input or output label when available.
fn run(args: Args) -> io::Result<()> {
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
        },
    )
    .map_err(|error| invalid(error, &input_label))?;
    let output_path = args.output.as_deref();
    if is_file(output_path) {
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
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("asonfmt: {error}");
            ExitCode::FAILURE
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
