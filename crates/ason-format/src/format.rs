// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Validate document values, plan structural spacing, and copy original tokens.
//!
//! Parsing and rendering serve different purposes here. Parsed values let us
//! check nested collection types, but serializing those values would lose the
//! spelling of literals and all comments. Instead, a second pass lays out source
//! tokens and a final pass copies their exact text into the output.

use ason::syntax::{SyntaxKind, SyntaxToken, tokenize};

use crate::AsonError;

const MAX_LINE_WIDTH: usize = 80;

/// Controls the whitespace added around tokens during formatting.
///
/// Literal and comment contents are copied verbatim, even when they contain
/// multiple lines. These settings affect surrounding layout, not their contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// The number of spaces per nesting level. With tabs, one tab is emitted per
    /// level and this value is the tab's visual width for line-fit calculations;
    /// it does not control the number of tab characters emitted.
    ///
    /// The default is four. A width of zero removes space indentation, but tabs
    /// are still emitted once per level; zero only affects line-fit calculations
    /// in that mode. A width that cannot be represented or reserved when writing
    /// an indented token makes formatting return an error.
    pub indent_width: usize,
    pub indent_style: IndentStyle,
    pub line_ending: LineEnding,
}

impl Default for FormatOptions {
    /// Uses four spaces per level and keeps containers on one line when they fit.
    fn default() -> Self {
        Self {
            indent_width: 4,
            indent_style: IndentStyle::Spaces,
            line_ending: LineEnding::Lf,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    Spaces,
    Tabs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

pub fn validate_document(source: &str) -> Result<(), AsonError> {
    shape(&ason::de_value_from_str(source)?)?;
    Ok(())
}

/// Formats one complete ASON document with four spaces per nesting level.
///
/// # Arguments
///
/// * `source` - UTF-8 text containing exactly one ASON value, with optional
///   surrounding whitespace and comments.
///
/// Containers without comments or multiline tokens stay on one line when the
/// result fits within 80 characters; otherwise, nonempty containers use
/// multiple lines. Optional commas are removed, and the result ends with a
/// newline. Literal spellings and comment bodies are kept exactly as written.
/// Formatting an already formatted result leaves it unchanged.
///
/// # Errors
///
/// Returns an error for invalid syntax, invalid values, or inconsistent element
/// types in lists or map keys/values. See [`format_with_options`] for custom layout.
///
/// # Examples
///
/// ```
/// use ason_format::format;
///
/// assert_eq!(format("{answer:0x2a}")?, "{answer: 0x2a}\n");
/// # Ok::<(), ason_format::AsonError>(())
/// ```
pub fn format(source: &str) -> Result<String, AsonError> {
    format_with_options(source, &FormatOptions::default())
}

/// Validates and formats a document with the supplied layout settings.
///
/// # Arguments
///
/// * `source` - UTF-8 text containing exactly one ASON value. Comments are allowed;
///   an empty document or a document containing only comments is not a value.
/// * `options` - Layout settings borrowed for this call. `indent_width` controls
///   spaces per nesting level or the visual width of tabs when using tab indentation.
///
/// The formatter copies each literal and comment from the original source rather
/// than serializing parsed values. Numeric bases, escape sequences, raw-string
/// delimiters, and whitespace inside multiline tokens therefore stay intact.
/// Whitespace between tokens and optional commas are replaced by the chosen
/// layout. A final newline is always appended.
///
/// # Errors
///
/// Returns parsing or tokenization errors, type-consistency errors in nested
/// collections, or an error if an indentation size cannot be represented or
/// reserved. No partial output is returned on failure.
///
/// # Examples
///
/// ```
/// use ason_format::{FormatOptions, format_with_options};
///
/// let options = FormatOptions {
///     indent_width: 2,
///     ..FormatOptions::default()
/// };
/// assert_eq!(format_with_options("[1,2]", &options)?, "[1 2]\n");
/// # Ok::<(), ason_format::AsonError>(())
/// ```
pub fn format_with_options(source: &str, options: &FormatOptions) -> Result<String, AsonError> {
    validate_document(source)?;
    let tokens: Vec<_> = tokenize(source)?
        .into_iter()
        .filter(|token| !matches!(token.kind, SyntaxKind::Whitespace | SyntaxKind::Comma))
        .collect();
    // Comments do not participate in the grammar walk. Keep them in `tokens`
    // so the writer can restore them without disturbing structural decisions.
    let comments: Vec<_> = tokens
        .iter()
        .filter(|token| is_comment(token.kind))
        .collect();
    let semantic: Vec<_> = tokens
        .iter()
        .filter(|token| !is_comment(token.kind))
        .collect();
    let mut layout = Layout {
        source,
        tokens: &semantic,
        comments: &comments,
        indent_width: options.indent_width,
        cursor: 0,
        placements: Vec::new(),
    };
    layout.value(0, Gap::None)?;
    // There is exactly one placement per non-comment token. The replay below
    // consumes those placements in order, inserting comments between them.
    let mut placements = layout.placements.into_iter();
    let mut writer = Writer {
        output: String::new(),
        indent_width: options.indent_width,
        indent_style: options.indent_style,
        line_ending: options.line_ending,
    };
    let mut previous: Option<&SyntaxToken> = None;
    let mut depth = 0;
    for token in &tokens {
        let comment = is_comment(token.kind);
        // Comments use the surrounding nesting depth. A source newline keeps a
        // comment on its own line; otherwise a block comment can remain inline.
        let (mut gap, token_depth) = if comment {
            let gap = match previous {
                None => Gap::None,
                Some(previous)
                    if is_open(previous.kind)
                        || source[previous.span.end..token.span.start].contains('\n')
                        || previous.kind == SyntaxKind::LineComment =>
                {
                    Gap::Line
                }
                Some(_) => Gap::Space,
            };
            (gap, depth)
        } else {
            placements.next().ok_or_else(layout_error)?
        };
        // A line comment must never swallow the next token. Block comments also
        // retain an existing following newline, and closers get their own line.
        if let Some(previous) = previous {
            if previous.kind == SyntaxKind::LineComment
                || (is_comment(previous.kind)
                    && (source[previous.span.end..token.span.start].contains('\n')
                        || matches!(
                            token.kind,
                            SyntaxKind::BraceClose
                                | SyntaxKind::BracketClose
                                | SyntaxKind::ParenthesisClose
                        )))
            {
                gap = Gap::Line;
            } else if is_comment(previous.kind) && gap == Gap::None {
                gap = Gap::Space;
            }
        }
        writer.write(&source[token.span.clone()], gap, token_depth)?;
        // After an opener, comments belong one level inside the container.
        // A closer's placement already carries the parent's depth.
        if !comment {
            depth = if is_open(token.kind) {
                token_depth + 1
            } else {
                token_depth
            };
        }
        previous = Some(token);
    }
    writer.output.push_str(options.line_ending.as_str());
    Ok(writer.output)
}

/// Identifies tokens that must be preserved but skipped during structural layout.
fn is_comment(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::LineComment | SyntaxKind::BlockComment)
}

/// Identifies delimiters that introduce a nested container or variant payload.
fn is_open(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BraceOpen | SyntaxKind::BracketOpen | SyntaxKind::ParenthesisOpen
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// Whitespace to insert immediately before a token, outside its source text.
enum Gap {
    /// Keep adjacent tokens touching, as in a sign followed by its number.
    None,
    /// Separate tokens with one space on the same line.
    Space,
    /// Start a new line and indent to the token's planned nesting depth.
    Line,
}

/// Reports an unexpected mismatch between valid syntax and the layout walker.
fn layout_error() -> AsonError {
    AsonError::Message("Unexpected token while laying out the document.".into())
}

/// A grammar walk that records spacing without rendering or rewriting tokens.
///
/// The input has no whitespace, commas, or comments. Each call to `consume`
/// advances one token and appends one placement, keeping both streams aligned.
struct Layout<'a> {
    /// Original text used to measure the single-line form of containers.
    source: &'a str,
    /// Borrowed structural tokens in source order; the source itself is unchanged.
    tokens: &'a [&'a SyntaxToken],
    /// Comments excluded from the grammar walk but used to protect comment layout.
    comments: &'a [&'a SyntaxToken],
    /// Spaces per nesting level, used when measuring line width.
    indent_width: usize,
    /// Index of the next token that has not yet received a placement.
    cursor: usize,
    /// Leading whitespace and nesting depth for each consumed token, in order.
    placements: Vec<(Gap, usize)>,
}

