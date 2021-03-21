use std::io::{self, BufRead, Read, Seek, SeekFrom, Take};

use crate::checksum::ChecksumStream;
use crate::{varint, Checksum};

#[derive(thiserror::Error, Debug)]
pub enum UpsParseError {
    #[error("The file doesn't look like it's in UPS format: {}", .0)]
    FormatMismatch(String),
    #[error("Checksum mismatch for patch file: expected {}, got {}", .expected, .actual)]
    PatchChecksumMismatch {
        expected: Checksum,
        actual: Checksum,
    },
    #[error("I/O error reading UPS file: {}", .0)]
    Io(
        #[source]
        #[from]
        io::Error,
    ),
}

pub type UpsParseResult<T> = Result<T, UpsParseError>;

#[derive(Debug)]
pub struct Parser<R> {
    pub hunks: Hunks<R>,
    pub src_size: usize,
    pub dst_size: usize,
    pub checksums: Checksums,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct Checksums {
    pub src: Checksum,
    pub dst: Checksum,
}

#[derive(Debug)]
pub struct Hunks<R> {
    reader: Take<R>,
    remaining: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub offset: usize,
    pub patch: Vec<u8>,
}

impl<R: BufRead + Seek> Parser<R> {
    pub fn init(reader: R) -> UpsParseResult<Self> {
        Self::init_inner(reader, false)
    }

    pub fn init_skip_checksum(reader: R) -> UpsParseResult<Self> {
        Self::init_inner(reader, true)
    }

    fn init_inner(mut reader: R, skip_checksum: bool) -> UpsParseResult<Self> {
        let size = reader.seek(SeekFrom::End(0))?;
        Parser::validate_header(&mut reader)?;
        let (checksums, patch_checksum) = Parser::read_checksums(&mut reader)?;
        if !skip_checksum {
            Parser::validate_patch(&mut reader, size, patch_checksum)?;
        }

        reader.seek(SeekFrom::Start(4))?;
        let (src_size, src_bytes_read) = varint::read(&mut reader)?;
        let (dst_size, dst_bytes_read) = varint::read(&mut reader)?;
        // Total file size - magic - file sizes - checksums
        let body_size = size as usize - 4 - src_bytes_read - dst_bytes_read - 12;

        Ok(Parser {
            hunks: Hunks::new(reader, body_size),
            src_size,
            dst_size,
            checksums,
        })
    }

    fn validate_header(reader: &mut R) -> UpsParseResult<()> {
        reader.seek(SeekFrom::Start(0))?;
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != b"UPS1" {
            Err(UpsParseError::FormatMismatch(format!(
                "invalid file magic number {:?}, expected 'UPS1' ({:?})",
                magic, b"UPS1"
            )))
        } else {
            Ok(())
        }
    }

    fn read_checksums(reader: &mut R) -> UpsParseResult<(Checksums, Checksum)> {
        reader.seek(SeekFrom::End(-12))?;
        let mut raw_checksum = [0u8; 4];
        reader.read_exact(&mut raw_checksum)?;
        let src = Checksum(u32::from_le_bytes(raw_checksum));
        reader.read_exact(&mut raw_checksum)?;
        let dst = Checksum(u32::from_le_bytes(raw_checksum));
        reader.read_exact(&mut raw_checksum)?;
        let patch = Checksum(u32::from_le_bytes(raw_checksum));
        Ok((Checksums { src, dst }, patch))
    }

    fn validate_patch(reader: &mut R, size: u64, checksum: Checksum) -> UpsParseResult<()> {
        reader.seek(SeekFrom::Start(0))?;
        let actual_checksum = ChecksumStream::new(reader.take(size - 4)).calculate_checksum()?;
        if checksum == actual_checksum {
            Ok(())
        } else {
            Err(UpsParseError::PatchChecksumMismatch {
                expected: checksum,
                actual: actual_checksum,
            })
        }
    }
}

impl<R: BufRead> Hunks<R> {
    fn new(reader: R, remaining: usize) -> Self {
        Hunks {
            reader: reader.take(remaining as u64),
            remaining,
        }
    }

    // Transposed version of Iterator::next so that we can use `?`
    fn next_result(&mut self) -> UpsParseResult<Option<Hunk>> {
        if self.remaining == 0 {
            return Ok(None);
        }

        let (offset, offset_n_bytes) = varint::read(&mut self.reader)?;
        if offset_n_bytes == 0 {
            self.remaining = 0;
            return Ok(None);
        }
        let mut patch = Vec::new();
        self.reader.read_until(0, &mut patch)?;

        if patch.len() == 0 {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Unexpected EOF while reading hunks",
            )
            .into())
        } else {
            self.remaining -= offset_n_bytes + patch.len();
            Ok(Some(Hunk { offset, patch }))
        }
    }
}

