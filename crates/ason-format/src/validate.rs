// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Validate a document beyond what the ASON parser checks, reporting source ranges.
//!
//! The parser checks syntax, value ranges, duplicate keys, reserved Object keys,
//! and the predefined `Option` and `Result` Enumerations. This module adds two
//! document-level checks: collection members must have compatible types, and
//! reserved words cannot name an Enumeration type.
//!
//! Parsed values do not record where they came from, so the checker walks the
//! value tree and the structural tokens together. Each value consumes exactly
//! the tokens that spell it, which gives every reported value a source range.

use std::{collections::BTreeMap, ops::Range};

use ason::syntax::{SyntaxKind, SyntaxToken, tokenize};

use crate::AsonError;

/// Keywords of the ASON lexer, including the `Option`/`Result` variant shorthands.
///
/// The parser already rejects them as Object keys, but it still accepts them as
/// the type name of a user-defined Enumeration (e.g. `Some::A`).
const RESERVED_WORDS: &[&str] = &[
    "true", "false", "NaN", "NaN_f32", "NaN_f64", "Inf", "Inf_f32", "Inf_f64", "Some", "None",
    "Ok", "Err",
];

/// Checks that `source` is one valid ASON value with consistent collection types.
///
/// # Errors
///
/// Returns the parser's error for invalid syntax or values. Inconsistent
/// collection types and reserved Enumeration type names are reported as
/// [`AsonError::MessageWithRange`] covering the offending value or name.
///
/// # Examples
///
/// ```
/// use ason_format::{AsonError, validate_document};
///
/// assert!(validate_document("[1 2 3]").is_ok());
/// let error = validate_document("[1 2 true]").unwrap_err();
/// assert!(matches!(error, AsonError::MessageWithRange(..)));
/// ```
pub fn validate_document(source: &str) -> Result<(), AsonError> {
    let value = ason::de_value_from_str(source)?;
    let tokens = tokenize(source)?;
    check(source, &value, &structural(&tokens))
}

/// Selects the tokens that spell values, skipping whitespace, commas, and comments.
pub(crate) fn structural(tokens: &[SyntaxToken]) -> Vec<&SyntaxToken> {
    tokens
        .iter()
        .filter(|token| {
            !matches!(
                token.kind,
                SyntaxKind::Whitespace
                    | SyntaxKind::Comma
                    | SyntaxKind::LineComment
                    | SyntaxKind::BlockComment
            )
        })
        .collect()
}

/// Validates an already parsed `value` against its structural `tokens`.
pub(crate) fn check(
    source: &str,
    value: &ason::Value,
    tokens: &[&SyntaxToken],
) -> Result<(), AsonError> {
    let mut checker = Checker {
        source,
        tokens,
        cursor: 0,
    };
    checker.shape(value)?;
    Ok(())
}

/// Reports an unexpected mismatch between the parsed value and its tokens.
fn walk_error() -> AsonError {
    AsonError::Message("Unexpected token while validating the document.".into())
}

/// Builds an error covering the byte range `span` of `source`.
///
/// `ason` does not export its `Range` and `Position` types, but they implement
/// `Default` and have public fields, so the range is filled in place.
fn range_error(source: &str, span: Range<usize>, message: String) -> AsonError {
    // Positions use character indexes and zero-based lines and columns, with
    // only `\n` starting a new line, matching the ASON lexer.
    let locate = |byte: usize| {
        let prefix = &source[..byte];
        let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
        (
            prefix.chars().count(),
            prefix.matches('\n').count(),
            prefix[line_start..].chars().count(),
        )
    };
    // The range end is inclusive: it is the start of the last character.
    let last = source[..span.end]
        .char_indices()
        .next_back()
        .map_or(span.start, |(index, _)| index.max(span.start));
    let mut error = AsonError::MessageWithRange(message, Default::default());
    if let AsonError::MessageWithRange(_, range) = &mut error {
        (range.start.index, range.start.line, range.start.column) = locate(span.start);
        (
            range.end_inclusive.index,
            range.end_inclusive.line,
            range.end_inclusive.column,
        ) = locate(last);
    }
    error
}

/// Walks parsed values and their structural tokens in step.
struct Checker<'a> {
    source: &'a str,
    tokens: &'a [&'a SyntaxToken],
    /// Index of the next token that no value has consumed yet.
    cursor: usize,
}

