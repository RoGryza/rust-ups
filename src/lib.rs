mod checksum;
pub mod parser;
mod varint;

pub use checksum::{Checksum, ChecksumStream};

use std::fmt::{self, Display, Formatter};
use std::io::{self, BufRead, ErrorKind, Read, Seek, SeekFrom, Write};

use parser::{Parser, UpsParseError};

#[derive(thiserror::Error, Debug)]
pub enum UpsApplyError {
    #[error("I/O error reading source file: {}", .0)]
    SourceRead(#[source] io::Error),
    #[error("I/O error writing to destination file: {}", .0)]
    DestWrite(#[source] io::Error),
    #[error(transparent)]
    PatchRead(#[from] UpsParseError),
    #[error("Metadata mismatch for source file: {}", .0)]
    SourceMetadataMismatch(FileMetadataMismatch),
    #[error("Metadata mismatch for dest file: {}", .0)]
    DestMetadataMismatch(FileMetadataMismatch),
}

#[derive(Debug)]
pub enum FileMetadataMismatch {
    Checksum {
        expected: Checksum,
        actual: Checksum,
    },
    Size {
        expected: usize,
        actual: usize,
    },
}

impl FileMetadataMismatch {
    fn source(self) -> UpsApplyError {
        UpsApplyError::SourceMetadataMismatch(self)
    }

    fn dest(self) -> UpsApplyError {
        UpsApplyError::DestMetadataMismatch(self)
    }
}

impl Display for FileMetadataMismatch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            FileMetadataMismatch::Checksum { expected, actual } => {
                write!(f, "expected checksum {}, got {}", expected, actual)
            }
            FileMetadataMismatch::Size { expected, actual } => {
                write!(f, "expected size {}, got {}", expected, actual)
            }
        }
    }
}

pub type UpsApplyResult<T> = Result<T, UpsApplyError>;

pub fn apply_patch<R, S, W>(patch: Parser<R>, mut src: S, dst: W) -> UpsApplyResult<()>
where
    R: BufRead,
    S: Read + Seek,
    W: Write,
{
    let src_size = src
        .seek(SeekFrom::End(0))
        .map_err(UpsApplyError::SourceRead)? as usize;
    if src_size != patch.src_size {
        return Err(FileMetadataMismatch::Size {
            expected: patch.src_size,
            actual: src_size,
        }
        .source());
    }
    src.seek(SeekFrom::Start(0))
        .map_err(UpsApplyError::SourceRead)?;

    let mut src_reader = ChecksumStream::new(src).chain(io::repeat(0));
    let mut dst_writer = ChecksumStream::new(dst);
    let mut dst_size = 0;
    let mut buf = Vec::new();

    for hunk_res in patch.hunks {
        let mut hunk = hunk_res?;
        if hunk.offset > 0 {
            iocopy(
                &mut src_reader.by_ref().take(hunk.offset as u64),
                &mut dst_writer,
            )?;
        }

        dst_size += hunk.offset + hunk.patch.len();
        if dst_size > patch.dst_size {
            let delta = dst_size - patch.dst_size;
            hunk.patch.truncate(hunk.patch.len() - delta);
            dst_size -= delta;
        }
        buf.resize(hunk.patch.len(), 0);
        src_reader
            .read_exact(&mut buf)
            .map_err(UpsApplyError::SourceRead)?;
        for (src_byte, patch_byte) in buf.iter_mut().zip(&hunk.patch) {
            *src_byte ^= patch_byte;
        }
        dst_writer
            .write_all(&buf)
            .map_err(UpsApplyError::DestWrite)?;
    }

    if dst_size != patch.dst_size {
        return Err(FileMetadataMismatch::Size {
            expected: patch.dst_size,
            actual: dst_size,
        }
        .dest());
    }
    let (_, src_checksum) = src_reader.into_inner().0.finalize();
    if src_checksum != patch.checksums.src {
        return Err(FileMetadataMismatch::Checksum {
            expected: patch.checksums.src,
            actual: src_checksum,
        }
        .source());
    }
    let (_, dst_checksum) = dst_writer.finalize();
    if dst_checksum != patch.checksums.dst {
        return Err(FileMetadataMismatch::Checksum {
            expected: patch.checksums.dst,
            actual: dst_checksum,
        }
        .dest());
    }

    Ok(())
}

// Like io::copy but maps errors to UpsApplyError
fn iocopy<R, W>(reader: &mut R, writer: &mut W) -> UpsApplyResult<()>
where
    R: Read,
    W: Write,
{
    let mut buf = [0u8; 4096];
    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(UpsApplyError::SourceRead(e)),
        };
        writer
            .write_all(&buf[..len])
            .map_err(UpsApplyError::DestWrite)?;
    }
}
