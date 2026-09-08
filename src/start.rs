//! Decoding of the start section: the function, if any, that runs
//! automatically once a module is instantiated, before the host calls
//! anything itself.
//!
//! There can be at most one start section, and its payload is nothing but a
//! single function index - `sections` already guarantees it appears at most
//! once and in the right place, so all that is left to check here is that
//! the index decodes and nothing trails after it.

use crate::leb::read_u32;
use crate::sections::{ParseError, ParseErrorKind};

/// Decodes the payload of a start section (id 8) into the index of the
/// function it names. `base` is the offset of `bytes` within the whole
/// module, so a `ParseError` reports a position the caller can find in the
/// original file rather than one relative to the section payload.
pub fn decode_start_section(bytes: &[u8], base: usize) -> Result<u32, ParseError> {
    let mut pos = 0;
    let index = read_u32(bytes, &mut pos)
        .map_err(|_| ParseError { offset: base, kind: ParseErrorKind::Leb })?;

    if pos != bytes.len() {
        return Err(ParseError { offset: base + pos, kind: ParseErrorKind::SectionSizeMismatch });
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_start_function_index() {
        assert_eq!(decode_start_section(&[0x02], 0), Ok(2));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let bytes = [0x00, 0xAA];
        assert_eq!(
            decode_start_section(&bytes, 0),
            Err(ParseError { offset: 1, kind: ParseErrorKind::SectionSizeMismatch })
        );
    }

    #[test]
    fn rejects_empty_start_section() {
        assert_eq!(decode_start_section(&[], 0), Err(ParseError { offset: 0, kind: ParseErrorKind::Leb }));
    }

    #[test]
    fn reports_offsets_relative_to_the_base() {
        let bytes = [0x00, 0xAA];
        assert_eq!(
            decode_start_section(&bytes, 100),
            Err(ParseError { offset: 101, kind: ParseErrorKind::SectionSizeMismatch })
        );
    }
}
