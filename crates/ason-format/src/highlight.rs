// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Classify source tokens, then render them without changing the original text.
//!
//! Both renderers copy gaps between tokens as well as the tokens themselves. This
//! keeps whitespace intact and avoids reconstructing literals from parsed values.

use std::ops::Range;

use ason::syntax::{SyntaxKind, tokenize};

use crate::AsonError;

/// A visual category shared by the terminal and HTML renderers.
///
/// Categories describe lexical tokens, not the meaning of a parsed value. For
/// example, a numeric sign has its own token but uses the number category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    /// A numeric literal or its leading sign.
    Number,
    /// A `true` or `false` literal.
    Boolean,
    /// A single-character literal, including its quotes and escapes.
    Char,
    /// A string literal, including ordinary, raw, and multiline forms.
    String,
    /// A date/time literal, including its prefix and quotes.
    DateTime,
    /// A byte-data literal, including its prefix and quotes.
    Bytes,
    /// A bare identifier, such as an object field name.
    Identifier,
    /// A qualified enumeration name such as `Color::Red`.
    Enumeration,
    /// A line comment or a complete block comment.
    Comment,
    /// Structural syntax, including delimiters, colons, and commas.
    Punctuation,
}

impl HighlightKind {
    /// Returns the stable CSS class used for this category in HTML output.
    ///
    /// # Arguments
    ///
    /// * `self` - The category to translate into an `ason-*` class name.
    ///
    /// The result is a static string with no leading dot. Use it in a `class`
    /// attribute, or add a dot when constructing a stylesheet selector.
    ///
    /// # Examples
    ///
    /// ```
    /// use ason_format::HighlightKind;
    ///
    /// assert_eq!(HighlightKind::Number.css_class(), "ason-number");
    /// ```
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Number => "ason-number",
            Self::Boolean => "ason-boolean",
            Self::Char => "ason-char",
            Self::String => "ason-string",
            Self::DateTime => "ason-datetime",
            Self::Bytes => "ason-bytes",
            Self::Identifier => "ason-identifier",
            Self::Enumeration => "ason-enumeration",
            Self::Comment => "ason-comment",
            Self::Punctuation => "ason-punctuation",
        }
    }

    /// Returns the opening ANSI color sequence; callers must add a reset later.
    fn ansi(self) -> &'static str {
        match self {
            Self::Number => "\x1b[36m",
            Self::Boolean => "\x1b[33m",
            Self::Char | Self::String => "\x1b[32m",
            Self::DateTime | Self::Bytes => "\x1b[35m",
            Self::Identifier => "\x1b[34m",
            Self::Enumeration => "\x1b[1;33m",
            Self::Comment => "\x1b[90m",
            Self::Punctuation => "\x1b[37m",
        }
    }
}

/// One highlighted region in the original UTF-8 source.
///
/// A span refers to bytes, not character positions or display columns. Slice the
/// same source string that was passed to [`highlight_spans`] to retrieve its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// The visual category to apply to the text covered by `span`.
    pub kind: HighlightKind,
    /// A half-open UTF-8 byte range in the unchanged input.
    ///
    /// The start is included and the end is excluded. Returned ranges fall on
    /// character boundaries, so they can be used directly for string slicing.
    pub span: Range<usize>,
}

