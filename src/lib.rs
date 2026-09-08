//! A WebAssembly binary parser and fuel-metered stack interpreter. See the
//! crate README for the full picture.
//!
//! The crate is built bottom-up: `leb` decodes the variable-length integers
//! the format is made of, `sections` frames the module header and the
//! top-level section list, and `types`, `imports`, `functions`, `exports`
//! and `start` each decode one section's payload. Nothing above that exists
//! yet - no `Module`, no code section decoding, no interpreter.

#![forbid(unsafe_code)]

pub mod exports;
pub mod functions;
pub mod imports;
pub mod leb;
pub mod sections;
pub mod start;
pub mod types;
