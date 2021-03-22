use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use structopt::StructOpt;

use ups::{Patch, PatchDirection, UpsParseError, UpsPatchError};

#[derive(Debug, StructOpt)]
#[structopt(name = "upstool", about = "Simple UPS patcher")]
pub enum Args {
    /// Apply or revert UPS patches.
    Patch(PatchArgs),
}

#[derive(Debug, StructOpt)]
pub struct PatchArgs {
    /// Path to UPS patch file.
    patch: PathBuf,
    /// Path to input file or - for stdin.
    input: Option<PathBuf>,
    /// Path to output file or - for stdout.
    output: Option<PathBuf>,
    /// Whether to patch a source file to get it back from the patched one.
    #[structopt(
        short, long,
        default_value = "apply",
        possible_values(&["apply", "revert"]),
        parse(try_from_str = parse_direction),
    )]
    direction: PatchDirection,
}

fn parse_direction(s: &str) -> Result<PatchDirection, String> {
    match s {
        "apply" => Ok(PatchDirection::Apply),
        "revert" => Ok(PatchDirection::Revert),
        _ => Err(format!("Invalid direction value \"{}\"", s)),
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error("{}: {}", .0, .1)]
    Io(String, io::Error),
    #[error(transparent)]
    Parse(#[from] UpsParseError),
    #[error(transparent)]
    Patch(#[from] UpsPatchError),
}

impl Args {
    pub fn from_args() -> Self {
        StructOpt::from_args()
    }

    pub fn run(&self) -> Result<(), RunError> {
        match self {
            Args::Patch(args) => patch(args),
        }
    }
}

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

    let (output_filename, output_stream_res) = match &args.output {
        Some(p) => (format!("\"{}\"", p.display()), fs::write(p, &output_data)),
        None => ("<stdout>".to_string(), io::stdout().write_all(&output_data)),
    };
    output_stream_res.map_err(|e| {
        RunError::Io(
            format!("Failed to write to output file {}", output_filename,),
            e,
        )
    })?;

    Ok(())
}
