//! Decoding of the function section: the type index of every function the
//! module defines itself.
//!
//! This section only says *which signature* each local function has, in
//! declaration order; the code that goes with it lives in the code section
//! and is matched up positionally, function by function, not decoded here.
//! Local function indices continue on from the last imported function index,
//! but this layer does not know how many of those there are - that lines up
//! at the `Module` level, once the import section has been decoded too.

use crate::leb::read_u32;
use crate::sections::{ParseError, ParseErrorKind};

/// Decodes the payload of a function section (id 3) into the type index of
/// each local function, in the order they appear. `base` is the offset of
/// `bytes` within the whole module, so a `ParseError` reports a position the
/// caller can find in the original file rather than one relative to the
/// section payload.
pub fn decode_function_section(bytes: &[u8], base: usize) -> Result<Vec<u32>, ParseError> {
    let mut pos = 0;
    let count = read_u32(bytes, &mut pos)
        .map_err(|_| ParseError { offset: base, kind: ParseErrorKind::Leb })? as usize;

    let mut type_indices = Vec::with_capacity(count.min(bytes.len()));
    for _ in 0..count {
        let index_offset = base + pos;
        let index = read_u32(bytes, &mut pos)
            .map_err(|_| ParseError { offset: index_offset, kind: ParseErrorKind::Leb })?;
        type_indices.push(index);
    }

    if pos != bytes.len() {
        return Err(ParseError { offset: base + pos, kind: ParseErrorKind::SectionSizeMismatch });
    }

    Ok(type_indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indices_of(bytes: &[u8]) -> Result<Vec<u32>, ParseError> {
        decode_function_section(bytes, 0)
    }

    #[test]
    fn decodes_empty_function_section() {
        assert_eq!(indices_of(&[0x00]), Ok(Vec::new()));
    }

    #[test]
    fn decodes_type_indices_in_order() {
        let bytes = [0x03, 0x00, 0x02, 0x01];
        assert_eq!(indices_of(&bytes), Ok(vec![0, 2, 1]));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let bytes = [0x00, 0xAA];
        assert_eq!(
            indices_of(&bytes),
            Err(ParseError { offset: 1, kind: ParseErrorKind::SectionSizeMismatch })
        );
    }

    #[test]
    fn rejects_truncated_function_section() {
        let bytes = [0x02, 0x00, 0x80];
        assert_eq!(
            indices_of(&bytes),
            Err(ParseError { offset: 2, kind: ParseErrorKind::Leb })
        );
    }

    #[test]
    fn reports_offsets_relative_to_the_base() {
        let bytes = [0x01, 0x80];
        assert_eq!(
            decode_function_section(&bytes, 100),
            Err(ParseError { offset: 101, kind: ParseErrorKind::Leb })
        );
    }
}
