# ASON Formatter

Source-preserving formatting and syntax highlighting for [ASON](https://github.com/hemashushu/ason).

The workspace contains the `ason-format` Rust library and the `asonfmt` executable (built by the `ason-format-cli` crate).



## Build

```sh
cargo build --workspace
cargo test --workspace
```

Development currently uses a local ASON dependency at `/Users/yang/projects/ason`.
That checkout must include the new `ason::syntax` API. Both the path dependency
and reference-example tests use this location; this setup is not yet portable
or ready for publishing to crates.io. The projects use Rust edition 2024.

## Formatting

```rust
use ason_format::{FormatOptions, format, format_with_options};

let source = r#"{name: "Alice", values: [0xff, 0b10]}"#;
let formatted = format(source)?;
let two_spaces = format_with_options(source, &FormatOptions { indent_width: 2 })?;
# Ok::<(), ason_format::AsonError>(())
```

Default output:

```text
{
    name: "Alice"
    values: [
        0xff
        0b10
    ]
}
```

Formatting validates exactly one root value, then applies these rules:

- Four spaces per level by default; zero indentation still emits line breaks.
- Nonempty containers and enumeration payloads span multiple lines.
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

## Highlighting

```rust
use ason_format::{highlight_spans, highlight_ansi, highlight_html};

let source = "{count: 0xff} // unchanged source\n";
let spans = highlight_spans(source)?;
let terminal_text = highlight_ansi(source)?;
let html = highlight_html(source)?;
# Ok::<(), ason_format::AsonError>(())
```

- `highlight_spans` returns ordered `HighlightSpan { kind, span }` values, where
  `span` is a half-open **UTF-8 byte range** into the input. Whitespace is omitted.
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

## CLI

```sh
# stdin to stdout
printf '{answer: 42}' | cargo run -q -p ason-format-cli --

# File to stdout; colors are automatic when stdout is a terminal
cargo run -q -p ason-format-cli -- document.ason

# File to file; always plain text
cargo run -q -p ason-format-cli -- document.ason -o formatted.ason

# Explicit standard streams and layout/color settings
cat document.ason | cargo run -q -p ason-format-cli -- - -o - --indent-width 2 --color never
```

`INPUT` is optional; omit it or pass `-` for stdin. `-o/--output FILE` defaults to
stdout; `-o -` also selects stdout. `--color auto|always|never` defaults to `auto`.
Automatic color requires terminal stdout, no `NO_COLOR` environment variable,
and `TERM` other than `dumb`. Explicit `always` or `never` overrides those
environment settings, but actual output files are never colored.

There is no in-place mode. Input/output aliases, including Unix symbolic links
and hard links, are rejected before truncation. Invalid input never truncates an
existing output file. Normal file output is not an atomic replacement, so a
write failure can leave partial output. Diagnostics go to stderr; exit status is
1 for document/I/O errors and 2 for invalid arguments. A closed stdout pipe is
treated as successful termination.

## Verification

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --manifest-path /Users/yang/projects/ason/Cargo.toml
```

Tests cover source preservation, semantic equivalence, idempotence, reference
examples, Unicode byte ranges, HTML escaping, ANSI round trips, CLI streams,
color policy, and output-file protection.