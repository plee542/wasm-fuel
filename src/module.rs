//! The top-level `Module`: the point where the sections decoded so far get
//! tied together and checked against each other.
//!
//! Each section decoder only validates its own payload - `imports` says
//! outright that it cannot check a function import's type index against how
//! many types actually exist, because the type section might not even have
//! been decoded yet at that point. This is that later point: `parse` decodes
//! the sections this crate currently understands, then checks the type
//! indices that cross between them.
//!
//! What this does not check yet is whether a start or export function index
//! actually names a function: that space also includes function bodies this
//! crate cannot decode yet, and a bad index there is meant to surface as
//! `Trap::UndefinedFunction` when something tries to call it, not as a parse
//! failure. The table, memory, global, element, data and data-count sections
//! are not decoded at all yet - `sections` has already checked they are
//! framed correctly, so a module containing them still parses.

use crate::exports::{decode_export_section, Export};
use crate::functions::decode_function_section;
use crate::imports::{decode_import_section, ExternKind, Import};
use crate::leb::read_u32;
use crate::sections::{parse_module_sections, ParseError, ParseErrorKind};
use crate::start::decode_start_section;
use crate::types::{decode_type_section, FuncType};

/// A decoded module: the sections this crate currently understands, checked
/// against each other. Sections it does not decode yet (table, memory,
/// global, element, code, data, data count) are accepted by the framing pass
/// but do not appear here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    /// The type index of each function the module defines itself, in
    /// declaration order. Function indices in `start` and in `exports`
    /// continue on from the imported functions, i.e. index `imports.len()`
    /// worth of `Func` imports names `function_types[0]`.
    pub function_types: Vec<u32>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
}

fn offset_of(module: &[u8], section: &[u8]) -> usize {
    section.as_ptr() as usize - module.as_ptr() as usize
}

/// Reads only the leading count of a section that is otherwise not decoded
/// yet, so the function/code count check below does not need a real code
/// section decoder.
fn peek_vector_count(bytes: &[u8], base: usize) -> Result<u32, ParseError> {
    let mut pos = 0;
    read_u32(bytes, &mut pos).map_err(|_| ParseError { offset: base, kind: ParseErrorKind::Leb })
}

