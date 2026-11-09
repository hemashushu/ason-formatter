# ASON Formatter

Source-preserving formatting and syntax highlighting for [ASON](https://github.com/hemashushu/ason).

This repository provides the `ason-format` Rust library and the `asonfmt` command-line tool.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [1 Features](#1-features)
- [2 Using the `asonfmt` CLI](#2-using-the-asonfmt-cli)
- [3 Using the `ason-format` Library](#3-using-the-ason-format-library)
  - [3.1 Formatting](#31-formatting)
  - [3.2 Highlighting](#32-highlighting)
- [4 Links](#4-links)
- [5 License](#5-license)

<!-- /code_chunk_output -->

## 1 Features

- Source-preserving formatting for ASON documents.
- Syntax highlighting with ANSI and HTML output.
- Configurable indentation and layout options.
- Validation of collection type consistency.

## 2 Using the `asonfmt` CLI

Install the `asonfmt` CLI with Cargo by running `cargo install asonfmt`. To
install it from a local clone of the repository, run
`cargo install --path crates/asonfmt`.

Alternatively, build from source with `cargo build -p asonfmt`.

```text
NAME
    asonfmt - Source-preserving formatting and syntax highlighting for ASON documents

USAGE
    asonfmt [OPTIONS] [INPUT]

ARGUMENTS
    INPUT                   Input file (defaults to standard input when omitted)

OPTIONS
    -r, --replace             Overwrite the input file with formatted output
        --replace-atomic      Use atomic replacement when overwriting the input file
    -o, --output OUTPUT       Write output to this file (defaults to standard output when omitted)
    -c, --color COLOR         Set color mode (auto, always, never)
        --indent-width WIDTH  Spaces per indentation level (default: 4)
        --complete SHELL      Print a completion script to standard output
                              Supported shells: bash, zsh, fish, PowerShell
    -h, --help                Show help information
    -V, --version             Show version information

EXIT STATUS
    0   Successful formatting
    1   Invalid arguments
    2   Document error and failed formatting
    3   I/O errors

DESCRIPTION
    `INPUT` is optional; if omitted, input is read from stdin. A path named `-`
    is treated as a file. `-o/--output FILE` is also optional and defaults to
    stdout when omitted; an output path named `-` is treated as a file. The
    `--color` option accepts `auto`, `always`, or `never` and defaults to `auto`.
    Automatic color is enabled only when stdout is a terminal, `NO_COLOR` is
    unset, and `TERM` is not `dumb`. Explicit `always` or `never` overrides
    these environment settings, but output files are never colored.

    The `-r/--replace` option replaces the input file with formatted output.
    By default, replacement is not atomic, so a write failure can leave a
    partially written file. Use `--replace-atomic` for safer in-place updates.

EXAMPLES
    * stdin to stdout; colors are automatic when stdout is a terminal
    printf '{answer: 42}' | asonfmt

    * File to stdout; colors are automatic when stdout is a terminal
    asonfmt input.ason

    * File to file; no color applied
    asonfmt input.ason -o output.ason

    * Piped input and layout/color settings
    cat input.ason | asonfmt --indent-width 2 --color never
```

## 3 Using the `ason-format` Library

Add the following to your `Cargo.toml`:

```toml
[dependencies]
ason-format = "1.0"
```

### 3.1 Formatting

```rust
use ason_format::{FormatOptions, format, format_with_options};

fn main() -> Result<(), ason_format::AsonError> {
  let source = r#"{name: "Alice", values: [0xff, 0b10]}"#;
  let formatted = format(source)?;
  let two_spaces = format_with_options(source, &FormatOptions { indent_width: 2 })?;
  println!("{formatted}");
  println!("{two_spaces}");
  Ok(())
}
```

Default output:

```text
{name: "Alice" values: [0xff 0b10]}
```

Formatting validates that the input contains exactly one root value before applying these rules:

- Four spaces per level by default; zero indentation still emits line breaks.
- Nonempty containers and enumeration payloads without comments or multiline
  literals stay on one line when the formatted line, including indentation, fits
  within 80 characters; otherwise, they span multiple lines. For example,
  `[1 2 3]` stays on one line.
- Empty `[]` and `{}` stay compact unless they contain comments; empty tuples are invalid.
- One space follows a colon. Optional commas and extra blank lines are removed.
- Generated line breaks use LF, and the document ends with one layout newline.
- Literal spelling and comment bodies are copied byte for byte, including escapes,
  numeric bases and suffixes, raw delimiters, and internal whitespace or CRLF.
- Multi-line literals and comments are not reindented internally. Their original
  indentation may therefore differ from the surrounding generated layout.
- Comment order is preserved; same-line trailing comments remain trailing,
  standalone comments remain on separate lines, and line comments force a newline.

The formatter checks collection type consistency, including nested collections.
Missing object fields and empty collections remain compatible with types inferred
from other elements. Target-specific defaults and the complete definitions of
enumeration types cannot be validated without an application schema.

### 3.2 Highlighting

```rust
use ason_format::{highlight_spans, highlight_ansi, highlight_html};

fn main() -> Result<(), ason_format::AsonError> {
  let source = "{count: 0xff} // unchanged source\n";
  let spans = highlight_spans(source)?;
  let terminal_text = highlight_ansi(source)?;
  let html = highlight_html(source)?;
  println!("{spans:?}");
  println!("{terminal_text}");
  println!("{html}");
  Ok(())
}
```

- `highlight_spans` returns ordered `HighlightSpan { kind, span }` values. Each
  `span` is a half-open UTF-8 byte range (`[start..end)`) in the input; whitespace
  is omitted.
- `highlight_ansi` inserts SGR colors and resets without formatting the source.
  It does not detect terminals or sanitize existing input control characters;
  take care when displaying untrusted documents in a terminal.
- `highlight_html` returns `<pre class="ason"><code>...</code></pre>` with escaped
  source text and fixed token classes. It does not include CSS or JavaScript.
- Highlighting requires valid lexical tokens, not a complete document structure.
  Unterminated strings and comments return errors; incremental error recovery is
  not implemented. Empty or whitespace-only input can be highlighted.

Stable HTML classes: `ason-number`, `ason-boolean`, `ason-char`, `ason-string`,
`ason-datetime`, `ason-bytes`, `ason-identifier`, `ason-enumeration`, `ason-comment`,
and `ason-punctuation`. `HighlightKind::css_class()` returns these names.

Example stylesheet:

```css
.ason { color: #24292f; background: #f6f8fa; white-space: pre; }
.ason-number, .ason-boolean { color: #0550ae; }
.ason-string, .ason-char { color: #116329; }
.ason-datetime, .ason-bytes { color: #953800; }
.ason-identifier { color: #8250df; }
.ason-enumeration { color: #9a6700; }
.ason-comment { color: #57606a; }
.ason-punctuation { color: #24292f; }
```

## 4 Links

- [ASON Specification](https://github.com/hemashushu/ason/blob/main/docs/specification.md)
- [ASON Converter](https://github.com/hemashushu/ason-converter)

## 5 License

This project is licensed under the MPL 2.0 License with additional terms. See [LICENSE](./LICENSE) and [LICENSE.additional](./LICENSE.additional).
