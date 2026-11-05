// SPDX-License-Identifier: Apache-2.0
//! Text header separation tests.

use super::{is_transmit_header, split};
use crate::test_support::{parasolid::text_header, with_decode_context};

fn binary() -> Vec<u8> {
    b"PS\0\0\0\x03abc".to_vec()
}

fn run(stream: &[u8]) -> super::Transmit {
    with_decode_context(stream, |ctx| split(ctx, stream).unwrap())
}

#[test]
fn the_standard_header_gives_the_schema_and_the_binary_offset() {
    let mut stream = text_header("SCH_1700272_16100", "binary");
    let header_len = stream.len();
    stream.extend(binary());
    let transmit = run(&stream);
    assert_eq!(transmit.binary_offset, header_len);
    assert_eq!(
        transmit
            .text_schema
            .as_ref()
            .map(cadmpeg_parasolid::OwnedSchemaToken::value),
        Some("SCH_1700272_16100")
    );
    assert!(!transmit.text_encoded);
    assert_eq!(&stream[transmit.binary_offset..][..4], b"PS\0\0");
}

#[test]
fn a_line_break_after_the_end_line_is_skipped() {
    let mut stream = text_header("SCH_2201216_20000", "binary");
    stream.extend(b"\r\n");
    let start = stream.len();
    stream.extend(binary());
    assert_eq!(run(&stream).binary_offset, start);
}

#[test]
fn carriage_returns_in_the_header_lines_are_ignored() {
    let text = String::from_utf8(text_header("SCH_2201216_20000", "binary")).unwrap();
    let mut stream = text.replace('\n', "\r\n").into_bytes();
    stream.extend(binary());
    let transmit = run(&stream);
    assert_eq!(
        transmit
            .text_schema
            .as_ref()
            .map(cadmpeg_parasolid::OwnedSchemaToken::value),
        Some("SCH_2201216_20000")
    );
    assert!(!transmit.text_encoded);
    assert_eq!(&stream[transmit.binary_offset..][..4], b"PS\0\0");
}

#[test]
fn a_text_format_is_flagged_without_regard_to_case() {
    for value in ["text", "TEXT", "Text;"] {
        let mut stream = text_header("SCH_3001278_30100", value);
        stream.extend(binary());
        assert!(run(&stream).text_encoded, "{value}");
    }
}

#[test]
fn a_header_without_an_end_line_leaves_the_offset_at_zero() {
    let stream = b"**ABCDEFGHIJKLMNOPQRSTUVWXYZ\n**PART1;\nSCH=SCH_1_2\n".to_vec();
    let transmit = run(&stream);
    assert_eq!(transmit.binary_offset, 0);
    assert!(transmit.text_schema.is_none());
}

#[test]
fn an_invalid_schema_line_is_ignored() {
    let mut stream = text_header("not a token", "binary");
    stream.extend(binary());
    let transmit = run(&stream);
    assert!(transmit.text_schema.is_none());
    assert!(transmit.binary_offset > 0);
}

#[test]
fn detection_needs_the_alphabet_line_and_the_parasolid_line() {
    assert!(is_transmit_header(&text_header("SCH_1_2", "binary")));
    assert!(!is_transmit_header(
        b"**ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz\n"
    ));
    assert!(!is_transmit_header(b"**PARASOLID\n"));
    assert!(!is_transmit_header(b"PS\0\0\0\x03abc"));
    assert!(!is_transmit_header(b""));
}