/// Parses a complete module: validates the header and section framing,
/// decodes every section this crate understands, and checks the type
/// indices between them.
pub fn parse(bytes: &[u8]) -> Result<Module, ParseError> {
    let raw_sections = parse_module_sections(bytes)?;

    let mut types = Vec::new();
    let mut imports = Vec::new();
    let mut function_types = Vec::new();
    let mut exports = Vec::new();
    let mut start = None;

    let mut import_section_base = None;
    let mut function_section_base = None;
    let mut code_section_base = None;
    let mut code_function_count = None;

    for section in &raw_sections {
        let base = offset_of(bytes, section.bytes);
        match section.id {
            0 => {} // custom: opaque to every layer of this crate
            1 => types = decode_type_section(section.bytes, base)?,
            2 => {
                import_section_base = Some(base);
                imports = decode_import_section(section.bytes, base)?;
            }
            3 => {
                function_section_base = Some(base);
                function_types = decode_function_section(section.bytes, base)?;
            }
            7 => exports = decode_export_section(section.bytes, base)?,
            8 => start = Some(decode_start_section(section.bytes, base)?),
            10 => {
                code_section_base = Some(base);
                code_function_count = Some(peek_vector_count(section.bytes, base)?);
            }
            _ => {} // table, memory, global, element, data, data count: not decoded yet
        }
    }

    for import in &imports {
        if let ExternKind::Func(type_index) = import.kind {
            if type_index as usize >= types.len() {
                return Err(ParseError {
                    offset: import_section_base.unwrap_or(0),
                    kind: ParseErrorKind::TypeIndexOutOfRange,
                });
            }
        }
    }

    for &type_index in &function_types {
        if type_index as usize >= types.len() {
            return Err(ParseError {
                offset: function_section_base.unwrap_or(0),
                kind: ParseErrorKind::TypeIndexOutOfRange,
            });
        }
    }

    let declared_code_count = code_function_count.unwrap_or(0) as usize;
    if declared_code_count != function_types.len() {
        return Err(ParseError {
            offset: code_section_base.or(function_section_base).unwrap_or(0),
            kind: ParseErrorKind::FunctionCodeMismatch,
        });
    }

    Ok(Module { types, imports, function_types, exports, start })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    fn name_bytes(s: &str) -> Vec<u8> {
        let mut out = vec![s.len() as u8];
        out.extend_from_slice(s.as_bytes());
        out
    }

    #[test]
    fn parses_a_module_with_no_sections() {
        let module = parse(&HEADER).unwrap();
        assert_eq!(module, Module {
            types: vec![],
            imports: vec![],
            function_types: vec![],
            exports: vec![],
            start: None,
        });
    }

    #[test]
    fn parses_a_function_matched_with_its_code_body() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type: () -> ()
        bytes.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]); // function: one, type 0
        bytes.extend_from_slice(&[0x0A, 0x04, 0x01, 0x02, 0x00, 0x0B]); // code: one body

        let module = parse(&bytes).unwrap();
        assert_eq!(module.function_types, vec![0]);
    }

    #[test]
    fn accepts_a_module_with_undecoded_sections() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x04, 0x04, 0x01, 0x70, 0x00, 0x00]); // table: funcref, min 0
        bytes.extend_from_slice(&[0x05, 0x03, 0x01, 0x00, 0x01]); // memory: min 1

        let module = parse(&bytes).unwrap();
        assert_eq!(module.types, Vec::new());
    }

    #[test]
    fn accepts_a_start_index_naming_an_imported_function() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type: () -> ()
        let mut import_bytes = vec![0x01];
        import_bytes.extend(name_bytes("env"));
        import_bytes.extend(name_bytes("f"));
        import_bytes.extend_from_slice(&[0x00, 0x00]); // func import, type 0
        bytes.push(0x02);
        bytes.push(import_bytes.len() as u8);
        bytes.extend(import_bytes);
        bytes.extend_from_slice(&[0x08, 0x01, 0x00]); // start: function 0, the import

        let module = parse(&bytes).unwrap();
        assert_eq!(module.start, Some(0));
    }

    #[test]
    fn rejects_import_type_index_out_of_range() {
        let mut bytes = HEADER.to_vec();
        let mut import_bytes = vec![0x01];
        import_bytes.extend(name_bytes("env"));
        import_bytes.extend(name_bytes("f"));
        import_bytes.extend_from_slice(&[0x00, 0x00]); // func import, type 0 - none exist
        bytes.push(0x02); // import section id
        bytes.push(import_bytes.len() as u8); // size
        let import_section_offset = bytes.len();
        bytes.extend(import_bytes);

        assert_eq!(
            parse(&bytes),
            Err(ParseError { offset: import_section_offset, kind: ParseErrorKind::TypeIndexOutOfRange })
        );
    }

    #[test]
    fn rejects_function_type_index_out_of_range() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x01, 0x00]); // type section: zero types
        bytes.push(0x03); // function section id
        bytes.push(0x02); // size
        let function_section_offset = bytes.len();
        bytes.extend_from_slice(&[0x01, 0x00]); // one function, type index 0 - none exist

        assert_eq!(
            parse(&bytes),
            Err(ParseError { offset: function_section_offset, kind: ParseErrorKind::TypeIndexOutOfRange })
        );
    }

    #[test]
    fn rejects_function_section_without_a_matching_code_section() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type: () -> ()
        bytes.push(0x03); // function section id
        bytes.push(0x02); // size
        let function_section_offset = bytes.len();
        bytes.extend_from_slice(&[0x01, 0x00]); // one function, type 0, no code section

        assert_eq!(
            parse(&bytes),
            Err(ParseError { offset: function_section_offset, kind: ParseErrorKind::FunctionCodeMismatch })
        );
    }

    #[test]
    fn rejects_code_section_with_a_different_count_than_the_function_section() {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type: () -> ()
        bytes.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]); // function: one, type 0
        bytes.push(0x0A); // code section id
        bytes.push(0x01); // size
        let code_section_offset = bytes.len();
        bytes.push(0x00); // zero bodies

        assert_eq!(
            parse(&bytes),
            Err(ParseError { offset: code_section_offset, kind: ParseErrorKind::FunctionCodeMismatch })
        );
    }

    #[test]
    fn accepts_an_export_naming_a_function_index_it_cannot_yet_check() {
        // Export index validity is left to the interpreter (`Trap::UndefinedFunction`),
        // not to parsing - this crate has no way to check it correctly yet since
        // it does not decode code bodies.
        let mut bytes = HEADER.to_vec();
        let mut export_bytes = vec![0x01];
        export_bytes.extend(name_bytes("run"));
        export_bytes.extend_from_slice(&[0x00, 0x00]); // func export, index 0 - none exist
        bytes.push(0x07);
        bytes.push(export_bytes.len() as u8);
        bytes.extend(export_bytes);

        let module = parse(&bytes).unwrap();
        assert_eq!(module.exports.len(), 1);
    }
}
