//! A WebAssembly binary parser and fuel-metered stack interpreter. See the
//! crate README for the full picture.
//!
//! The crate is built bottom-up: `leb` decodes the variable-length integers
//! the format is made of, `sections` frames the module header and the
//! top-level section list, `types`, `imports`, `functions`, `exports` and
//! `start` each decode one section's payload, and `module` ties those
//! sections together into a `Module`, checking the references between them.
//! The code section is not decoded yet - `module` only checks that its
//! declared function count matches the function section's - and there is no
//! interpreter yet either.

#![forbid(unsafe_code)]

pub mod exports;
pub mod functions;
pub mod imports;
pub mod leb;
pub mod module;
pub mod sections;
pub mod start;
pub mod types;
