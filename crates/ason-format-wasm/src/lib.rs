// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use ason_format::{AsonError, FormatOptions, IndentStyle, LineEnding};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticError {
    message: String,
    kind: &'static str,
    start: Option<usize>,
    end: Option<usize>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum Response {
    Success { text: Option<String> },
    Error { error: DiagnosticError },
}

fn utf16_offset(source: &str, scalar_index: usize) -> usize {
    source.chars().take(scalar_index).map(char::len_utf16).sum()
}

fn diagnostic(source: &str, error: AsonError) -> DiagnosticError {
    let (message, kind, range) = match error {
        AsonError::Message(message) => (message, "document", None),
        AsonError::UnexpectedEndOfDocument(message) => {
            let end = source.encode_utf16().count();
            (message, "eof", Some((end, end)))
        }
        AsonError::MessageWithPosition(message, position) => (
            message,
            "syntax",
            Some((
                utf16_offset(source, position.index),
                utf16_offset(source, position.index.saturating_add(1)),
            )),
        ),
        AsonError::MessageWithRange(message, range) => (
            message,
            "syntax",
            Some((
                utf16_offset(source, range.start.index),
                utf16_offset(source, range.end_inclusive.index.saturating_add(1)),
            )),
        ),
    };
    DiagnosticError {
        message,
        kind,
        start: range.map(|range| range.0),
        end: range.map(|range| range.1),
    }
}

fn response(source: &str, result: Result<Option<String>, AsonError>) -> String {
    let response = match result {
        Ok(text) => Response::Success { text },
        Err(error) => Response::Error {
            error: diagnostic(source, error),
        },
    };
    serde_json::to_string(&response).expect("Response contains only JSON-serializable fields")
}

#[wasm_bindgen(js_name = formatDocument)]
pub fn format_document(source: &str, tab_size: u32, insert_spaces: bool, crlf: bool) -> String {
    response(
        source,
        ason_format::format_with_options(
            source,
            &FormatOptions {
                indent_width: tab_size as usize,
                indent_style: if insert_spaces {
                    IndentStyle::Spaces
                } else {
                    IndentStyle::Tabs
                },
                line_ending: if crlf {
                    LineEnding::CrLf
                } else {
                    LineEnding::Lf
                },
            },
        )
        .map(Some),
    )
}

#[wasm_bindgen(js_name = validateDocument)]
pub fn validate_document(source: &str) -> String {
    response(
        source,
        ason_format::validate_document(source).map(|()| None),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_validates() {
        let output: serde_json::Value =
            serde_json::from_str(&format_document("{a:1}", 2, true, true)).unwrap();
        assert_eq!(output["text"], "{a: 1}\r\n");
        for source in ["d\"2024-03-16T16:30:50+08:00\"", "{a:[1 2]}"] {
            assert!(validate_document(source).contains("\"status\":\"success\""));
        }
    }

    #[test]
    fn unicode_crlf_and_error_scopes() {
        let source = "{😀:1\r\n😀:2}";
        let error: serde_json::Value = serde_json::from_str(&validate_document(source)).unwrap();
        assert_eq!(error["error"]["start"], "{😀:1\r\n".encode_utf16().count());
        assert_eq!(error["error"]["end"], "{😀:1\r\n😀".encode_utf16().count());
        assert_eq!(error["error"]["kind"], "syntax");
        let error: serde_json::Value =
            serde_json::from_str(&validate_document("[1 true]")).unwrap();
        assert_eq!(error["error"]["kind"], "syntax");
        assert_eq!(error["error"]["start"], 3);
        assert_eq!(error["error"]["end"], 7);
        let source = "[😀::A(1), 😀::A(true)]";
        let error: serde_json::Value = serde_json::from_str(&validate_document(source)).unwrap();
        assert_eq!(
            error["error"]["start"],
            "[😀::A(1), ".encode_utf16().count()
        );
        assert_eq!(error["error"]["end"], source.encode_utf16().count() - 1);
        let error: serde_json::Value =
            serde_json::from_str(&validate_document("{a: true::B}")).unwrap();
        assert_eq!(
            (&error["error"]["start"], &error["error"]["end"]),
            (&4.into(), &8.into())
        );
        let error: serde_json::Value = serde_json::from_str(&validate_document("[😀::A")).unwrap();
        assert_eq!(error["error"]["kind"], "eof");
        assert_eq!(error["error"]["start"], 6);
    }
}