impl Layout<'_> {
    /// Peeks at the next token, returning `None` when the stream is exhausted.
    fn kind(&self) -> Option<SyntaxKind> {
        self.tokens.get(self.cursor).map(|token| token.kind)
    }

    /// Assigns `gap` and `depth` to the current token, then advances the cursor.
    fn consume(&mut self, depth: usize, gap: Gap) {
        self.placements.push((gap, depth));
        self.cursor += 1;
    }

    /// Plans one value at `depth`, using `gap` only before its first token.
    ///
    /// Signs and enumeration names can prefix another token or a whole payload.
    /// Recursing with no gap keeps those pieces attached without special cases
    /// in the writer. Syntax was validated earlier, so this is not a full parser.
    fn value(&mut self, depth: usize, gap: Gap) -> Result<(), AsonError> {
        match self.kind().ok_or_else(layout_error)? {
            SyntaxKind::Sign => {
                self.consume(depth, gap);
                self.value(depth, Gap::None)?;
            }
            SyntaxKind::Enumeration => {
                self.consume(depth, gap);
                if matches!(
                    self.kind(),
                    Some(SyntaxKind::ParenthesisOpen | SyntaxKind::BraceOpen)
                ) {
                    self.container(depth, Gap::None)?;
                }
            }
            kind if is_open(kind) => self.container(depth, gap)?,
            SyntaxKind::Number
            | SyntaxKind::Boolean
            | SyntaxKind::Char
            | SyntaxKind::String
            | SyntaxKind::DateTime
            | SyntaxKind::Bytes => self.consume(depth, gap),
            _ => return Err(layout_error()),
        }
        Ok(())
    }

    /// Plans a delimited container, indenting entries one level below `depth`.
    ///
    /// Object keys are identifiers; other entries begin with values. A following
    /// colon handles both object fields and map entries, with one space before
    /// the associated value. Containers use a compact layout when it fits.
    fn container(&mut self, depth: usize, gap: Gap) -> Result<(), AsonError> {
        let closing = match self.kind() {
            Some(SyntaxKind::BraceOpen) => SyntaxKind::BraceClose,
            Some(SyntaxKind::BracketOpen) => SyntaxKind::BracketClose,
            Some(SyntaxKind::ParenthesisOpen) => SyntaxKind::ParenthesisClose,
            _ => return Err(layout_error()),
        };
        let opening_index = self.cursor;
        self.consume(depth, gap);
        let mut entries = Vec::new();
        while self.kind() != Some(closing) {
            entries.push(self.cursor);
            if self.kind() == Some(SyntaxKind::Identifier) {
                self.consume(depth + 1, Gap::Line);
            } else {
                self.value(depth + 1, Gap::Line)?;
            }
            if self.kind() == Some(SyntaxKind::Colon) {
                self.consume(depth + 1, Gap::None);
                self.value(depth + 1, Gap::Space)?;
            }
        }
        let closing_index = self.cursor;
        let empty = entries.is_empty();
        self.consume(depth, if empty { Gap::None } else { Gap::Line });
        if !empty && self.fits_single_line(opening_index, closing_index, &entries, depth) {
            for (index, entry) in entries.iter().enumerate() {
                self.placements[*entry].0 = if index == 0 { Gap::None } else { Gap::Space };
            }
            self.placements[closing_index].0 = Gap::None;
        }
        Ok(())
    }

    /// Checks the compact form, counting indentation and rejecting comments or
    /// tokens that already contain line breaks.
    fn fits_single_line(
        &self,
        opening_index: usize,
        closing_index: usize,
        entries: &[usize],
        depth: usize,
    ) -> bool {
        let opening = self.tokens[opening_index];
        let closing = self.tokens[closing_index];
        if self.comments.iter().any(|comment| {
            comment.span.start > opening.span.start && comment.span.end < closing.span.end
        }) {
            return false;
        }

        let line_start = if opening_index > 0
            && self.tokens[opening_index - 1].kind == SyntaxKind::Enumeration
            && self.placements[opening_index].0 == Gap::None
        {
            opening_index - 1
        } else {
            opening_index
        };
        let Some(mut width) = depth.checked_mul(self.indent_width) else {
            return false;
        };
        let mut entry_cursor = 0;
        for index in line_start..=closing_index {
            let token = self.tokens[index];
            let text = &self.source[token.span.clone()];
            if text.contains('\n') || text.contains('\r') {
                return false;
            }
            width = width.saturating_add(text.chars().count());

            if index == line_start || index == closing_index {
                continue;
            }
            let gap_width = if entries.get(entry_cursor) == Some(&index) {
                let gap_width = usize::from(entry_cursor > 0);
                entry_cursor += 1;
                gap_width
            } else {
                match self.placements[index].0 {
                    Gap::None => 0,
                    Gap::Space => 1,
                    Gap::Line => return false,
                }
            };
            width = width.saturating_add(gap_width);
            if width > MAX_LINE_WIDTH {
                return false;
            }
        }
        width <= MAX_LINE_WIDTH
    }
}

