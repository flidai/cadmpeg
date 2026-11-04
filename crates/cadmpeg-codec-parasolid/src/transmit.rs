// SPDX-License-Identifier: Apache-2.0
//! The text header of a Parasolid transmit file.
//!
//! A Parasolid transmit file, as written to an `.x_t` or `.x_b` file, starts
//! with a block of text lines that each begin with `**`: a line of the
//! alphabet, a `**PARASOLID` line, and the parts `**PART1;`, `**PART2;`, and
//! `**PART3;`. Part 2 holds the line `SCH=<schema token>`. Part 1 can hold the
//! line `FORMAT=<value>`. The line `**END_OF_HEADER` followed by asterisks ends
//! the block, and the transmit data starts after it. The layout is the transmit
//! file header of the public Siemens *Parasolid XT Format Reference*.
//!
//! [`split`] separates the header from the data. It reads the schema token and
//! the encoding from the text and leaves the binary data to the shared
//! Parasolid decoder.

use cadmpeg_core::decode::{u64_from_index, DecodeContext};
use cadmpeg_core::CodecError;
use cadmpeg_parasolid::OwnedSchemaToken;

/// First line of every transmit header, up to its trailing asterisks.
const ALPHABET_LINE: &[u8] = b"**ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Start of the second header line.
const PARASOLID_LINE: &[u8] = b"**PARASOLID";

/// How far into the prefix the second header line can start.
const PARASOLID_LINE_LIMIT: usize = 256;

/// Line that ends the text header, up to its trailing asterisks.
const END_MARKER: &[u8] = b"**END_OF_HEADER";

/// Upper bound on the text header. The block is a few hundred bytes.
const TEXT_LIMIT: usize = 4096;

/// How far past the header end the binary signature may start.
const SIGNATURE_SLACK: usize = 16;

/// Signature of the binary transmit data.
const SIGNATURE: &[u8] = b"PS\0\0";

/// Whether `prefix` starts with the first two lines of a transmit header.
pub(crate) fn is_transmit_header(prefix: &[u8]) -> bool {
    prefix.starts_with(ALPHABET_LINE)
        && cadmpeg_core::bytes::find(
            &prefix[..prefix.len().min(PARASOLID_LINE_LIMIT)],
            PARASOLID_LINE,
        )
        .is_some()
}

/// The result of separating the text header from the transmit data.
#[derive(Debug)]
pub(crate) struct Transmit {
    /// Offset of the binary part in the stream. It is 0 when the header does
    /// not reach its end line or the binary signature does not follow it.
    pub(crate) binary_offset: usize,
    /// The schema token of the `SCH=` line, when the line is present and valid.
    pub(crate) text_schema: Option<OwnedSchemaToken>,
    /// Whether the `FORMAT=` line declares a text encoding of the data.
    pub(crate) text_encoded: bool,
}

/// Separate the text header from the transmit data of `stream`.
///
/// A header that never reaches its end line, or whose end line is not followed
/// by the binary signature, keeps `binary_offset` at 0. The header reader of the
/// decoder then finds no binary header and the caller reports it.
pub(crate) fn split(ctx: &DecodeContext<'_>, stream: &[u8]) -> Result<Transmit, CodecError> {
    let mut transmit = Transmit {
        binary_offset: 0,
        text_schema: None,
        text_encoded: false,
    };
    let window = &stream[..stream.len().min(TEXT_LIMIT)];
    ctx.charge_work(u64_from_index(window.len()), "scan Parasolid text header")?;
    let Some(end) = cadmpeg_core::bytes::find(window, END_MARKER) else {
        return Ok(transmit);
    };
    for line in window[..end].split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(value) = line.strip_prefix(b"SCH=") {
            let value = std::str::from_utf8(value)
                .ok()
                .map(|text| text.trim().trim_end_matches(';').trim());
            if let Some(value) = value.filter(|text| !text.is_empty()) {
                let owned = ctx.copy_retained_text(value, "retain Parasolid text schema")?;
                transmit.text_schema = OwnedSchemaToken::try_from(owned).ok();
            }
        } else if let Some(value) = line.strip_prefix(b"FORMAT=") {
            let value = value.trim_ascii().trim_ascii_end();
            let value = value.strip_suffix(b";").unwrap_or(value);
            transmit.text_encoded = value.eq_ignore_ascii_case(b"text");
        }
    }
    let mut cursor = end + END_MARKER.len();
    while stream.get(cursor) == Some(&b'*') {
        cursor += 1;
    }
    while matches!(stream.get(cursor), Some(b'\r' | b'\n')) {
        cursor += 1;
    }
    let tail_end = stream.len().min(cursor + SIGNATURE_SLACK + SIGNATURE.len());
    if let Some(offset) =
        cadmpeg_core::bytes::find(&stream[cursor.min(tail_end)..tail_end], SIGNATURE)
    {
        transmit.binary_offset = cursor + offset;
    }
    Ok(transmit)
}

#[cfg(test)]
mod tests;
