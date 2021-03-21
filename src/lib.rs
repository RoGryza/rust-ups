//! Parse, apply and revert UPS patches.
//!
//! ## Note
//! This crate was not designed to handle large files, it reads files to completion into memory.
//!
//! ## Example
//!
//! ```no_run
//! use std::fs::{self, File};
//! use ups::Patch;
//!
//! let rom = fs::read("samples/rom.bin")?;
//! let raw_patch = fs::read("samples/rr-2-2b.ups")?;
//! let patch = Patch::parse(&raw_patch)?;
//! let output = patch.apply(&rom)?;
//! fs::write("rr-2-2b.bin", &output)?;
//!
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
#![deny(warnings)]
mod checksum;
#[cfg(feature = "cli")]
pub mod cli;
mod patch;
mod varint;

pub use checksum::Checksum;
pub use patch::{Patch, UpsApplyError, UpsParseError};