#[derive(Clone)]
/// Type information inferred from values, without retaining their actual data.
///
/// Empty collections and absent options start with unknown inner types. Merging
/// shapes from sibling entries fills those gaps and rejects incompatible types.
/// Objects collect fields by name; enumerations collect payloads by variant name.
enum Shape {
    /// No evidence yet, as with the element type of an empty list.
    Unknown,
    /// A scalar type, distinguished by its `ason::Value` variant rather than value.
    Atom(
        /// The value variant identifying the scalar's type.
        std::mem::Discriminant<ason::Value>,
    ),
    /// A homogeneous list whose entries share one inferred shape.
    List(
        /// The common element shape, possibly still unknown for an empty list.
        Box<Shape>,
    ),
    /// A map whose keys and values are checked independently.
    Map(
        /// The common key shape.
        Box<Shape>,
        /// The common value shape.
        Box<Shape>,
    ),
    /// A fixed-length sequence whose positions may have different types.
    Tuple(
        /// One shape per position, in tuple order.
        Vec<Shape>,
    ),
    /// An object whose shared field names must have compatible types.
    Object(
        /// Field names mapped to their inferred shapes, independent of source order.
        std::collections::BTreeMap<String, Shape>,
    ),
    /// An enumeration with one shared type name and separately checked variants.
    Enumeration(
        /// The enumeration type name, without the variant name.
        String,
        /// Known variant names mapped to the shape of each variant's payload.
        std::collections::BTreeMap<String, Shape>,
    ),
    /// An optional value; `None` contributes no evidence about the inner type.
    Option(
        /// The inferred shape of a present value, or unknown if none was observed.
        Box<Shape>,
    ),
}

