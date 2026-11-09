// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Source-preserving formatting and syntax highlighting for ASON documents.
//!
//! Use [`format()`] for the default layout or [`format_with_options`] to configure
//! indentation and line endings. Formatting validates a complete document, including
//! nested collection types, but keeps literal spellings and comment bodies unchanged.
//! [`validate_document`] checks syntax and types without formatting.
//!
//! Highlighting is a separate operation: [`highlight_spans`] classifies tokens,
//! [`highlight_ansi`] adds terminal colors, and [`highlight_html`] produces an
//! escaped HTML fragment. These functions only require tokenizable input, so they
//! can also display fragments whose containers are not yet closed. They do not
//! format the source; call a formatter first when both operations are wanted.
//!
//! # Example
//!
//! ```
//! use ason_format::{format, highlight_html};
//!
//! let formatted = format("{enabled:true}")?;
//! assert_eq!(formatted, "{enabled: true}\n");
//! let html = highlight_html(&formatted)?;
//! assert!(html.contains("class=\"ason-boolean\">true</span>"));
//! # Ok::<(), ason_format::AsonError>(())
//! ```
//!
//! # Errors
//!
//! Fallible operations return [`AsonError`], re-exported from the ASON parser so
//! callers can handle parser, tokenizer, and formatter errors through one type.

mod format;
mod highlight;

pub use ason::AsonError;
pub use format::{
    FormatOptions, IndentStyle, LineEnding, format, format_with_options, validate_document,
};
pub use highlight::{
    HighlightKind, HighlightSpan, highlight_ansi, highlight_html, highlight_spans,
};
