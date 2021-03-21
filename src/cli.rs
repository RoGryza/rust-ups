use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use structopt::StructOpt;

use crate::{Patch, UpsApplyError, UpsParseError};

#[derive(Debug, StructOpt)]
#[structopt(name = "upstool", about = "Simple UPS patcher")]
pub struct Args {
    #[structopt(subcommand)]
    pub sub: Subcommands,
    pub patch: PathBuf,
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
}

#[derive(Debug, PartialEq, StructOpt)]
pub enum Subcommands {
    Apply,
    Revert,
}

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error("{}: {}", .0, .1)]
    Io(String, io::Error),
    #[error(transparent)]
    Parse(#[from] UpsParseError),
    #[error(transparent)]
    Apply(#[from] UpsApplyError),
}

impl Args {
    pub fn from_args() -> Self {
        StructOpt::from_args()
    }

    pub fn run(&self) -> Result<(), RunError> {
        let raw_patch = fs::read(&self.patch).map_err(|e| {
            RunError::Io(
                format!("Failed to read patch file \"{}\"", self.patch.display()),
                e,
            )
        })?;
        let patch = Patch::parse(&raw_patch)?;

        let mut input_data = Vec::new();
        let (input_filename, input_stream_res) = match &self.input {
            Some(p) => (
                format!("\"{}\"", p.display()),
                File::open(p).and_then(|mut f| f.read_to_end(&mut input_data)),
            ),
            None => (
                "<stdin>".to_string(),
                io::stdin().read_to_end(&mut input_data),
            ),
        };
        input_stream_res.map_err(|e| {
            RunError::Io(format!("Failed to read input file {}", input_filename), e)
        })?;

        let output_data = match self.sub {
            Subcommands::Apply => patch.apply(&input_data)?,
            Subcommands::Revert => patch.revert(&input_data)?,
        };

        let (output_filename, output_stream_res) = match &self.output {
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
}