/// Returns ordered, non-overlapping highlight ranges, excluding whitespace.
///
/// # Arguments
///
/// * `source` - The unchanged ASON text whose tokens should be classified. It can
///   be a complete document or a structurally incomplete fragment.
///
/// Whitespace has no span of its own. Comments and punctuation do, so a renderer
/// can style them without having to infer their locations. No formatting or
/// semantic type checking takes place.
///
/// # Errors
///
/// Returns the tokenizer's error for invalid lexical input, such as an unfinished
/// string. Unclosed containers alone are allowed because no parser is run.
///
/// # Examples
///
/// ```
/// use ason_format::{HighlightKind, highlight_spans};
///
/// let source = "[42";
/// let spans = highlight_spans(source)?;
/// assert_eq!(spans[1].kind, HighlightKind::Number);
/// assert_eq!(&source[spans[1].span.clone()], "42");
/// # Ok::<(), ason_format::AsonError>(())
/// ```
pub fn highlight_spans(source: &str) -> Result<Vec<HighlightSpan>, AsonError> {
    Ok(tokenize(source)?
        .into_iter()
        .filter_map(|token| {
            let kind = match token.kind {
                SyntaxKind::Whitespace => return None,
                SyntaxKind::Number | SyntaxKind::Sign => HighlightKind::Number,
                SyntaxKind::Boolean => HighlightKind::Boolean,
                SyntaxKind::Char => HighlightKind::Char,
                SyntaxKind::String => HighlightKind::String,
                SyntaxKind::DateTime => HighlightKind::DateTime,
                SyntaxKind::Bytes => HighlightKind::Bytes,
                SyntaxKind::Identifier => HighlightKind::Identifier,
                SyntaxKind::Enumeration => HighlightKind::Enumeration,
                SyntaxKind::LineComment | SyntaxKind::BlockComment => HighlightKind::Comment,
                _ => HighlightKind::Punctuation,
            };
            Some(HighlightSpan {
                kind,
                span: token.span,
            })
        })
        .collect())
}

/// Inserts ANSI SGR colors without formatting the source or detecting a terminal.
///
/// # Arguments
///
/// * `source` - The ASON text to color. Token text, whitespace, and line endings
///   are copied as supplied; the function only inserts color and reset sequences.
///
/// Each token gets its own color sequence followed by an SGR reset. The caller
/// decides whether the destination supports colors. Existing control characters
/// in the input are preserved, not sanitized, so this is not a terminal-safety
/// filter for untrusted text.
///
/// # Errors
///
/// Returns a lexical error from [`highlight_spans`]. Structural completeness and
/// semantic validity are not checked.
///
/// # Examples
///
/// ```
/// use ason_format::highlight_ansi;
///
/// let colored = highlight_ansi("42\n")?;
/// assert_eq!(colored, "\x1b[36m42\x1b[0m\n");
/// # Ok::<(), ason_format::AsonError>(())
/// ```
pub fn highlight_ansi(source: &str) -> Result<String, AsonError> {
    let mut output = String::new();
    let mut previous_end = 0;
    for token in highlight_spans(source)? {
        // Spans omit whitespace. Copy the gap before adding color to this token.
        output.push_str(&source[previous_end..token.span.start]);
        output.push_str(token.kind.ansi());
        output.push_str(&source[token.span.clone()]);
        output.push_str("\x1b[0m");
        previous_end = token.span.end;
    }
    output.push_str(&source[previous_end..]);
    Ok(output)
}

/// Renders escaped source inside `<pre class="ason"><code>`, using `ason-*` classes.
///
/// # Arguments
///
/// * `source` - The ASON text to display. All source text, including comments and
///   whitespace between tokens, is HTML-escaped before it enters the fragment.
///
/// The returned fragment supplies markup, not a stylesheet or a complete HTML
/// page. Define styles for the classes from [`HighlightKind::css_class`] in the
/// containing page. The source cannot introduce tags through this renderer.
///
/// # Errors
///
/// Returns a lexical error from [`highlight_spans`]. Like the other highlighters,
/// this function accepts structurally incomplete but tokenizable input.
///
/// # Examples
///
/// ```
/// use ason_format::highlight_html;
///
/// let html = highlight_html("42")?;
/// assert_eq!(html,
///     "<pre class=\"ason\"><code><span class=\"ason-number\">42</span></code></pre>");
/// # Ok::<(), ason_format::AsonError>(())
/// ```
pub fn highlight_html(source: &str) -> Result<String, AsonError> {
    let mut output = String::from("<pre class=\"ason\"><code>");
    let mut previous_end = 0;
    for token in highlight_spans(source)? {
        escape_html(&source[previous_end..token.span.start], &mut output);
        output.push_str("<span class=\"");
        output.push_str(token.kind.css_class());
        output.push_str("\">");
        escape_html(&source[token.span.clone()], &mut output);
        output.push_str("</span>");
        previous_end = token.span.end;
    }
    escape_html(&source[previous_end..], &mut output);
    output.push_str("</code></pre>");
    Ok(output)
}

