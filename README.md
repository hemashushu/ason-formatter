# ASON Formatter

ASON Formatter formats, validates, and syntax-highlights [ASON](https://github.com/hemashushu/ason) documents. It is available in three forms:

| Form | Package | Use it for |
|------|---------|------------|
| Command-line tool | [`asonfmt`](https://crates.io/crates/asonfmt) | Formatting and validating files in a terminal, scripts, or CI |
| Rust library | [`ason-format`](https://crates.io/crates/ason-format) | Formatting, validating, and highlighting ASON in Rust programs |
| WebAssembly package | [`@hemashushu/ason-format-wasm`](https://www.npmjs.com/package/@hemashushu/ason-format-wasm) | Formatting and validating ASON in browsers, workers, and Node.js |

All three share the same formatter. It preserves the exact spelling of literals and comments, and it checks that values in each list or map have compatible types.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [1 The command line](#1-the-command-line)
  - [1.1 Install `asonfmt`](#11-install-asonfmt)
  - [1.2 Format and validate ASON documents](#12-format-and-validate-ason-documents)
  - [1.3 What formatting changes](#13-what-formatting-changes)
- [2 The Rust library](#2-the-rust-library)
  - [2.1 Install the crate](#21-install-the-crate)
  - [2.2 Formatting](#22-formatting)
  - [2.3 Highlighting](#23-highlighting)
  - [2.4 Validating](#24-validating)
- [3 The WebAssembly package](#3-the-webassembly-package)
  - [3.1 Install the WebAssembly package](#31-install-the-webassembly-package)
  - [3.2 Use the WebAssembly package](#32-use-the-webassembly-package)
  - [3.3 Build and publish](#33-build-and-publish)
- [4 More information](#4-more-information)
- [5 License](#5-license)

<!-- /code_chunk_output -->

## 1 The command line

### 1.1 Install `asonfmt`

Install the command-line tool with Cargo:

```sh
cargo install asonfmt
```

### 1.2 Format and validate ASON documents

`asonfmt` reads a file or standard input and writes the formatted document to standard output:

```sh
asonfmt input.ason
cat input.ason | asonfmt
```

Write to another file, or replace the input file in place:

```sh
asonfmt input.ason --output formatted.ason
asonfmt input.ason --replace
asonfmt input.ason --replace-atomic
```

`-r`/`--replace` overwrites the input file directly, so a failed write can leave it partially written. `--replace-atomic` writes a temporary file first and then replaces the input with it, which is safer for in-place updates.

Validate a document without formatting it:

```sh
asonfmt --validate input.ason
```

With `-v`/`--validate`, nothing is written on success. An invalid document is reported on standard error with its location (lines and columns start at 1), for example:

```text
asonfmt: input.ason: Error from line: 3 column: 5, to line: 3 column: 8
List element type does not match the earlier elements.
```

`--validate` cannot be combined with output, replacement, layout, color, or `--complete` options.

Common options:

- `--indent-width 2` uses two spaces per level. With `--indent-style tabs`, the formatter emits one tab per level, and the width only sets how many columns a tab counts for when deciding whether content fits on one line.
- `--indent-style tabs` indents with tabs instead of spaces.
- `--line-ending crlf` uses CRLF for generated line breaks. The default is LF.
- `-c`/`--color auto|always|never` controls terminal colors. `auto`, the default, colors output only when standard output is a terminal, `NO_COLOR` is unset, and `TERM` is not `dumb`. Output files are never colored.
- `--complete bash|zsh|fish|powershell` prints a shell completion script.
- `-h`/`--help` lists all options.

For detailed command-line documentation, see [`crates/asonfmt/docs/asonfmt.txt`](https://github.com/hemashushu/ason-formatter/blob/main/crates/asonfmt/docs/asonfmt.txt).

The tool returns `0` on success, `1` for invalid arguments, `2` for invalid documents (including non-UTF-8 input), and `3` for input/output errors. A broken output pipe is treated as normal pipeline termination.

### 1.3 What formatting changes

For example, this input:

```text
{name:"Alice",tags:["admin","dev",],


// Optional contact
contact:Option::Some({email:"alice@example.com",phone:None}),scores:[0x10,+2_i32,0b11]}
```

is formatted as:

```text
{
    name: "Alice"
    tags: ["admin" "dev"]
    // Optional contact
    contact: Option::Some({email: "alice@example.com" phone: None})
    scores: [0x10 +2_i32 0b11]
}
```

The formatter checks that the input contains exactly one root value, then lays it out consistently:

- A nonempty container (an object, list, map, tuple, or enumeration payload) stays on one line when it fits within 80 columns and contains no comments or multiline literals. Otherwise, each entry goes on its own line, one level deeper, and the closing delimiter goes on its own line. In the example, the root object does not fit on one line, while the nested values do.
- Empty lists and objects are written as `[]` and `{}` unless they contain comments.
- A space follows each colon, commas are removed (entries are separated by spaces or line breaks), blank lines are removed, and the output ends with a newline.
- By default, indentation is four spaces and generated line breaks use LF. You can choose tabs or CRLF instead. With tabs, one tab is emitted per level regardless of the indentation width.

Formatting preserves the exact text of literals and comments, including escapes, numeric bases and type suffixes (`0x10`, `+2_i32`), raw-string delimiters, and line endings inside literals and comments. Multiline literals and comments keep their original internal indentation. Comments stay in their original order. A comment that starts on its own line in the input stays on its own line; a comment after a value on the same line stays there. Code after a line comment always moves to the next line. Formatting an already formatted document leaves it unchanged.

The formatter also checks that values in collections have compatible types, including nested collections:

- All elements of a list, all keys of a map, and all values of a map must have compatible types. Scalars must have the same type, including numeric type suffixes, so `[1 2_u8]` is rejected because `1` is an `i32`.
- Tuples must have the same length and compatible types at each position.
- Objects in the same collection may have different fields, but fields with the same name must have compatible types.
- Enumeration values in the same collection must have the same enumeration name. Different variants may have different payloads, but the same variant must always carry compatible payloads.
- An empty list or map is compatible with any list or map, because it does not provide type information.

The formatter cannot check application-specific defaults or the full definition of an enumeration without an application schema. When a value's type does not match the earlier members of its list, map keys, or map values, the error points at that value; inside nested collections, it points at the member of the innermost collection that conflicts.

The predefined enumerations `Option` and `Result` are always checked. `Option` has only `Some(value)` and `None`, and `Result` has only `Ok(value)` and `Err(error)`. `Some`, `Ok`, and `Err` take exactly one value, and `None` takes none. Both the full forms (`Option::Some(1)`) and the shorthands (`Some(1)`, `None`, `Ok(1)`, `Err("e")`) are accepted and kept as written. Names are case-sensitive, so `some(1)` and `Result::ok(1)` are rejected, as are new variants such as `Option::Foo`.

`true`, `false`, `NaN`, and `Inf` (including their type-suffixed forms such as `NaN_f32`) are reserved keywords. They cannot be used as object keys (`{NaN: 1}`), enumeration type names (`true::A`), or variant names (`Color::Inf`). The variant shorthand names `Some`, `None`, `Ok`, and `Err` are not reserved: they can be used as object keys (`{Some: Some(1)}`), enumeration type names (`Some::A`), and variant names (`Custom::Some(1)`).

## 2 The Rust library

### 2.1 Install the crate

Add the crate to `Cargo.toml`:

```toml
[dependencies]
ason-format = "1.5.0"  # Use the latest version of the crate
```

or run `cargo add ason-format`. API documentation is available on [docs.rs](https://docs.rs/ason-format).

### 2.2 Formatting

Format a document with the defaults (four spaces, LF), or set options explicitly:

```rust
use ason_format::{format, format_with_options, AsonError, FormatOptions, IndentStyle, LineEnding};

fn main() -> Result<(), AsonError> {
    let source = r#"{name: "Alice", values: [0xff, 0b10]}"#;

    let formatted = format(source)?;
    assert_eq!(formatted, "{name: \"Alice\" values: [0xff 0b10]}\n");

    let options = FormatOptions {
        indent_width: 2,
        indent_style: IndentStyle::Spaces,
        line_ending: LineEnding::CrLf,
    };
    let formatted = format_with_options(source, &options)?;
    println!("{formatted}");
    Ok(())
}
```

`FormatOptions` has three fields: `indent_width` (default `4`), `indent_style` (`Spaces` or `Tabs`), and `line_ending` (`Lf` or `CrLf`). Use `..FormatOptions::default()` to set only some of them.

`format` and `format_with_options` validate the document first (see [2.4 Validating](#24-validating)) and return an `AsonError` without partial output if it is invalid. A successful result always ends with a newline, and formatting it again returns the same text.

### 2.3 Highlighting

The library can return token ranges, ANSI-colored text, or escaped HTML:

```rust
use ason_format::{highlight_ansi, highlight_html, highlight_spans, AsonError};

fn main() -> Result<(), AsonError> {
    let source = "{count: 0xff}";

    for span in highlight_spans(source)? {
        println!("{:?}: {}", span.kind, &source[span.span]);
    }

    let terminal_text = highlight_ansi(source)?;
    print!("{terminal_text}");

    let html = highlight_html(source)?;
    assert!(html.starts_with("<pre class=\"ason\"><code>"));
    Ok(())
}
```

Highlighting does not format or validate the document. It only requires that the source can be split into tokens, so it also works on incomplete fragments such as `[1 2`. Unterminated strings or comments, and numbers outside the range of their type suffix (such as `256_u8`), return errors. To highlight formatted output, call `format` first and highlight its result.

- `highlight_spans` returns a `HighlightSpan` for every token except whitespace. Each span has a `kind` (`HighlightKind`) and a `span`, a half-open UTF-8 byte range that can be used to slice the source. A numeric sign and its number are separate spans.
- `highlight_ansi` wraps each token in an ANSI color sequence followed by a reset. It does not detect whether the output is a terminal, and it does not sanitize control characters in the input, so do not use it as a safety filter for untrusted text.
- `highlight_html` wraps the output in `<pre class="ason"><code>...</code></pre>` and each token in a `<span>` with a CSS class. All source text is HTML-escaped, but no stylesheet is included.

HTML token classes are `ason-number`, `ason-boolean`, `ason-char`, `ason-string`, `ason-datetime`, `ason-bytes`, `ason-identifier`, `ason-enumeration`, `ason-comment`, and `ason-punctuation`. `HighlightKind::css_class()` returns the class for each kind. A minimal stylesheet might look like:

```css
pre.ason .ason-number { color: #0086b3; }
pre.ason .ason-string, pre.ason .ason-char { color: #183691; }
pre.ason .ason-boolean, pre.ason .ason-enumeration { color: #a71d5d; }
pre.ason .ason-datetime, pre.ason .ason-bytes { color: #795da3; }
pre.ason .ason-identifier { color: #333333; }
pre.ason .ason-comment { color: #969896; font-style: italic; }
pre.ason .ason-punctuation { color: #666666; }
```

### 2.4 Validating

`validate_document` runs the same checks as `format` without producing output. Use it when you only need to know whether a document is valid, for example before loading a configuration file or in a CI check:

```rust
use ason_format::{validate_document, AsonError};

fn main() {
    let source = "{\n    list: [1 2\n        true]\n}";

    match validate_document(source) {
        Ok(()) => println!("The document is valid."),
        Err(AsonError::MessageWithRange(message, range)) => {
            // Lines and columns start at zero; add one for display.
            eprintln!(
                "{}:{}-{}:{}: {message}",
                range.start.line + 1,
                range.start.column + 1,
                range.end_inclusive.line + 1,
                range.end_inclusive.column + 1,
            );
        }
        Err(error) => eprintln!("{error}"),
    }
}
```

This prints `3:9-3:12: List element type does not match the earlier elements.`, because `true` does not match the earlier numbers in the list.

A document is valid when:

- it contains exactly one value, optionally surrounded by whitespace and comments (an empty document is invalid);
- its syntax and literal values are valid, for example no unterminated strings, duplicate object keys, out-of-range numbers such as `256_u8`, or reserved keywords used as names;
- values in each list, map key set, and map value set have compatible types, as described in [1.3 What formatting changes](#13-what-formatting-changes).

All functions in the crate return `AsonError`, re-exported from the [`ason`](https://crates.io/crates/ason) crate. It has four variants:

| Variant | Meaning |
|---------|---------|
| `Message(String)` | An error without a source location, such as an indentation that is too large to allocate |
| `UnexpectedEndOfDocument(String)` | The input ended inside an unfinished value, string, or comment |
| `MessageWithPosition(String, Position)` | An error at a single character |
| `MessageWithRange(String, Range)` | An error covering a range of source text, such as a value with a conflicting type |

A `Position` has `index` (a character index, not a byte offset), `line`, and `column`, all starting at zero. A `Range` has `start` and `end_inclusive` positions; the end position is the last character of the range. The `Display` implementation of `AsonError` prints the message with one-based line and column numbers, in the same form as `asonfmt`.

## 3 The WebAssembly package

The npm package includes prebuilt WebAssembly, JavaScript bindings, TypeScript declarations, build information, and dependency license notices. Applications using it do not need Rust or a local copy of this repository. It has no npm runtime dependencies.

### 3.1 Install the WebAssembly package

Install the prebuilt WebAssembly package from npm:

```sh
npm install --save @hemashushu/ason-format-wasm
```

### 3.2 Use the WebAssembly package

In Node.js, initialize the package with its matching `.wasm` file:

```js
import { readFile } from 'node:fs/promises';
import init, { formatDocument, validateDocument } from '@hemashushu/ason-format-wasm';

const wasm = await readFile(new URL(import.meta.resolve('@hemashushu/ason-format-wasm/ason_bg.wasm')));
await init({ module_or_path: wasm });

const formatted = JSON.parse(formatDocument('{answer:42}', 4, true, false));
// { status: 'success', text: '{answer: 42}\n' }

const validation = JSON.parse(validateDocument('[1 true]'));
// {
//   status: 'error',
//   error: {
//     message: 'List element type does not match the earlier elements.',
//     kind: 'syntax',
//     start: 3,
//     end: 7
//   }
// }
```

The package exports two functions; syntax highlighting is only available in the Rust library.

- `formatDocument(source, tabSize, insertSpaces, crlf)` validates and formats a document. `tabSize` sets the indentation width, `insertSpaces` selects spaces rather than tabs, and `crlf` selects CRLF rather than LF for generated line breaks. With tabs, one tab is emitted per level and `tabSize` only affects whether content fits on one line.
- `validateDocument(source)` checks a document without formatting it.

Both functions return a JSON string and never throw for invalid documents:

- On success, `status` is `"success"`. `text` is the formatted document for `formatDocument`, and `null` for `validateDocument`.
- On failure, `status` is `"error"`, and `error` has `message`, `kind`, `start`, and `end`. `start` and `end` are UTF-16 offsets (the same units as JavaScript string indexes) with an exclusive end, so `source.slice(error.start, error.end)` returns the offending text. They are `null` when the location is unknown.
- `kind` is `"syntax"` for errors with a source location, including type mismatches and reserved names; `"eof"` for an unexpected end of the document, with both offsets at the end of the source; and `"document"` for errors without a source location.

For browsers, configure your bundler to include `ason_bg.wasm` and pass its URL, bytes, or compiled `WebAssembly.Module` to `init`. Keep the JavaScript bindings and WebAssembly file on the same package version.

### 3.3 Build and publish

This section is for maintainers. Building the npm package requires Node.js 24 or later, Rust (CI uses 1.99.1), the `wasm32-unknown-unknown` target, and `wasm-bindgen-cli` 0.2.130 (the build script requires this exact version):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
npm ci
npm run build
npm test
npm pack
```

The build checks that the npm and Rust package versions match, then writes generated files to `pkg/`, along with `pkg/build-info.json` (tool versions and file checksums) and `THIRD_PARTY_NOTICES.md`. Tests exercise the WebAssembly API and verify that the packed npm package works without Rust sources. `npm pack` runs the build and tests before creating the archive. The Rust `ason-format-wasm` crate is an internal build component and is not published to crates.io.

On Windows, install Visual Studio's **Desktop development with C++** workload and **C++ Clang tools for Windows**, then run the Cargo installation from **Developer PowerShell for VS**. Make sure Cargo's binary directory is on `PATH` if `wasm-bindgen` is not found.

The repository has three GitHub Actions workflows:

- **Test Rust library and WASM** (`test.yml`) runs on every push and pull request to `main`. It checks formatting, builds, tests, and runs Clippy on the Rust workspace, then builds and tests the npm package.
- **Publish to crates.io** (`publish-crates.yml`) is started manually. It tests and packages the workspace, then publishes `ason-format` and `asonfmt` using crates.io trusted publishing, skipping any crate version that is already published.
- **Publish WASM to npmjs.com** (`publish-wasm.yml`) is started manually. It rebuilds and tests the package, then stages it using npm trusted publishing. Configure `hemashushu/ason-formatter` and `publish-wasm.yml` as the package's trusted publisher on npm, with **npm stage publish** enabled under **Allowed actions**. After the workflow succeeds, a package maintainer must review and approve the staged version on npmjs.com or with `npm stage approve <stage-id>` using 2FA before it becomes available.

For each release, update the versions in all three `Cargo.toml` files and `package.json` together; published versions cannot be overwritten.

If the npm package is not yet available, consumers can use the `.tgz` created by `npm pack`. Keep the archive in the consuming project and reference it with a `file:` dependency and npm lockfile.

## 4 More information

- [ASON specification](https://github.com/hemashushu/ason/blob/main/docs/specification.md)
- [ASON Converter (ason2b, ason2j)](https://github.com/hemashushu/ason-converter)

## 5 License

This project is distributed under the Mozilla Public License 2.0 with additional terms. See [LICENSE](https://github.com/hemashushu/ason-formatter/blob/main/LICENSE) and [LICENSE.additional](https://github.com/hemashushu/ason-formatter/blob/main/LICENSE.additional).