/// Builds the shared diagnostic for incompatible collection member types.
fn type_error() -> AsonError {
    AsonError::Message("List elements and Map keys/values must have consistent types.".into())
}

/// Refines `target` with evidence from `incoming`, or reports incompatible types.
///
/// Unknown shapes act as missing information, not as a separate concrete type.
/// Compound shapes merge recursively: tuple positions must line up, map keys and
/// values are independent, and enum payloads are compared only within a variant.
/// A failed merge may leave `target` partly refined; callers discard it on error.
fn merge(target: &mut Shape, incoming: Shape) -> Result<(), AsonError> {
    match (target, incoming) {
        (target @ Shape::Unknown, incoming) => *target = incoming,
        (_, Shape::Unknown) => {}
        (Shape::Atom(left), Shape::Atom(right)) if *left == right => {}
        (Shape::List(left), Shape::List(right)) | (Shape::Option(left), Shape::Option(right)) => {
            merge(left, *right)?
        }
        // Empty bracket syntax carries no evidence that it is a list rather
        // than a map. Only a list with an unknown element can become a map;
        // once actual list elements exist, that interpretation is no longer valid.
        (target @ Shape::List(_), Shape::Map(key, value)) => {
            if !matches!(target, Shape::List(inner) if matches!(**inner, Shape::Unknown)) {
                return Err(type_error());
            }
            *target = Shape::Map(key, value);
        }
        (Shape::Map(_, _), Shape::List(inner)) if matches!(*inner, Shape::Unknown) => {}
        (Shape::Map(left_key, left_value), Shape::Map(right_key, right_value)) => {
            merge(left_key, *right_key)?;
            merge(left_value, *right_value)?;
        }
        (Shape::Tuple(left), Shape::Tuple(right)) if left.len() == right.len() => {
            for (left, right) in left.iter_mut().zip(right) {
                merge(left, right)?;
            }
        }
        (Shape::Object(left), Shape::Object(right)) => merge_fields(left, right)?,
        (Shape::Enumeration(left_name, left), Shape::Enumeration(right_name, right))
            if *left_name == right_name =>
        {
            merge_fields(left, right)?
        }
        _ => return Err(type_error()),
    }
    Ok(())
}