impl<R: BufRead> Iterator for Hunks<R> {
    type Item = UpsParseResult<Hunk>;

    fn next(&mut self) -> Option<UpsParseResult<Hunk>> {
        self.next_result().transpose()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;
    use std::matches;

    use proptest::array;
    use proptest::collection::vec;
    use proptest::prelude::*;

    proptest! {
        // TODO assert parser fails when there isn't enough input

        // TODO generate problematic data for testing, this is just a placeholder dumb "fuzzer"
        #[test]
        fn test_garbage(mut raw in vec(any::<u8>(), 0..4096)) {
            if raw.len() >= 4 {
                // Set magic preamble to test other failure cases
                raw[..4].copy_from_slice(b"UPS1");
            }
            Parser::init(Cursor::new(raw)).unwrap_err();
        }

        #[test]
        fn test_patch_metadata_ok(patch in patches()) {
            let parser = Parser::init(Cursor::new(patch.data)).unwrap();
            prop_assert_eq!(patch.src_size, parser.src_size);
            prop_assert_eq!(patch.src_checksum, parser.checksums.src);
            prop_assert_eq!(patch.dst_size, parser.dst_size);
            prop_assert_eq!(patch.dst_checksum, parser.checksums.dst);
        }

        #[test]
        fn test_patch_magic_err(magic in invalid_magic(), mut patch in patches()) {
            patch.set_magic(magic);
            let err = Parser::init(Cursor::new(patch.data)).unwrap_err();
            prop_assert!(matches!(err, UpsParseError::FormatMismatch(_)));
        }

        #[test]
        fn test_patch_checksum_err(mut patch in patches()) {
            patch.set_patch_checksum(0);
            let err = Parser::init(Cursor::new(patch.data)).unwrap_err();
            // prop_assert doesn't accept curly brackets in the pattern
            assert!(matches!(err, UpsParseError::PatchChecksumMismatch { .. }));
        }

        #[test]
        fn test_patch_hunks_ok(patch in patches()) {
            let parser = Parser::init(Cursor::new(patch.data)).unwrap();
            let parsed_hunks: Vec<_> = parser.hunks.collect::<UpsParseResult<_>>().unwrap();
            assert_eq!(patch.hunks, parsed_hunks);
        }
    }

    fn invalid_magic() -> impl Strategy<Value = [u8; 4]> {
        array::uniform4(any::<u8>()).prop_filter("Valid magic", |v| v != b"UPS1")
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct PatchData {
        data: Vec<u8>,
        hunks: Vec<Hunk>,
        src_size: usize,
        src_checksum: Checksum,
        dst_size: usize,
        dst_checksum: Checksum,
        patch_checksum: Checksum,
    }

    impl PatchData {
        fn set_magic(&mut self, magic: [u8; 4]) {
            self.data[..4].copy_from_slice(&magic);
        }

        fn set_patch_checksum(&mut self, checksum: u32) {
            let idx = self.data.len() - 4;
            self.data[idx..].copy_from_slice(&checksum.to_le_bytes());
        }
    }

    prop_compose! {
        fn patches()
            (hunks in vec(patch_hunks(), 1..64),
             src_size in file_sizes(),
             src_checksum in file_checksums(),
             dst_size in file_sizes(),
             dst_checksum in file_checksums())
            -> PatchData
        {
            let mut bytes = b"UPS1".to_vec();
            varint::write(&mut bytes, src_size).unwrap();
            varint::write(&mut bytes, dst_size).unwrap();
            for hunk in &hunks {
                varint::write(&mut bytes, hunk.offset).unwrap();
                bytes.extend(&hunk.patch);
            }

            bytes.extend_from_slice(&src_checksum.0.to_le_bytes());
            bytes.extend_from_slice(&dst_checksum.0.to_le_bytes());
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(&bytes);
            let patch_checksum = Checksum(hasher.finalize());
            bytes.extend_from_slice(&patch_checksum.0.to_le_bytes());

            PatchData {
                data: bytes,
                hunks,
                src_size,
                src_checksum,
                dst_size,
                dst_checksum,
                patch_checksum,
            }
        }
    }

    fn file_sizes() -> impl Strategy<Value = usize> {
        1..32usize
    }

    fn file_checksums() -> impl Strategy<Value = Checksum> {
        (0..32u32).prop_map(Checksum)
    }

    prop_compose! {
        fn patch_hunks()
            (offset in any::<usize>(), patch in patch_byte_vec())
            -> Hunk
        {
            Hunk {
                offset,
                patch,
            }
        }
    }

    fn patch_byte_vec() -> impl Strategy<Value = Vec<u8>> {
        vec(1..=255u8, 1..64).prop_map(|mut v| {
            v.push(0);
            v
        })
    }
}