impl Checker<'_> {
    /// Consumes one token, which must have the given `kind`.
    fn expect(&mut self, kind: SyntaxKind) -> Result<&SyntaxToken, AsonError> {
        let token = *self.tokens.get(self.cursor).ok_or_else(walk_error)?;
        if token.kind != kind {
            return Err(walk_error());
        }
        self.cursor += 1;
        Ok(token)
    }

    /// Returns the byte range covering the tokens consumed since token `start`.
    fn span_from(&self, start: usize) -> Range<usize> {
        self.tokens[start].span.start..self.tokens[self.cursor - 1].span.end
    }

    /// Infers the shape of `value` and merges it into the shape shared by its
    /// siblings. A value that conflicts with earlier ones is reported with its
    /// own range.
    fn merge_into(
        &mut self,
        target: &mut Shape,
        value: &ason::Value,
        message: &str,
    ) -> Result<(), AsonError> {
        let start = self.cursor;
        let shape = self.shape(value)?;
        merge(target, shape)
            .map_err(|Mismatch| range_error(self.source, self.span_from(start), message.into()))
    }

    /// Infers the shapes of `values`, one per position, as in a tuple.
    fn sequence(&mut self, values: &[ason::Value]) -> Result<Vec<Shape>, AsonError> {
        values.iter().map(|value| self.shape(value)).collect()
    }

    /// Infers the shapes of named fields, consuming each `name: value` pair.
    fn fields(
        &mut self,
        fields: &[(String, ason::Value)],
    ) -> Result<BTreeMap<String, Shape>, AsonError> {
        self.expect(SyntaxKind::BraceOpen)?;
        let mut shapes = BTreeMap::new();
        for (name, value) in fields {
            self.expect(SyntaxKind::Identifier)?;
            self.expect(SyntaxKind::Colon)?;
            shapes.insert(name.clone(), self.shape(value)?);
        }
        self.expect(SyntaxKind::BraceClose)?;
        Ok(shapes)
    }

    /// Infers a recursive shape and checks homogeneous collections along the way.
    ///
    /// Lists merge all elements; maps separately merge all keys and all values.
    /// Tuples retain a shape for every position.
    fn shape(&mut self, value: &ason::Value) -> Result<Shape, AsonError> {
        use ason::{Value, VariantValue};
        Ok(match value {
            Value::List(values) => {
                self.expect(SyntaxKind::BracketOpen)?;
                let mut element = Shape::Unknown;
                for value in values {
                    self.merge_into(
                        &mut element,
                        value,
                        "List element type does not match the earlier elements.",
                    )?;
                }
                self.expect(SyntaxKind::BracketClose)?;
                Shape::List(Box::new(element))
            }
            Value::Map(entries) => {
                self.expect(SyntaxKind::BracketOpen)?;
                let (mut key, mut value) = (Shape::Unknown, Shape::Unknown);
                for (entry_key, entry_value) in entries {
                    self.merge_into(
                        &mut key,
                        entry_key,
                        "Map key type does not match the earlier keys.",
                    )?;
                    self.expect(SyntaxKind::Colon)?;
                    self.merge_into(
                        &mut value,
                        entry_value,
                        "Map value type does not match the earlier values.",
                    )?;
                }
                self.expect(SyntaxKind::BracketClose)?;
                Shape::Map(Box::new(key), Box::new(value))
            }
            Value::Tuple(values) => {
                self.expect(SyntaxKind::ParenthesisOpen)?;
                let shapes = self.sequence(values)?;
                self.expect(SyntaxKind::ParenthesisClose)?;
                Shape::Tuple(shapes)
            }
            Value::Object(fields) => Shape::Object(self.fields(fields)?),
            Value::Enumeration {
                name,
                variant,
                payload,
            } => {
                let token = self.expect(SyntaxKind::Enumeration)?;
                if RESERVED_WORDS.contains(&name.as_str()) {
                    // Full forms are spelled `name::variant` without whitespace.
                    let start = token.span.start;
                    return Err(range_error(
                        self.source,
                        start..start + name.len(),
                        format!(
                            "\"{name}\" is a reserved word and cannot be used as an Enumeration name."
                        ),
                    ));
                }
                // A variant's name selects its own payload shape. Unit variants use
                // an empty tuple, while struct payloads keep their named fields.
                let payload = match payload {
                    VariantValue::Unit => Shape::Tuple(Vec::new()),
                    VariantValue::Newtype(value) => {
                        self.expect(SyntaxKind::ParenthesisOpen)?;
                        let shape = self.shape(value)?;
                        self.expect(SyntaxKind::ParenthesisClose)?;
                        Shape::Tuple(vec![shape])
                    }
                    VariantValue::Tuple(values) => {
                        self.expect(SyntaxKind::ParenthesisOpen)?;
                        let shapes = self.sequence(values)?;
                        self.expect(SyntaxKind::ParenthesisClose)?;
                        Shape::Tuple(shapes)
                    }
                    VariantValue::Struct(fields) => Shape::Object(self.fields(fields)?),
                };
                Shape::Enumeration(name.clone(), [(variant.clone(), payload)].into())
            }
            // Compare scalar variants, not scalar contents: two different numbers
            // can share a type without needing equal values.
            _ => {
                if self.tokens.get(self.cursor).map(|token| token.kind) == Some(SyntaxKind::Sign) {
                    self.cursor += 1;
                }
                let kind = self.tokens.get(self.cursor).ok_or_else(walk_error)?.kind;
                if !matches!(
                    kind,
                    SyntaxKind::Number
                        | SyntaxKind::Boolean
                        | SyntaxKind::Char
                        | SyntaxKind::String
                        | SyntaxKind::DateTime
                        | SyntaxKind::Bytes
                ) {
                    return Err(walk_error());
                }
                self.cursor += 1;
                Shape::Atom(std::mem::discriminant(value))
            }
        })
    }
}

