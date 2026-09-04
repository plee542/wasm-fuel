//! Decoding of the import section: the module/name pairs an instance expects
//! its host to supply, and what shape each one must have.
//!
//! Function imports occupy the low function indices, ahead of any function
//! defined by the module itself, so this has to be decoded before the
//! function or code sections can be indexed correctly. Table, memory and
//! global imports are decoded too even though this crate never instantiates
//! them, because getting their payload length wrong would misalign every
//! import that follows.

use crate::leb::read_u32;
use crate::sections::{ParseError, ParseErrorKind};
use crate::types::{read_val_type, ValType};

/// A table or memory's size bounds, as a count of elements or pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

/// A table import's element type and size bounds. The element type is
/// always `funcref` in the instruction set this crate targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableType {
    pub limits: Limits,
}

/// A global import's value type and whether the host may write to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalType {
    pub val_type: ValType,
    pub mutable: bool,
}

/// What kind of item an import binds to, and the type information needed to
/// check it against whatever the host provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternKind {
    Func(u32),
    Table(TableType),
    Memory(Limits),
    Global(GlobalType),
}

/// One entry of the import section: the host module and field it is bound
/// to, and what it must look like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub kind: ExternKind,
}

fn read_name(bytes: &[u8], pos: &mut usize, base: usize) -> Result<String, ParseError> {
    let len_offset = base + *pos;
    let len = read_u32(bytes, pos)
        .map_err(|_| ParseError { offset: len_offset, kind: ParseErrorKind::Leb })? as usize;
    let start_offset = base + *pos;
    let raw = bytes
        .get(*pos..*pos + len)
        .ok_or(ParseError { offset: start_offset, kind: ParseErrorKind::UnexpectedEof })?;
    let name = std::str::from_utf8(raw)
        .map_err(|_| ParseError { offset: start_offset, kind: ParseErrorKind::InvalidUtf8 })?
        .to_string();
    *pos += len;
    Ok(name)
}

fn read_limits(bytes: &[u8], pos: &mut usize, base: usize) -> Result<Limits, ParseError> {
    let flag_offset = base + *pos;
    let flag = *bytes
        .get(*pos)
        .ok_or(ParseError { offset: flag_offset, kind: ParseErrorKind::UnexpectedEof })?;
    *pos += 1;
    if flag > 1 {
        return Err(ParseError { offset: flag_offset, kind: ParseErrorKind::InvalidLimits });
    }

    let min_offset = base + *pos;
    let min = read_u32(bytes, pos)
        .map_err(|_| ParseError { offset: min_offset, kind: ParseErrorKind::Leb })?;

    let max = if flag == 1 {
        let max_offset = base + *pos;
        let max = read_u32(bytes, pos)
            .map_err(|_| ParseError { offset: max_offset, kind: ParseErrorKind::Leb })?;
        if max < min {
            return Err(ParseError { offset: max_offset, kind: ParseErrorKind::InvalidLimits });
        }
        Some(max)
    } else {
        None
    };

    Ok(Limits { min, max })
}

fn read_table_type(bytes: &[u8], pos: &mut usize, base: usize) -> Result<TableType, ParseError> {
    let elem_offset = base + *pos;
    let elem = *bytes
        .get(*pos)
        .ok_or(ParseError { offset: elem_offset, kind: ParseErrorKind::UnexpectedEof })?;
    if elem != 0x70 {
        return Err(ParseError { offset: elem_offset, kind: ParseErrorKind::InvalidExternKind });
    }
    *pos += 1;
    Ok(TableType { limits: read_limits(bytes, pos, base)? })
}

fn read_global_type(bytes: &[u8], pos: &mut usize, base: usize) -> Result<GlobalType, ParseError> {
    let val_type = read_val_type(bytes, pos, base)?;
    let mut_offset = base + *pos;
    let mutability = *bytes
        .get(*pos)
        .ok_or(ParseError { offset: mut_offset, kind: ParseErrorKind::UnexpectedEof })?;
    let mutable = match mutability {
        0x00 => false,
        0x01 => true,
        _ => return Err(ParseError { offset: mut_offset, kind: ParseErrorKind::InvalidExternKind }),
    };
    *pos += 1;
    Ok(GlobalType { val_type, mutable })
}