/// Merges named fields or enum variants, checking only names present in both.
///
/// Missing names add new evidence rather than causing an error. This lets objects
/// with different field sets coexist while still checking shared fields.
fn merge_fields(
    target: &mut std::collections::BTreeMap<String, Shape>,
    incoming: std::collections::BTreeMap<String, Shape>,
) -> Result<(), AsonError> {
    for (name, incoming) in incoming {
        merge(target.entry(name).or_insert(Shape::Unknown), incoming)?;
    }
    Ok(())
}

/// Infers a recursive shape and checks homogeneous collections along the way.
///
/// `value` is already parsed, so this pass can focus on relationships between
/// values rather than token syntax. Lists merge all elements; maps separately
/// merge all keys and all values. Tuples retain a shape for every position.
fn shape(value: &ason::Value) -> Result<Shape, AsonError> {
    use ason::{Value, VariantValue};
    // Use names rather than source positions so field order cannot change type
    // compatibility. The same conversion also handles struct-style variants.
    let fields = |fields: &[(String, Value)]| -> Result<_, AsonError> {
        fields
            .iter()
            .map(|(name, value)| Ok((name.clone(), shape(value)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()
    };
    Ok(match value {
        Value::List(values) => {
            let mut element = Shape::Unknown;
            for value in values {
                merge(&mut element, shape(value)?)?;
            }
            Shape::List(Box::new(element))
        }
        Value::Map(entries) => {
            let (mut key, mut value) = (Shape::Unknown, Shape::Unknown);
            for (entry_key, entry_value) in entries {
                merge(&mut key, shape(entry_key)?)?;
                merge(&mut value, shape(entry_value)?)?;
            }
            Shape::Map(Box::new(key), Box::new(value))
        }
        Value::Tuple(values) => Shape::Tuple(values.iter().map(shape).collect::<Result<_, _>>()?),
        Value::Object(values) => Shape::Object(fields(values)?),
        Value::Option(value) => Shape::Option(Box::new(match value {
            Some(value) => shape(value)?,
            None => Shape::Unknown,
        })),
        Value::Enumeration {
            name,
            variant,
            payload,
        } => {
            // A variant's name selects its own payload shape. Unit variants use
            // an empty tuple, while struct payloads keep their named fields.
            let payload = match payload {
                VariantValue::Unit => Shape::Tuple(Vec::new()),
                VariantValue::Newtype(value) => Shape::Tuple(vec![shape(value)?]),
                VariantValue::Tuple(values) => {
                    Shape::Tuple(values.iter().map(shape).collect::<Result<_, _>>()?)
                }
                VariantValue::Struct(values) => Shape::Object(fields(values)?),
            };
            Shape::Enumeration(name.clone(), [(variant.clone(), payload)].into())
        }
        // Compare scalar variants, not scalar contents: two different numbers
        // can share a type without needing equal values.
        _ => Shape::Atom(std::mem::discriminant(value)),
    })
}

/// Builds output from unchanged token text and planned leading whitespace.
struct Writer {
    /// The accumulated formatted document, before the final newline is appended.
    output: String,
    /// Spaces per nesting level, copied from the caller's format options.
    indent_width: usize,
    indent_style: IndentStyle,
    line_ending: LineEnding,
}

impl Writer {
    /// Appends `text` verbatim after `gap`, indenting new lines to `depth`.
    ///
    /// Only whitespace outside the token is changed. Newlines inside `text` are
    /// deliberately not reindented, since doing so could change a literal value
    /// or alter a comment. Leading gaps are suppressed for the first output token.
    fn write(&mut self, text: &str, gap: Gap, depth: usize) -> Result<(), AsonError> {
        if !self.output.is_empty() {
            match gap {
                Gap::Line => self.output.push_str(self.line_ending.as_str()),
                Gap::Space => self.output.push(' '),
                Gap::None => {}
            }
        }
        if self.output.is_empty() || gap == Gap::Line {
            // Both the multiplication and the capacity request can fail for a
            // very large user-supplied width. Turn either case into a diagnostic.
            let count = depth
                .checked_mul(if self.indent_style == IndentStyle::Tabs {
                    1
                } else {
                    self.indent_width
                })
                .ok_or_else(|| AsonError::Message("Indentation is too large.".into()))?;
            self.output
                .try_reserve(count)
                .map_err(|_| AsonError::Message("Indentation is too large.".into()))?;
            let character = if self.indent_style == IndentStyle::Tabs {
                '\t'
            } else {
                ' '
            };
            self.output.extend(std::iter::repeat_n(character, count));
        }
        self.output.push_str(text);
        Ok(())
    }
}

#[cfg(test)]
/// Covers exact layout, semantic preservation, and recursive type compatibility.
mod tests {
    use super::*;

    /// Collects literal and comment spellings that formatting must never change.
    fn preserved(source: &str) -> Vec<String> {
        tokenize(source)
            .unwrap()
            .iter()
            .filter(|token| {
                matches!(
                    token.kind,
                    SyntaxKind::Number
                        | SyntaxKind::Sign
                        | SyntaxKind::Boolean
                        | SyntaxKind::Char
                        | SyntaxKind::String
                        | SyntaxKind::DateTime
                        | SyntaxKind::Bytes
                        | SyntaxKind::LineComment
                        | SyntaxKind::BlockComment
                )
            })
            .map(|token| source[token.span.clone()].to_owned())
            .collect()
    }

    /// Formats a fixture and checks idempotence, token preservation, and meaning.
    ///
    /// Serializing parsed values provides a comparable representation even for
    /// values such as NaN, for which ordinary equality is not a useful check.
    fn check(source: &str) -> String {
        let output = format(source).unwrap();
        assert_eq!(format(&output).unwrap(), output, "{source}");
        assert_eq!(preserved(source), preserved(&output));
        assert_eq!(
            ason::ser_value_to_string(&ason::de_value_from_str(source).unwrap()).unwrap(),
            ason::ser_value_to_string(&ason::de_value_from_str(&output).unwrap()).unwrap()
        );
        output
    }

    #[test]
    /// Checks compact containers and compact empty containers.
    fn object_layout() {
        assert_eq!(
            check("{name: r\"Alice\", nums: [0xff, 0b10], pair: (true, '中')}"),
            "{name: r\"Alice\" nums: [0xff 0b10] pair: (true '中')}\n"
        );
        assert_eq!(check("[1 2 3]"), "[1 2 3]\n");
        assert_eq!(check("[1: true 2: false]"), "[1: true 2: false]\n");
        assert_eq!(check("Option::Some(1)"), "Option::Some(1)\n");
        for source in [
            "Some(Ok([11 13 17]))",
            "Result::Ok(Some(11))",
            "Err(\"failed\")",
            "None",
            "Option::Some(None)",
            "Some({id: 123 name: \"Alice\"})",
        ] {
            assert_eq!(check(source), format!("{source}\n"));
        }
        assert_eq!(
            check("Shape::Rect{width: 200 height: 100}"),
            "Shape::Rect{width: 200 height: 100}\n"
        );
        assert_eq!(check("[]"), "[]\n");
        assert_eq!(check("{}"), "{}\n");
        assert_eq!(check("[ , ]"), "[]\n");
    }

    #[test]
    /// Exercises tricky literal spellings and comments inside grammatical units.
    fn comments_and_literals() {
        for source in [
            "// header\n{a: - /* sign */ 128_i8, /* nested /* inner */ end */ b: true // tail\n}\n// eof",
            "{a /* key */ : /* value */ 42}",
            "Option::Some /* payload */ (0x1.8p+2_f64)",
            "(r##\"a\"#中\"##, \"a\\\r\n  b\", \"a\r\n b\", \"\"\"\n  x\n    y\n\"\"\", h\"48\n 65\", d\"2024-03-16\")",
            "[(1, true): 42, (2, false): 43]",
            "[/* empty */]",
            "{/* defaults */}",
            "{a: 1 /* trailing */\n/* leading */ b: 2}",
            "(- // sign\n128_i8, +Inf, NaN_f32, 0o755_u32)",
            "Shape::Rect{width: 200 height: 100}",
        ] {
            check(source);
        }
    }

    #[test]
    /// Rejects syntax, range, and type errors instead of formatting partial input.
    fn rejects_invalid_documents() {
        for source in [
            "",
            "// only comment",
            "1 2",
            "[1 true]",
            "()",
            "Option::Some()",
            "{Some: 1}",
            "{None: 1}",
            "{Ok: 1}",
            "{Err: 1}",
            "{a: 1 a: 2}",
            "256_u8",
            "[",
            "{a 1}",
        ] {
            assert!(format(source).is_err(), "{source}");
        }
    }

    #[test]
    /// Confirms compact values and custom indentation for values wider than 80.
    fn custom_indent() {
        assert_eq!(
            format_with_options(
                "[1 2]",
                &FormatOptions {
                    indent_width: 2,
                    ..FormatOptions::default()
                }
            )
            .unwrap(),
            "[1 2]\n"
        );
        let item = "a".repeat(39);
        let source = format!("[\"{item}\" \"{item}\"]");
        assert_eq!(
            format_with_options(
                &source,
                &FormatOptions {
                    indent_width: 2,
                    ..FormatOptions::default()
                }
            )
            .unwrap(),
            format!("[\n  \"{item}\"\n  \"{item}\"\n]\n")
        );
        assert_eq!(
            format_with_options(
                &source,
                &FormatOptions {
                    indent_width: 0,
                    ..FormatOptions::default()
                }
            )
            .unwrap(),
            format!("[\n\"{item}\"\n\"{item}\"\n]\n")
        );
    }

    #[test]
    fn editor_layout_preserves_tokens() {
        let source = "{a: \"one\n two\", /* line\r\n body */ b: r#\"raw\r\n text\"#}";
        for indent_style in [IndentStyle::Spaces, IndentStyle::Tabs] {
            for line_ending in [LineEnding::Lf, LineEnding::CrLf] {
                let options = FormatOptions {
                    indent_width: 2,
                    indent_style,
                    line_ending,
                };
                let output = format_with_options(source, &options).unwrap();
                assert_eq!(preserved(source), preserved(&output));
                assert_eq!(format_with_options(&output, &options).unwrap(), output);
                assert!(output.ends_with(line_ending.as_str()));
                let indentation = if indent_style == IndentStyle::Tabs {
                    "\t"
                } else {
                    "  "
                };
                assert!(output.contains(&format!("{}{indentation}a:", line_ending.as_str())));
            }
        }
        assert_eq!(
            format(source).unwrap(),
            format_with_options(source, &FormatOptions::default()).unwrap()
        );
        for source in ["[", "[1 true]", "{a:1 a:2}", "", "256_u8"] {
            assert_eq!(
                validate_document(source).unwrap_err(),
                format(source).unwrap_err()
            );
        }
    }

    #[test]
    fn tab_width_controls_line_fitting() {
        let item = "x".repeat(69);
        let source = format!("{{// root\na: [\"{item}\"]}}");
        let narrow = FormatOptions {
            indent_width: 2,
            indent_style: IndentStyle::Tabs,
            ..FormatOptions::default()
        };
        let wide = FormatOptions {
            indent_width: 8,
            ..narrow
        };
        assert!(
            format_with_options(&source, &narrow)
                .unwrap()
                .contains(&format!("\ta: [\"{item}\"]"))
        );
        assert!(
            format_with_options(&source, &wide)
                .unwrap()
                .contains("\ta: [\n\t\t")
        );
    }

    #[test]
    /// Keeps a container inline at 80 characters but wraps it at 81.
    fn compact_width_boundary() {
        let item = "x".repeat(76);
        let source = format!("[\"{item}\"]");
        assert_eq!(source.chars().count(), MAX_LINE_WIDTH);
        assert_eq!(format(&source).unwrap(), format!("{source}\n"));

        let item = "x".repeat(77);
        let source = format!("[\"{item}\"]");
        assert_eq!(source.chars().count(), MAX_LINE_WIDTH + 1);
        assert_eq!(format(&source).unwrap(), format!("[\n    \"{item}\"\n]\n"));
    }

    #[test]
    /// Pins down standalone, trailing, and inline comment placement.
    fn comment_layout() {
        assert_eq!(
            check("// header\n{a:1 // tail\n// next\nb:2}\n// end"),
            "// header\n{\n    a: 1 // tail\n    // next\n    b: 2\n}\n// end\n"
        );
        assert_eq!(check("[/* empty */]"), "[\n    /* empty */\n]\n");
        assert_eq!(check("{a: /* value */ 1}"), "{\n    a: /* value */ 1\n}\n");
    }

    #[test]
    /// Covers unknown-type refinement and incompatible nested collection shapes.
    fn compound_type_inference() {
        for source in [
            "[[] [1] [2]]",
            "[[] [1: true]]",
            "[[1: true] []]",
            "[{} {a: 1} {a: 2 b: true}]",
            "[Option::None Option::Some(1)]",
            "[Color::Red Color::Rgb(1 2 3)]",
            "[{b: true a: 1} {a: 2 b: false}]",
            "(NaN NaN_f32 -Inf +Inf_f32 -128_i8 255_u8 65_535_u16)",
        ] {
            check(source);
        }
        for source in [
            "[[1] [true]]",
            "[1: true 2: 3]",
            "[1: true false: true]",
            "[(1 true) (2 3)]",
            "[{a: 1} {a: true}]",
            "[Option::Some(1) Option::Some(true)]",
            "[Color::Rgb(1) Color::Rgb(true)]",
            "[A::X B::X]",
            "[[] [1] [true]]",
            "[[] [1: true] [false: true]]",
        ] {
            assert!(format(source).is_err(), "{source}");
        }
    }

    #[test]
    /// Inserts comments at every token boundary to stress the replay/layout split.
    fn comments_at_token_boundaries() {
        for source in [
            "{a: -128_i8 b: [1 2] c: X::Y(3)}",
            "[(1 true): 42 (2 false): 43]",
            "Option::Some({a: [] b: {}})",
            "(NaN -Inf 0x1.8p+2_f64 r##\"a\"#中\"##)",
        ] {
            let tokens = tokenize(source).unwrap();
            let lexemes: Vec<_> = tokens
                .iter()
                .filter(|token| !matches!(token.kind, SyntaxKind::Whitespace | SyntaxKind::Comma))
                .map(|token| &source[token.span.clone()])
                .collect();
            // Rebuilding from lexemes exercises positions that hand-written
            // fixtures can miss, such as between a sign and its numeric literal.
            for gap in [
                " /* gap */ ",
                " // gap\n",
                "\n/* gap */\n",
                " /* one */ /* two */ ",
            ] {
                let commented = lexemes.join(gap);
                let output = check(&commented);
                assert_eq!(
                    ason::ser_value_to_string(&ason::de_value_from_str(source).unwrap()).unwrap(),
                    ason::ser_value_to_string(&ason::de_value_from_str(&output).unwrap()).unwrap()
                );
            }
        }
    }
}
