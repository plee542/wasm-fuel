//! Decoding of the type section: value types and function signatures.
//!
//! A function type is the byte `0x60` followed by two vectors of value
//! types, params then results. This is the first section whose payload
//! actually gets decoded rather than skipped - every later section refers
//! back into this one by index, so it has to exist before anything else can
//! be built on top of it.

use crate::leb::read_u32;
use crate::sections::{ParseError, ParseErrorKind};

/// One of the four value types the MVP instruction set operates on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
}

/// A function signature: the types of its parameters, in order, and the
/// types of its results, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

fn read_val_type(bytes: &[u8], pos: &mut usize, base: usize) -> Result<ValType, ParseError> {
    let offset = base + *pos;
    let byte = *bytes
        .get(*pos)
        .ok_or(ParseError { offset, kind: ParseErrorKind::UnexpectedEof })?;
    let val = match byte {
        0x7F => ValType::I32,
        0x7E => ValType::I64,
        0x7D => ValType::F32,
        0x7C => ValType::F64,
        _ => return Err(ParseError { offset, kind: ParseErrorKind::InvalidValType }),
    };
    *pos += 1;
    Ok(val)
}

fn read_val_type_vec(bytes: &[u8], pos: &mut usize, base: usize) -> Result<Vec<ValType>, ParseError> {
    let count_offset = base + *pos;
    let count = read_u32(bytes, pos)
        .map_err(|_| ParseError { offset: count_offset, kind: ParseErrorKind::Leb })? as usize;
    // A vector cannot actually hold more entries than there are bytes left
    // to read them from; cap the reservation so a bogus huge count does not
    // turn into an oversized allocation before the loop below rejects it.
    let mut values = Vec::with_capacity(count.min(bytes.len().saturating_sub(*pos)));
    for _ in 0..count {
        values.push(read_val_type(bytes, pos, base)?);
    }
    Ok(values)
}

/// Decodes the payload of a type section (id 1) into its function types, in
/// the order they appear. `base` is the offset of `bytes` within the whole
/// module, so a `ParseError` reports a position the caller can find in the
/// original file rather than one relative to the section payload.
pub fn decode_type_section(bytes: &[u8], base: usize) -> Result<Vec<FuncType>, ParseError> {
    let mut pos = 0;
    let count = read_u32(bytes, &mut pos)
        .map_err(|_| ParseError { offset: base, kind: ParseErrorKind::Leb })? as usize;

    let mut types = Vec::with_capacity(count.min(bytes.len()));
    for _ in 0..count {
        let form_offset = base + pos;
        let form = *bytes
            .get(pos)
            .ok_or(ParseError { offset: form_offset, kind: ParseErrorKind::UnexpectedEof })?;
        if form != 0x60 {
            return Err(ParseError { offset: form_offset, kind: ParseErrorKind::InvalidFuncType });
        }
        pos += 1;

        let params = read_val_type_vec(bytes, &mut pos, base)?;
        let results = read_val_type_vec(bytes, &mut pos, base)?;
        types.push(FuncType { params, results });
    }

    if pos != bytes.len() {
        return Err(ParseError { offset: base + pos, kind: ParseErrorKind::SectionSizeMismatch });
    }

    Ok(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types_of(bytes: &[u8]) -> Result<Vec<FuncType>, ParseError> {
        decode_type_section(bytes, 0)
    }

    #[test]
    fn decodes_empty_type_section() {
        assert_eq!(types_of(&[0x00]), Ok(Vec::new()));
    }

    #[test]
    fn decodes_a_type_with_no_params_or_results() {
        let bytes = [0x01, 0x60, 0x00, 0x00];
        assert_eq!(types_of(&bytes), Ok(vec![FuncType { params: vec![], results: vec![] }]));
    }

    #[test]
    fn decodes_params_and_results() {
        // (i32, i32) -> i32
        let bytes = [0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F];
        assert_eq!(
            types_of(&bytes),
            Ok(vec![FuncType { params: vec![ValType::I32, ValType::I32], results: vec![ValType::I32] }])
        );
    }

    #[test]
    fn decodes_multiple_types() {
        let bytes = [
            0x02, // count
            0x60, 0x00, 0x01, 0x7D, // () -> f32
            0x60, 0x01, 0x7C, 0x00, // (f64) -> ()
        ];
        assert_eq!(
            types_of(&bytes),
            Ok(vec![
                FuncType { params: vec![], results: vec![ValType::F32] },
                FuncType { params: vec![ValType::F64], results: vec![] },
            ])
        );
    }

    #[test]
    fn rejects_wrong_form_byte() {
        let bytes = [0x01, 0x61, 0x00, 0x00];
        assert_eq!(
            types_of(&bytes),
            Err(ParseError { offset: 1, kind: ParseErrorKind::InvalidFuncType })
        );
    }

    #[test]
    fn rejects_unknown_val_type() {
        let bytes = [0x01, 0x60, 0x01, 0x7B, 0x00];
        assert_eq!(
            types_of(&bytes),
            Err(ParseError { offset: 3, kind: ParseErrorKind::InvalidValType })
        );
    }

    #[test]
    fn rejects_truncated_type_section() {
        let bytes = [0x01, 0x60, 0x01, 0x7F];
        assert_eq!(
            types_of(&bytes),
            Err(ParseError { offset: 4, kind: ParseErrorKind::Leb })
        );
    }

    #[test]
    fn rejects_trailing_bytes() {
        let bytes = [0x00, 0xAA];
        assert_eq!(
            types_of(&bytes),
            Err(ParseError { offset: 1, kind: ParseErrorKind::SectionSizeMismatch })
        );
    }

    #[test]
    fn reports_offsets_relative_to_the_base() {
        let bytes = [0x01, 0x61, 0x00, 0x00];
        assert_eq!(
            decode_type_section(&bytes, 100),
            Err(ParseError { offset: 101, kind: ParseErrorKind::InvalidFuncType })
        );
    }
}