/// Decodes the payload of an import section (id 2) into its entries, in the
/// order they appear. `base` is the offset of `bytes` within the whole
/// module, so a `ParseError` reports a position the caller can find in the
/// original file rather than one relative to the section payload.
pub fn decode_import_section(bytes: &[u8], base: usize) -> Result<Vec<Import>, ParseError> {
    let mut pos = 0;
    let count = read_u32(bytes, &mut pos)
        .map_err(|_| ParseError { offset: base, kind: ParseErrorKind::Leb })? as usize;

    let mut imports = Vec::with_capacity(count.min(bytes.len()));
    for _ in 0..count {
        let module = read_name(bytes, &mut pos, base)?;
        let name = read_name(bytes, &mut pos, base)?;

        let kind_offset = base + pos;
        let kind_byte = *bytes
            .get(pos)
            .ok_or(ParseError { offset: kind_offset, kind: ParseErrorKind::UnexpectedEof })?;
        pos += 1;
        let kind = match kind_byte {
            0x00 => {
                let type_offset = base + pos;
                let index = read_u32(bytes, &mut pos)
                    .map_err(|_| ParseError { offset: type_offset, kind: ParseErrorKind::Leb })?;
                ExternKind::Func(index)
            }
            0x01 => ExternKind::Table(read_table_type(bytes, &mut pos, base)?),
            0x02 => ExternKind::Memory(read_limits(bytes, &mut pos, base)?),
            0x03 => ExternKind::Global(read_global_type(bytes, &mut pos, base)?),
            _ => return Err(ParseError { offset: kind_offset, kind: ParseErrorKind::InvalidExternKind }),
        };

        imports.push(Import { module, name, kind });
    }

    if pos != bytes.len() {
        return Err(ParseError { offset: base + pos, kind: ParseErrorKind::SectionSizeMismatch });
    }

    Ok(imports)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imports_of(bytes: &[u8]) -> Result<Vec<Import>, ParseError> {
        decode_import_section(bytes, 0)
    }

    fn name_bytes(s: &str) -> Vec<u8> {
        let mut out = vec![s.len() as u8];
        out.extend_from_slice(s.as_bytes());
        out
    }

    #[test]
    fn decodes_empty_import_section() {
        assert_eq!(imports_of(&[0x00]), Ok(Vec::new()));
    }

    #[test]
    fn decodes_a_func_import() {
        let mut bytes = vec![0x01]; // count
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("add"));
        bytes.extend_from_slice(&[0x00, 0x02]); // func import, type index 2
        assert_eq!(
            imports_of(&bytes),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "add".to_string(),
                kind: ExternKind::Func(2),
            }])
        );
    }

    #[test]
    fn decodes_a_table_import() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("t"));
        bytes.extend_from_slice(&[0x01, 0x70, 0x01, 0x02, 0x08]); // table, funcref, min 2, max 8
        assert_eq!(
            imports_of(&bytes),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "t".to_string(),
                kind: ExternKind::Table(TableType { limits: Limits { min: 2, max: Some(8) } }),
            }])
        );
    }

    #[test]
    fn decodes_a_memory_import_without_a_max() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("mem"));
        bytes.extend_from_slice(&[0x02, 0x00, 0x01]); // memory, min 1, no max
        assert_eq!(
            imports_of(&bytes),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "mem".to_string(),
                kind: ExternKind::Memory(Limits { min: 1, max: None }),
            }])
        );
    }

    #[test]
    fn decodes_a_mutable_global_import() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("g"));
        bytes.extend_from_slice(&[0x03, 0x7F, 0x01]); // global, i32, mutable
        assert_eq!(
            imports_of(&bytes),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "g".to_string(),
                kind: ExternKind::Global(GlobalType { val_type: ValType::I32, mutable: true }),
            }])
        );
    }

    #[test]
    fn decodes_multiple_imports_in_order() {
        let mut bytes = vec![0x02];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("a"));
        bytes.extend_from_slice(&[0x00, 0x00]);
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("b"));
        bytes.extend_from_slice(&[0x00, 0x01]);
        let imports = imports_of(&bytes).unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].kind, ExternKind::Func(0));
        assert_eq!(imports[1].kind, ExternKind::Func(1));
    }

    #[test]
    fn rejects_unknown_extern_kind() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("x"));
        let offset = bytes.len();
        bytes.push(0x04); // not a valid kind byte
        assert_eq!(
            imports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::InvalidExternKind })
        );
    }

    #[test]
    fn rejects_invalid_utf8_name() {
        let mut bytes = vec![0x01, 0x01, 0xFF]; // module name: one byte, not valid UTF-8
        let offset = bytes.len() - 1;
        bytes.extend(name_bytes("x"));
        bytes.extend_from_slice(&[0x00, 0x00]);
        assert_eq!(
            imports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::InvalidUtf8 })
        );
    }

    #[test]
    fn rejects_bad_limits_flag() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("mem"));
        bytes.push(0x02);
        let offset = bytes.len();
        bytes.push(0x02); // flag must be 0 or 1
        assert_eq!(
            imports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::InvalidLimits })
        );
    }

    #[test]
    fn rejects_max_below_min() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("mem"));
        bytes.extend_from_slice(&[0x02, 0x01, 0x05]);
        let offset = bytes.len();
        bytes.push(0x02); // max 2 < min 5
        assert_eq!(
            imports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::InvalidLimits })
        );
    }

    #[test]
    fn rejects_non_funcref_table_element_type() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        bytes.extend(name_bytes("t"));
        bytes.push(0x01);
        let offset = bytes.len();
        bytes.extend_from_slice(&[0x6F, 0x00, 0x00]); // externref, not funcref
        assert_eq!(
            imports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::InvalidExternKind })
        );
    }

    #[test]
    fn rejects_trailing_bytes() {
        let bytes = [0x00, 0xAA];
        assert_eq!(
            imports_of(&bytes),
            Err(ParseError { offset: 1, kind: ParseErrorKind::SectionSizeMismatch })
        );
    }

    #[test]
    fn rejects_truncated_import_section() {
        let mut bytes = vec![0x01];
        bytes.extend(name_bytes("env"));
        let offset = bytes.len();
        assert_eq!(
            imports_of(&bytes),
            Err(ParseError { offset, kind: ParseErrorKind::Leb })
        );
    }

    #[test]
    fn reports_offsets_relative_to_the_base() {
        let bytes = [0x01, 0x01, 0xFF];
        assert_eq!(
            decode_import_section(&bytes, 100),
            Err(ParseError { offset: 102, kind: ParseErrorKind::InvalidUtf8 })
        );
    }
}
