//! Decoding of the export section: the names an instance's host can look up,
//! and what each one refers to.
//!
//! An export's index is into whichever space its kind names - the function
//! space, the table space, and so on - and function indices there include
//! imports, counted before any function the module defines itself. This
//! layer does not check an index against how many items actually exist in
//! that space; like the function section, that check needs the rest of the
//! module assembled first.

use crate::imports::read_name;
use crate::leb::read_u32;
use crate::sections::{ParseError, ParseErrorKind};

/// Which index space an export's `index` refers into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Func,
    Table,
    Memory,
    Global,
}

/// One entry of the export section: the name a host looks it up by, and what
/// it points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

/// Decodes the payload of an export section (id 7) into its entries, in the
/// order they appear. `base` is the offset of `bytes` within the whole
/// module, so a `ParseError` reports a position the caller can find in the
/// original file rather than one relative to the section payload.
pub fn decode_export_section(bytes: &[u8], base: usize) -> Result<Vec<Export>, ParseError> {
    let mut pos = 0;
    let count = read_u32(bytes, &mut pos)
        .map_err(|_| ParseError { offset: base, kind: ParseErrorKind::Leb })? as usize;

    let mut exports = Vec::with_capacity(count.min(bytes.len()));
    for _ in 0..count {
        let name = read_name(bytes, &mut pos, base)?;

        let kind_offset = base + pos;
        let kind_byte = *bytes
            .get(pos)
            .ok_or(ParseError { offset: kind_offset, kind: ParseErrorKind::UnexpectedEof })?;
        pos += 1;
        let kind = match kind_byte {
            0x00 => ExportKind::Func,
            0x01 => ExportKind::Table,
            0x02 => ExportKind::Memory,
            0x03 => ExportKind::Global,
            _ => return Err(ParseError { offset: kind_offset, kind: ParseErrorKind::InvalidExternKind }),
        };

        let index_offset = base + pos;
        let index = read_u32(bytes, &mut pos)
            .map_err(|_| ParseError { offset: index_offset, kind: ParseErrorKind::Leb })?;

        exports.push(Export { name, kind, index });
    }

    if pos != bytes.len() {
        return Err(ParseError { offset: base + pos, kind: ParseErrorKind::SectionSizeMismatch });
    }

    Ok(exports)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exports_of(bytes: &[u8]) -> Result<Vec<Export>, ParseError> {
        decode_export_section(bytes, 0)
    }

    fn name_bytes(s: &str) -> Vec<u8> {
        let mut out = vec![s.len() as u8];
        out.extend_from_slice(s.as_bytes());
        out
    }

    #[test]
    fn decodes_empty_export_section() {
        assert_eq!(exports_of(&[0x00]), Ok(Vec::new()));
    }

    #[test]
    fn decodes_a_func_export() {
        let mut bytes = vec![0x01]; // count
        bytes.extend(name_bytes("run"));
        bytes.extend_from_slice(&[0x00, 0x02]); // func export, index 2
        assert_eq!(
            exports_of(&bytes),
            Ok(vec![Export { name: "run".to_string(), kind: ExportKind::Func, index: 2 }])
        );
    }

    #[test]
    fn decodes_every_export_kind() {
        let mut bytes = vec![0x04];
        bytes.extend(name_bytes("t"));
        bytes.extend_from_slice(&[0x01, 0x00]);
        bytes.extend(name_bytes("m"));
        bytes.extend_from_slice(&[0x02, 0x00]);
        bytes.extend(name_bytes("g"));
        bytes.extend_from_slice(&[0x03, 0x00]);
        bytes.extend(name_bytes("f"));
        bytes.extend_from_slice(&[0x00, 0x00]);
        let exports = exports_of(&bytes).unwrap();
        assert_eq!(
            exports.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![ExportKind::Table, ExportKind::Memory, ExportKind::Global, ExportKind::Func]
        );
    }

    #[test]
    fn decodes_multiple_exports_in_order() {
        let mut bytes = vec![0x02];
        bytes.extend(name_bytes("a"));
        bytes.extend_from_slice(&[0x00, 0x00]);
        bytes.extend(name_bytes("b"));
        bytes.extend_from_slice(&[0x00, 0x01]);
        let exports = exports_of(&bytes).unwrap();
        assert_eq!(exports[0].name, "a");
        assert_eq!(exports[1].name, "b");
    }

    #[test]
    fn rejects_unknown_export_kind() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("x"));
        let offset = bytes.len();
        bytes.push(0x04); // not a valid kind byte
        assert_eq!(
            exports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::InvalidExternKind })
        );
    }

    #[test]
    fn rejects_invalid_utf8_name() {
        let mut bytes = vec![0x01, 0x01, 0xFF]; // name: one byte, not valid UTF-8
        let offset = bytes.len() - 1;
        bytes.extend_from_slice(&[0x00, 0x00]);
        assert_eq!(
            exports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::InvalidUtf8 })
        );
    }

    #[test]
    fn rejects_trailing_bytes() {
        let bytes = [0x00, 0xAA];
        assert_eq!(
            exports_of(&bytes),
            Err(ParseError { offset: 1, kind: ParseErrorKind::SectionSizeMismatch })
        );
    }

    #[test]
    fn rejects_truncated_export_section() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("run"));
        let offset = bytes.len();
        assert_eq!(
            exports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::UnexpectedEof })
        );
    }

    #[test]
    fn reports_offsets_relative_to_the_base() {
        let bytes = [0x01, 0x01, 0xFF];
        assert_eq!(
            decode_export_section(&bytes, 100),
            Err(ParseError { offset: 102, kind: ParseErrorKind::InvalidUtf8 })
        );
    }
}