#[derive(Clone)]
/// Type information inferred from values, without retaining their actual data.
///
/// Empty collections start with unknown inner types. Merging
/// shapes from sibling entries fills those gaps and rejects incompatible types.
/// Objects collect fields by name; enumerations collect payloads by variant name.
enum Shape {
    /// No evidence yet, as with the element type of an empty list.
    Unknown,
    /// A scalar type, distinguished by its `ason::Value` variant rather than value.
    Atom(std::mem::Discriminant<ason::Value>),
    /// A homogeneous list whose entries share one inferred element shape.
    List(Box<Shape>),
    /// A map whose key shape and value shape are checked independently.
    Map(Box<Shape>, Box<Shape>),
    /// A fixed-length sequence with one shape per position.
    Tuple(Vec<Shape>),
    /// An object whose field names map to shapes, independent of source order.
    Object(BTreeMap<String, Shape>),
    /// An enumeration type name with known variant names mapped to payload shapes.
    Enumeration(String, BTreeMap<String, Shape>),
}

/// Marks two shapes that cannot describe the same type.
struct Mismatch;

/// Refines `target` with evidence from `incoming`, or reports incompatible types.
///
/// Unknown shapes act as missing information, not as a separate concrete type.
/// Compound shapes merge recursively: tuple positions must line up, map keys and
/// values are independent, and enum payloads are compared only within a variant.
/// A failed merge may leave `target` partly refined; callers discard it on error.
fn merge(target: &mut Shape, incoming: Shape) -> Result<(), Mismatch> {
    match (target, incoming) {
        (target @ Shape::Unknown, incoming) => *target = incoming,
        (_, Shape::Unknown) => {}
        (Shape::Atom(left), Shape::Atom(right)) if *left == right => {}
        (Shape::List(left), Shape::List(right)) => merge(left, *right)?,
        // Empty bracket syntax carries no evidence that it is a list rather
        // than a map. Only a list with an unknown element can become a map;
        // once actual list elements exist, that interpretation is no longer valid.
        (target @ Shape::List(_), Shape::Map(key, value)) => {
            if !matches!(target, Shape::List(inner) if matches!(**inner, Shape::Unknown)) {
                return Err(Mismatch);
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
        _ => return Err(Mismatch),
    }
    Ok(())
}

/// Merges named fields or enum variants, checking only names present in both.
///
/// Missing names add new evidence rather than causing an error. This lets objects
/// with different field sets coexist while still checking shared fields.
fn merge_fields(
    target: &mut BTreeMap<String, Shape>,
    incoming: BTreeMap<String, Shape>,
) -> Result<(), Mismatch> {
    for (name, incoming) in incoming {
        merge(target.entry(name).or_insert(Shape::Unknown), incoming)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the source text, line, and column (both one-based) of an error.
    fn located(source: &str) -> (String, usize, usize) {
        match validate_document(source).unwrap_err() {
            AsonError::MessageWithRange(_, range) => {
                let text: String = source
                    .chars()
                    .skip(range.start.index)
                    .take(range.end_inclusive.index + 1 - range.start.index)
                    .collect();
                (text, range.start.line + 1, range.start.column + 1)
            }
            error => panic!("expected a range for {source:?}, got {error:?}"),
        }
    }

    #[test]
    /// Covers unknown-type refinement and compatible nested collection shapes.
    fn accepts_compatible_types() {
        for source in [
            "[[] [1] [2]]",
            "[[] [1: true]]",
            "[[1: true] []]",
            "[{} {a: 1} {a: 2 b: true}]",
            "[Option::None Option::Some(1)]",
            "[None Some(1) Option::Some(2)]",
            "[Ok(1) Err(\"e\") Result::Ok(2)]",
            "[Some(Ok(1)) Some(Err(\"e\")) None]",
            "[Custom::Some(1 2) Custom::None]",
            "[Color::Red Color::Rgb(1 2 3)]",
            "[{b: true a: 1} {a: 2 b: false}]",
            "(NaN NaN_f32 -Inf +Inf_f32 -128_i8 255_u8 65_535_u16)",
            "[-1 /* c */ +2 // c\n 3]",
            "[Shape::Rect{w: 1 h: 2} Shape::Rect{h: 3 w: 4}]",
        ] {
            assert!(validate_document(source).is_ok(), "{source}");
        }
    }

    #[test]
    /// Reports the first value whose type conflicts with its earlier siblings.
    fn locates_type_errors() {
        for (source, text, line, column) in [
            ("[1 true]", "true", 1, 4),
            ("[[1] [true]]", "[true]", 1, 6),
            ("[1: true 2: 3]", "3", 1, 13),
            ("[1: true false: true]", "false", 1, 10),
            ("[(1 true) (2 3)]", "(2 3)", 1, 11),
            ("[{a: 1} {a: true}]", "{a: true}", 1, 9),
            (
                "[Option::Some(1) Option::Some(true)]",
                "Option::Some(true)",
                1,
                18,
            ),
            ("[Some(1) Some(true)]", "Some(true)", 1, 10),
            ("[Ok(1) Ok(true)]", "Ok(true)", 1, 8),
            ("[Some(1) Ok(1)]", "Ok(1)", 1, 10),
            ("[Err(1) Result::Err(\"e\")]", "Result::Err(\"e\")", 1, 9),
            (
                "[Color::Rgb(1) Color::Rgb(true)]",
                "Color::Rgb(true)",
                1,
                16,
            ),
            ("[A::X B::X]", "B::X", 1, 7),
            ("[[] [1] [true]]", "[true]", 1, 9),
            ("[[] [1: true] [false: true]]", "[false: true]", 1, 15),
            ("[1 2 -3_i8]", "-3_i8", 1, 6),
            // Inside nested values, the conflicting member of the innermost
            // collection is reported.
            ("{a: [1 2 'x']}", "'x'", 1, 10),
            ("{list: [\n    1\n    /* c */ \"中\"\n]}", "\"中\"", 3, 13),
        ] {
            assert_eq!(located(source), (text.into(), line, column), "{source}");
        }
    }

    #[test]
    /// Reports only the type name of a reserved Enumeration type name.
    fn locates_reserved_enumeration_names() {
        for (source, text, line, column) in [
            ("Some::A", "Some", 1, 1),
            ("None::A", "None", 1, 1),
            ("Ok::A(1)", "Ok", 1, 1),
            ("Err::A{a: 1}", "Err", 1, 1),
            ("NaN::A", "NaN", 1, 1),
            ("Inf_f32::A", "Inf_f32", 1, 1),
            ("true::A", "true", 1, 1),
            ("[Some::A]", "Some", 1, 2),
            ("{a: 1\n b: None::B}", "None", 2, 5),
        ] {
            assert_eq!(located(source), (text.into(), line, column), "{source}");
        }
        assert!(validate_document("Custom::Some(1)").is_ok());
    }
}
