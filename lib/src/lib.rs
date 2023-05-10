//! Parse, apply and revert UPS patches.
//!
//! ## Note
//! This crate was not designed to handle large files, it reads entire files into memory at once
//! and keeps this data around to apply patches.
//!
//! ## Example
//!
//! ```no_run
//! use std::fs::{self, File};
//! use ups::Patch;
//!
//! let rom = fs::read("samples/rom.bin")?;
//! let raw_patch = fs::read("samples/patch.ups")?;
//! let patch = Patch::parse(&raw_patch)?;
//! let output = patch.apply(&rom)?;
//! fs::write("patched.bin", &output)?;
//!
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
mod checksum;
mod patch;
mod util;
mod varint;

pub use checksum::Checksum;
pub use patch::{Block, Patch, PatchDirection, UpsParseError, UpsPatchError, UpsPatchErrors};
