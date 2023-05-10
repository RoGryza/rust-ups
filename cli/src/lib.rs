//! `upstool` cli library, this can be used to invoke any functionality from `upstool`
//! programmatically.
//!
//! ## Example
//!
//! ```no_run
//! use ups_cli::{PatchArgs, PatchDirection};
//!
//! let args = PatchArgs {
//!     patch: "some_patch.ups".into(),
//!     input: Some("some_rom.bin".into()),
//!     output: Some("patched_rom.bin".into()),
//!     direction: PatchDirection::Apply,
//! };
//! ups_cli::patch(&args).unwrap()
//! ```
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use structopt::StructOpt;

use ups::{Patch, UpsParseError, UpsPatchErrors};

pub use structopt;
pub use ups::{self, PatchDirection};

/// Command-line arguments for upstool.
#[derive(Debug, StructOpt)]
#[structopt(name = "upstool", about = "Simple UPS patcher")]
pub enum Args {
    /// Apply or revert UPS patches.
    Patch(PatchArgs),
    /// Generate UPS patch from input files.
    Generate(GenerateArgs),
}

/// Arguments for patch subcommand.
#[derive(Debug, StructOpt)]
pub struct PatchArgs {
    /// Path to UPS patch file.
    pub patch: PathBuf,
    /// Path to input file or - for stdin.
    pub input: Option<PathBuf>,
    /// Path to output file or - for stdout.
    pub output: Option<PathBuf>,
    /// Whether to patch a source file or get it back from the patched one.
    #[structopt(
        short, long,
        default_value = "apply",
        possible_values(&["apply", "revert"]),
        parse(try_from_str = parse_direction),
    )]
    pub direction: PatchDirection,
}

fn parse_direction(s: &str) -> Result<PatchDirection, String> {
    match s {
        "apply" => Ok(PatchDirection::Apply),
        "revert" => Ok(PatchDirection::Revert),
        _ => Err(format!("Invalid direction value \"{}\"", s)),
    }
}

/// Arguments for generate subcommand.
#[derive(Debug, StructOpt)]
pub struct GenerateArgs {
    /// Path to source file.
    pub source: PathBuf,
    /// Path to destination file.
    pub dest: PathBuf,
    /// Path to output patch file or - for stdout.
    pub patch: Option<PathBuf>,
}

/// Possible errors for any CLI command.
#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error("{}: {}", .0, .1)]
    Io(String, io::Error),
    #[error(transparent)]
    Parse(#[from] UpsParseError),
    #[error(transparent)]
    Patch(#[from] UpsPatchErrors),
}

impl Args {
    /// This is the same as [`StructOpt::from_args`], but you don't need the trait in scope.
    ///
    /// If you need access more methods from the trait, [`structopt`] is re-exported from this
    /// crate for convenience.
    pub fn from_args() -> Self {
        StructOpt::from_args()
    }

    /// Run the CLI application using these arguments.
    pub fn run(&self) -> Result<(), RunError> {
        match self {
            Args::Patch(args) => patch(args),
            Args::Generate(args) => generate(args),
        }
    }
}

/// Implementation for the patch subcommand.
pub fn patch(args: &PatchArgs) -> Result<(), RunError> {
    let raw_patch = fs::read(&args.patch).map_err(|e| {
        RunError::Io(
            format!("Failed to read patch file \"{}\"", args.patch.display()),
            e,
        )
    })?;
    let patch = Patch::parse(&raw_patch)?;

    let mut input_data = Vec::new();
    let (input_filename, input_stream_res) = match &args.input {
        Some(p) => (
            format!("\"{}\"", p.display()),
            File::open(p).and_then(|mut f| f.read_to_end(&mut input_data)),
        ),
        None => (
            "<stdin>".to_string(),
            io::stdin().read_to_end(&mut input_data),
        ),
    };
    input_stream_res
        .map_err(|e| RunError::Io(format!("Failed to read input file {}", input_filename), e))?;

    let output_data = patch.patch(args.direction, &input_data)?;
    write_output(&args.output, &output_data)
}

/// Implementation for the generate subcommand.
pub fn generate(args: &GenerateArgs) -> Result<(), RunError> {
    let src = fs::read(&args.source).map_err(|e| {
        RunError::Io(
            format!("Failed to read source file \"{}\"", args.source.display()),
            e,
        )
    })?;
    let dst = fs::read(&args.dest).map_err(|e| {
        RunError::Io(
            format!(
                "Failed to read destination file \"{}\"",
                args.dest.display()
            ),
            e,
        )
    })?;
    let patch = Patch::diff(&src, &dst);
    write_output(&args.patch, &patch.serialize())
}

fn write_output(path: &Option<PathBuf>, data: &[u8]) -> Result<(), RunError> {
    let (output_filename, output_stream_res) = match path {
        Some(p) => (format!("\"{}\"", p.display()), fs::write(p, data)),
        None => ("<stdout>".to_string(), io::stdout().write_all(data)),
    };
    output_stream_res.map_err(|e| {
        RunError::Io(
            format!("Failed to write to output file {}", output_filename),
            e,
        )
    })?;
    Ok(())
}
