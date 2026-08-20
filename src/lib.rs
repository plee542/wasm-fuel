//! A WebAssembly binary parser and fuel-metered stack interpreter. See the
//! crate README for the full picture.
//!
//! The crate is built bottom-up: `leb` decodes the variable-length integers
//! the format is made of, `sections` frames the module header and the
//! top-level section list. Nothing above section framing exists yet - no
//! type/import/export decoding, no `Module`, no interpreter.

#![forbid(unsafe_code)]

pub mod leb;
pub mod sections;
