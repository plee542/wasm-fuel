//! A WebAssembly binary parser and fuel-metered stack interpreter. See the
//! crate README for the full picture.
//!
//! The crate is built bottom-up: `leb` decodes the variable-length integers
//! the format is made of, `sections` frames the module header and the
//! top-level section list, `types` decodes the type section's function
//! signatures, and `imports` decodes the import section. Nothing above that
//! exists yet - no function/export/start decoding, no `Module`, no
//! interpreter.

#![forbid(unsafe_code)]

pub mod imports;
pub mod leb;
pub mod sections;
pub mod types;