/// Appends `source` to `output`, escaping characters that have meaning in HTML.
///
/// This handles source gaps as well as token bodies. Iterating over characters
/// leaves non-ASCII text intact instead of treating UTF-8 bytes as characters.
fn escape_html(source: &str, output: &mut String) {
    for character in source.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(character),
        }
    }
}

#[cfg(test)]
/// Checks token boundaries and verifies that rendering preserves source text.
mod tests {
    use super::*;

    #[test]
    /// Checks every literal category and byte-safe slicing of multilingual input.
    fn utf8_classification() {
        let source = "{中文: r#\"// not a comment 中\"#, value: -0xff, yes: true, kind: X::Y, date: d\"2024-03-16\", bytes: h\"48\", ch: '中'} /* comment */";
        let spans = highlight_spans(source).unwrap();
        let mut end = 0;
        for token in &spans {
            assert!(token.span.start >= end);
            assert!(source.get(token.span.clone()).is_some());
            end = token.span.end;
        }
        assert_eq!(
            spans
                .iter()
                .filter(|token| token.kind == HighlightKind::Comment)
                .count(),
            1
        );
        assert!(
            spans
                .iter()
                .any(|token| token.kind == HighlightKind::Identifier
                    && &source[token.span.clone()] == "中文")
        );
        for kind in [
            HighlightKind::Number,
            HighlightKind::Boolean,
            HighlightKind::Char,
            HighlightKind::String,
            HighlightKind::DateTime,
            HighlightKind::Bytes,
            HighlightKind::Enumeration,
        ] {
            assert!(spans.iter().any(|token| token.kind == kind));
        }
    }

    #[test]
    /// Removing the generated ANSI sequences must recover the exact input.
    fn ansi_round_trip() {
        let source = " // 中\r\n{key: r\"/*hi*/\", n: -1}\n";
        let ansi = highlight_ansi(source).unwrap();
        let mut plain = String::new();
        let mut chars = ansi.chars();
        // This fixture contains no source escapes, so every ESC belongs to us.
        while let Some(character) = chars.next() {
            if character == '\x1b' {
                assert_eq!(chars.next(), Some('['));
                for character in chars.by_ref() {
                    if character == 'm' {
                        break;
                    }
                }
            } else {
                plain.push(character);
            }
        }
        assert_eq!(plain, source);
        assert!(ansi.ends_with("\x1b[0m\n"));
    }

    #[test]
    /// Source resembling HTML must stay text, including inside comments.
    fn html_escapes_all_source() {
        let source = "\"<script>&'\\\"</script>\" // <img src=x>\n";
        let html = highlight_html(source).unwrap();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;script&gt;&amp;&#39;\\&quot;"));
        assert!(html.contains("class=\"ason-comment\""));
        let mut text = String::new();
        let mut in_tag = false;
        for character in html.chars() {
            match character {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => text.push(character),
                _ => {}
            }
        }
        // Decode ampersands last so an escaped entity is not decoded twice.
        let decoded = text
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&");
        assert_eq!(decoded, source);
    }

    #[test]
    /// Distinguishes lexical errors from incomplete syntax and covers empty input.
    fn lexical_not_structural_validation() {
        assert!(highlight_spans("{key: 1").is_ok());
        assert!(highlight_spans("\"unterminated").is_err());
        assert_eq!(highlight_ansi(" \n").unwrap(), " \n");
        assert_eq!(
            highlight_html("").unwrap(),
            "<pre class=\"ason\"><code></code></pre>"
        );
    }
}
