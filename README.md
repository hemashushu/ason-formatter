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
  - [3.3 WASM](#33-wasm)
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
        --indent-width WIDTH  Spaces per level; with tabs, visual columns per tab for line fitting (default: 4)
        --indent-style STYLE  Indentation style: spaces, tabs (default: spaces)
        --line-ending ENDING  Structural line ending: lf, crlf (default: lf)
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
    stdout when omitted; an output path named `-` is treated as a file.

    The `--color` option accepts `auto`, `always`, or `never` and defaults to `auto`.
    Automatic color is enabled only when stdout is a terminal, `NO_COLOR` is
    unset, and `TERM` is not `dumb`. Explicit `always` or `never` overrides
    these environment settings, but output files are never colored.

    The `-r/--replace` option replaces the input file with formatted output.
    By default, replacement is not atomic, so a write failure can leave a
    partially written file. Use `--replace-atomic` for safer in-place updates.

    With `--indent-style tabs`, the formatter emits one tab per nesting level.
    `--indent-width` then sets how many visual columns each tab occupies when
    fitting lines; it does not set the number of tabs. A width of zero still
    emits tabs, but counts them as zero columns for line fitting. With spaces,
    the width is the number of spaces per level, and zero emits no indentation.

    `--line-ending` controls generated structural breaks; line endings inside
    literals and comments are preserved as written.

EXAMPLES
    * stdin to stdout; colors are automatic when stdout is a terminal
    printf '{answer: 42}' | asonfmt

    * File to stdout; colors are automatic when stdout is a terminal
    asonfmt input.ason

    * File to file; no color applied
    asonfmt input.ason -o output.ason

    * Piped input with tabs and CRLF structural line endings
    cat input.ason | asonfmt --indent-style tabs --indent-width 4 --line-ending crlf
```

## 3 Using the `ason-format` Library

Add the following to your `Cargo.toml`:

```toml
[dependencies]
ason-format = "1.1"
```

### 3.1 Formatting

```rust
use ason_format::{FormatOptions, format, format_with_options};

fn main() -> Result<(), ason_format::AsonError> {
  let source = r#"{name: "Alice", values: [0xff, 0b10]}"#;
  let formatted = format(source)?;
  let two_spaces = format_with_options(source, &FormatOptions {
    indent_width: 2,
    ..FormatOptions::default()
  })?;
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

`FormatOptions` also controls structural indentation and line endings. By
default it uses four spaces and LF. With `IndentStyle::Tabs`, the formatter
emits one tab per nesting level; `indent_width` sets how many visual columns
each tab occupies when deciding whether a container fits on one line, not how
many tab characters are emitted. A width of zero still emits one tab per level
but counts each tab as zero columns for line fitting:

```rust
use ason_format::{FormatOptions, IndentStyle, LineEnding, format_with_options};

let options = FormatOptions {
  indent_width: 4,
  indent_style: IndentStyle::Tabs,
  line_ending: LineEnding::CrLf,
};
let formatted = format_with_options("{answer:42}", &options)?;
# Ok::<(), ason_format::AsonError>(())
```

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

### 3.3 WASM

`ason-format-wasm` is a private workspace crate that exposes the formatter and
document validator to browser and VS Code integrations through
[`wasm-bindgen`](https://github.com/rustwasm/wasm-bindgen). It uses the same
validation and layout behavior as the Rust library. The crate is not published
separately to crates.io.

To build the WebAssembly module and generate browser bindings from the repository
root, install the target and the matching `wasm-bindgen-cli` version, then run:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
cargo build -p ason-format-wasm --target wasm32-unknown-unknown --release --locked
wasm-bindgen target/wasm32-unknown-unknown/release/ason_format_wasm.wasm \
  --target web --out-dir pkg
```

The generated Web module exports `formatDocument(source, tabSize, insertSpaces,
crlf)` and `validateDocument(source)`. `formatDocument` returns a JSON string;
`tabSize` is the indentation width, `insertSpaces` selects spaces instead of
tabs, and `crlf` selects CRLF instead of LF for generated structural line
breaks. With tabs, one tab is emitted per nesting level and `tabSize` is its
visual width for line fitting. `validateDocument` validates without formatting.

```js
import init, { formatDocument, validateDocument } from "./pkg/ason_format_wasm.js";

await init();
const result = JSON.parse(formatDocument("{answer:42}", 4, true, false));
const validation = JSON.parse(validateDocument("{answer:42}"));
```

Successful formatting returns `{"status":"success","text":"..."}`;
successful validation returns `{"status":"success","text":null}`. Failures
return `{"status":"error","error":{"message":"...","kind":"syntax","start":0,"end":1}}`.
`kind` is `syntax`, `eof`, or `document`. `start` and `end` are UTF-16 offsets
with an exclusive end; they are `null` when an error has no source range, such
as a collection type mismatch.

## 4 Links

- [ASON Specification](https://github.com/hemashushu/ason/blob/main/docs/specification.md)
- [ASON Converter](https://github.com/hemashushu/ason-converter)

## 5 License

This project is licensed under the MPL 2.0 License with additional terms. See [LICENSE](./LICENSE) and [LICENSE.additional](./LICENSE.additional).
