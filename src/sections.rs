//! Binary format framing: the module header and the sequence of sections
//! that follow it.
//!
//! This layer does not know what a type or an export is. It checks the
//! magic number and version, then walks the section list making sure every
//! section id is one the format defines, that sections appear in the order
//! the specification requires, and that each section's declared size
//! actually fits in what is left of the input. What is inside a section's
//! payload is decoded by whatever layer is built on top of this one.
//!
//! ```
//! use wasm_fuel::sections::parse_module_sections;
//!
//! // Header only: a module with no sections at all.
//! let bytes = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
//! assert!(parse_module_sections(&bytes).unwrap().is_empty());
//! ```

use crate::leb::read_u32;

const MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];
const VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

// Non-custom sections must appear in this order, each at most once. The
// data-count section (id 12) was added after the others by the bulk-memory
// proposal but sits between element and code, not after data, so it cannot
// simply be numeric-sorted alongside the rest.
const SECTION_ORDER: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 10, 11];

/// Why a module's binary framing could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// The first four bytes are not `\0asm`.
    NotWasm,
    /// The version field is not `1`, the only version this crate understands.
    UnsupportedVersion,
    /// The input ended where more bytes were expected.
    UnexpectedEof,
    /// A section's size, encoded as LEB128, was malformed.
    Leb,
    /// A section id this format does not define.
    UnknownSectionId,
    /// Sections did not appear in the order the specification requires.
    SectionOutOfOrder,
    /// A section's declared size claims more bytes than remain in the input.
    SectionSizeMismatch,
    /// A byte where a value type was expected is not one of `i32`/`i64`/
    /// `f32`/`f64`.
    InvalidValType,
    /// A function type did not start with the `0x60` form byte.
    InvalidFuncType,
}

/// A parse failure, with the byte offset that caused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub kind: ParseErrorKind,
}

/// One section as found by the framing pass: an id and its raw payload
/// bytes, not yet decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawSection<'a> {
    pub id: u8,
    pub bytes: &'a [u8],
}

/// Validates the module header and splits the rest of `bytes` into raw
/// sections, in the order they appear.
pub fn parse_module_sections(bytes: &[u8]) -> Result<Vec<RawSection<'_>>, ParseError> {
    if bytes.get(0..4) != Some(&MAGIC[..]) {
        return Err(ParseError { offset: 0, kind: ParseErrorKind::NotWasm });
    }
    let version = bytes.get(4..8).ok_or(ParseError {
        offset: bytes.len(),
        kind: ParseErrorKind::UnexpectedEof,
    })?;
    if version != &VERSION[..] {
        return Err(ParseError { offset: 4, kind: ParseErrorKind::UnsupportedVersion });
    }

    let mut pos = 8;
    // Position within SECTION_ORDER of the last non-custom section seen, or
    // -1 before any has been. Custom sections do not participate.
    let mut last_order: i32 = -1;
    let mut sections = Vec::new();

    while pos < bytes.len() {
        let id_offset = pos;
        let id = bytes[pos];
        pos += 1;

        let size_offset = pos;
        let size = read_u32(bytes, &mut pos)
            .map_err(|_| ParseError { offset: size_offset, kind: ParseErrorKind::Leb })?
            as usize;

        let payload_start = pos;
        let payload_end = payload_start
            .checked_add(size)
            .filter(|&end| end <= bytes.len())
            .ok_or(ParseError { offset: payload_start, kind: ParseErrorKind::SectionSizeMismatch })?;

        if id != 0 {
            let position = SECTION_ORDER
                .iter()
                .position(|&candidate| candidate == id)
                .ok_or(ParseError { offset: id_offset, kind: ParseErrorKind::UnknownSectionId })?
                as i32;
            if position <= last_order {
                return Err(ParseError { offset: id_offset, kind: ParseErrorKind::SectionOutOfOrder });
            }
            last_order = position;
        }

        sections.push(RawSection { id, bytes: &bytes[payload_start..payload_end] });
        pos = payload_end;
    }

    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    #[test]
    fn empty_module_has_no_sections() {
        assert_eq!(parse_module_sections(&HEADER), Ok(Vec::new()));
    }

    #[test]
    fn rejects_wrong_magic() {
        let bytes = [0x00, 0x61, 0x73, 0x6E, 0x01, 0x00, 0x00, 0x00]; // "asn", not "asm"
        assert_eq!(
            parse_module_sections(&bytes),
            Err(ParseError { offset: 0, kind: ParseErrorKind::NotWasm })
        );
    }

    #[test]
    fn rejects_input_too_short_for_magic() {
        assert_eq!(
            parse_module_sections(&HEADER[..3]),
            Err(ParseError { offset: 0, kind: ParseErrorKind::NotWasm })
        );
    }

    #[test]
    fn rejects_input_too_short_for_version() {
        assert_eq!(
            parse_module_sections(&HEADER[..6]),
            Err(ParseError { offset: 6, kind: ParseErrorKind::UnexpectedEof })
        );
    }

    #[test]
    fn rejects_unsupported_version() {
        let bytes = [0x00, 0x61, 0x73, 0x6D, 0x02, 0x00, 0x00, 0x00];
        assert_eq!(
            parse_module_sections(&bytes),
            Err(ParseError { offset: 4, kind: ParseErrorKind::UnsupportedVersion })
        );
    }

    #[test]
    fn rejects_unknown_section_id() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x0D, 0x00]); // id 13 does not exist, size 0
        assert_eq!(
            parse_module_sections(&bytes),
            Err(ParseError { offset: 8, kind: ParseErrorKind::UnknownSectionId })
        );
    }

    #[test]
    fn rejects_sections_out_of_order() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x07, 0x00]); // export, empty
        bytes.extend_from_slice(&[0x01, 0x00]); // type, after export: too late
        assert_eq!(
            parse_module_sections(&bytes),
            Err(ParseError { offset: 10, kind: ParseErrorKind::SectionOutOfOrder })
        );
    }

    #[test]
    fn rejects_duplicate_sections() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x00]); // type
        bytes.extend_from_slice(&[0x01, 0x00]); // type again
        assert_eq!(
            parse_module_sections(&bytes),
            Err(ParseError { offset: 10, kind: ParseErrorKind::SectionOutOfOrder })
        );
    }

    #[test]
    fn allows_repeated_custom_sections_anywhere() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x00, 0x00]); // custom, empty
        bytes.extend_from_slice(&[0x01, 0x00]); // type
        bytes.extend_from_slice(&[0x00, 0x00]); // custom again, after type: fine
        let sections = parse_module_sections(&bytes).unwrap();
        assert_eq!(sections.iter().map(|s| s.id).collect::<Vec<_>>(), vec![0, 1, 0]);
    }

    #[test]
    fn rejects_section_size_past_end_of_input() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x05, 0xAA, 0xAA]); // claims 5 bytes, only 2 present
        assert_eq!(
            parse_module_sections(&bytes),
            Err(ParseError { offset: 10, kind: ParseErrorKind::SectionSizeMismatch })
        );
    }

    #[test]
    fn rejects_malformed_section_size() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x80]); // continuation bit set, then nothing
        assert_eq!(
            parse_module_sections(&bytes),
            Err(ParseError { offset: 9, kind: ParseErrorKind::Leb })
        );
    }

    #[test]
    fn captures_section_payload_bytes() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x03, 0xAA, 0xBB, 0xCC]); // type, 3 bytes of payload
        let sections = parse_module_sections(&bytes).unwrap();
        assert_eq!(sections, vec![RawSection { id: 1, bytes: &[0xAA, 0xBB, 0xCC] }]);
    }
}
