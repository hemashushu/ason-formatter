# ASON Formatter

ASON Formatter provides three ways to work with [ASON](https://github.com/hemashushu/ason): the `asonfmt` command-line tool, the `ason-format` Rust library, and the `@hemashushu/ason-format-wasm` npm package.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [Install](#install)
- [Format from the command line](#format-from-the-command-line)
- [What formatting changes](#what-formatting-changes)
- [Use the Rust library](#use-the-rust-library)
- [Highlight ASON in Rust](#highlight-ason-in-rust)
- [Use the WebAssembly package](#use-the-webassembly-package)
- [Build and publish the WebAssembly package](#build-and-publish-the-webassembly-package)
- [More information](#more-information)
- [License](#license)

<!-- /code_chunk_output -->


## Install

Install the command-line tool with Cargo:

```sh
cargo install asonfmt
```

Or install the prebuilt WebAssembly package from npm:

```sh
npm install --save-exact @hemashushu/ason-format-wasm@1.2.0
```

## Format from the command line

`asonfmt` reads a file or standard input and writes the result to standard output:

```sh
asonfmt input.ason
cat input.ason | asonfmt
```

Write to another file or replace the input file:

```sh
asonfmt input.ason --output formatted.ason
asonfmt input.ason --replace-atomic
```

Common options:

- `--indent-width 2` uses two spaces per level. With `--indent-style tabs`, it sets the visual width of each tab for line fitting; the formatter still emits one tab per level.
- `--line-ending crlf` uses CRLF for generated line breaks. The default is LF.
- `--color auto|always|never` controls terminal colors. `auto` is the default; output files are never colored.
- `--help` lists all options.

For detailed command-line documentation, see [`crates/asonfmt/docs/asonfmt.txt`](crates/asonfmt/docs/asonfmt.txt).

The tool returns `0` on success, `1` for invalid arguments, `2` for invalid documents, and `3` for input/output errors. A broken output pipe is treated as normal pipeline termination.

## What formatting changes

The formatter checks that the input contains exactly one root value, then lays it out consistently. Nonempty containers and enumeration payloads stay on one line when they fit within 80 columns; otherwise, they use multiple lines. Empty lists and objects stay compact unless they contain comments. A space follows each colon, optional commas and extra blank lines are removed, and the output ends with a newline.

By default, indentation is four spaces and generated line breaks use LF. You can choose tabs or CRLF instead. With tabs, `indent_width` controls how wide a tab counts when deciding whether content fits on one line; it does not control how many tab characters are emitted. A width of zero still emits indentation.

Formatting preserves literal spelling and comment text, including escapes, numeric bases, raw-string delimiters, and line endings inside literals and comments. Multiline literals and comments keep their original internal indentation. Comments stay in their original order; line comments end their line.

The formatter also checks that values in collections have compatible types, including nested collections. Missing object fields and empty collections are allowed when other values provide enough type information. The formatter cannot check application-specific defaults or the full definition of an enumeration without an application schema.

## Use the Rust library

Add the crate to `Cargo.toml`:

```toml
[dependencies]
ason-format = "1.2.0"
```

Format a document with the defaults or set options explicitly:

```rust
use ason_format::{format, format_with_options, FormatOptions};

let source = r#"{name: "Alice", values: [0xff, 0b10]}"#;
let formatted = format(source)?;
let compact_indent = format_with_options(source, &FormatOptions {
    indent_width: 2,
    ..FormatOptions::default()
})?;
# Ok::<(), ason_format::AsonError>(())
```

`format` and `format_with_options` return an error for invalid input. See the [API documentation on docs.rs](https://docs.rs/ason-format/1.2.0/ason_format/) for the full reference.

## Highlight ASON in Rust

The library can return token ranges, ANSI-colored text, or escaped HTML:

```rust
use ason_format::{highlight_ansi, highlight_html, highlight_spans};

let source = "{count: 0xff}";
let spans = highlight_spans(source)?;
let terminal_text = highlight_ansi(source)?;
let html = highlight_html(source)?;
# Ok::<(), ason_format::AsonError>(())
```

Token ranges are half-open UTF-8 byte ranges, and whitespace is not included. HTML output is wrapped in `<pre class="ason"><code>...</code></pre>` and escapes the source text; it does not include CSS. ANSI output does not detect terminals or sanitize control characters in the input. Highlighting needs valid tokens, but not a complete document; unterminated strings and comments return errors.

HTML token classes are `ason-number`, `ason-boolean`, `ason-char`, `ason-string`, `ason-datetime`, `ason-bytes`, `ason-identifier`, `ason-enumeration`, `ason-comment`, and `ason-punctuation`. `HighlightKind::css_class()` returns the class for each token kind.

## Use the WebAssembly package

The npm package includes prebuilt WebAssembly, JavaScript bindings, TypeScript declarations, build information, and dependency license notices. Applications using it do not need Rust or a local copy of this repository. It has no npm runtime dependencies.

In Node.js, initialize the package with its matching `.wasm` file:

```js
import { readFile } from 'node:fs/promises';
import init, { formatDocument, validateDocument } from '@hemashushu/ason-format-wasm';

const wasm = await readFile(new URL(import.meta.resolve('@hemashushu/ason-format-wasm/ason_bg.wasm')));
await init({ module_or_path: wasm });

const formatted = JSON.parse(formatDocument('{answer:42}', 4, true, false));
const validation = JSON.parse(validateDocument('{answer:42}'));
```

`formatDocument(source, tabSize, insertSpaces, crlf)` formats a document. `tabSize` sets the indentation width, `insertSpaces` selects spaces rather than tabs, and `crlf` selects CRLF rather than LF for generated line breaks. With tabs, one tab is emitted per level and `tabSize` is used for line fitting. `validateDocument(source)` checks a document without formatting it.

Both functions return a JSON string. Successful formatting returns a `success` status and the formatted text; successful validation returns a `success` status with `text: null`. Errors include a message, a kind (`syntax`, `eof`, or `document`), and, when available, a source range. Range offsets are UTF-16 offsets with an exclusive end.

For browsers, configure your bundler to include `ason_bg.wasm` and pass its URL, bytes, or compiled `WebAssembly.Module` to `init`. Keep the JavaScript bindings and WebAssembly file on the same package version.

## Build and publish the WebAssembly package

This section is for maintainers. Building the npm package requires Node.js 24 or later, Rust (tested with 1.99.1), the `wasm32-unknown-unknown` target, and `wasm-bindgen-cli` 0.2.130:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
npm ci
npm run build
npm test
npm pack
```

The build checks that the npm and Rust package versions match, then writes generated files to `pkg/`. Tests exercise the WebAssembly API and verify that the packed npm package works without Rust sources. `npm pack` runs the build and tests before creating the archive. The Rust `ason-format-wasm` crate is an internal build component and is not published to crates.io.

On Windows, install Visual Studio's **Desktop development with C++** workload and **C++ Clang tools for Windows**, then run the Cargo installation from **Developer PowerShell for VS**. Make sure Cargo's binary directory is on `PATH` if `wasm-bindgen` is not found.

The **Rust** workflow checks and tests the package on Ubuntu. The manual **Publish WASM to npm** workflow rebuilds and tests the package on Ubuntu, then publishes it using npm trusted publishing. Before the first release, the package owner must publish the tested archive with `npm publish <archive.tgz> --access public`, then configure `hemashushu/ason-formatter` and `publish-wasm.yml` as the package's trusted publisher on npm. For later releases, update the npm and Rust crate versions together; published versions cannot be overwritten.

If the npm package is not yet available, consumers can use the `.tgz` created by `npm pack`. Keep the archive in the consuming project and reference it with a `file:` dependency and npm lockfile.

## More information

- [ASON specification](https://github.com/hemashushu/ason/blob/main/docs/specification.md)
- [ASON Converter](https://github.com/hemashushu/ason-converter)
- [Contributing guide](CONTRIBUTING)

## License

This project is distributed under the Mozilla Public License 2.0 with additional terms. See [LICENSE](LICENSE) and [LICENSE.additional](LICENSE.additional).