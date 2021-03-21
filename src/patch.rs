use std::convert::TryInto;
use std::fmt::{self, Display, Formatter};

use memchr::memchr;

use crate::checksum::Checksum;
use crate::varint;

/// Possible errors when parsing an UPS patch file.
#[derive(thiserror::Error, Debug)]
pub enum UpsParseError {
    #[error("The file doesn't look like it's in UPS format: {}", .0)]
    FormatMismatch(String),
    /// Calculated patch checksum doesn't match the one from the patch metadata. You can access the
    /// patch in `parsed_patch` in case you want to ignore checksum errors.
    #[error(
        "Checksum mismatch for patch file: expected {}, got {}",
        .parsed_patch.patch_checksum,
        .actual,
    )]
    PatchChecksumMismatch {
        parsed_patch: Patch,
        actual: Checksum,
    },
}

pub type UpsParseResult<T> = Result<T, UpsParseError>;

/// Possible errors when applying or revering an UPS patch.
#[derive(thiserror::Error, Debug)]
pub enum UpsApplyError {
    #[error("Source file size mismatch: expected {}, got {}", .expected, .actual)]
    SourceSizeMismatch { expected: usize, actual: usize },
    #[error("Source file checksum mismatch: expected {}, got {}", .expected, .actual)]
    SourceChecksumMismatch {
        expected: Checksum,
        actual: Checksum,
    },
    #[error("Destination file size mismatch: expected {}, got {}", .expected, .actual)]
    DestSizeMismatch { expected: usize, actual: usize },
    #[error("Destination file checksum mismatch: expected {}, got {}", .expected, .actual)]
    DestChecksumMismatch {
        expected: Checksum,
        actual: Checksum,
    },
}

pub type UpsApplyResult<T> = Result<T, UpsApplyError>;

/// Parsed UPS patch file. Use [`parse`] to instantiate and [`apply`] and [`revert`] to use the
/// patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub hunks: Vec<Hunk>,
    pub src_size: usize,
    pub src_checksum: Checksum,
    pub dst_size: usize,
    pub dst_checksum: Checksum,
    pub patch_checksum: Checksum,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub offset: usize,
    pub xor_data: Vec<u8>,
}

impl Patch {
    pub fn parse(mut input: &[u8]) -> UpsParseResult<Self> {
        const MAGIC: &[u8] = b"UPS1";
        if !input.starts_with(MAGIC) {
            return Err(UpsParseError::FormatMismatch(format!(
                "invalid preamble, expected \"{}\", found \"{}\"",
                EscapeNonAscii(MAGIC),
                EscapeNonAscii(&input[..std::cmp::min(4, input.len())]),
            )));
        }

        // Calculate patch checksum before doing any changes to input
        let mut patch_hasher = crc32fast::Hasher::new();
        patch_hasher.update(&input[..input.len() - 4]);
        let actual_patch_checksum = Checksum(patch_hasher.finalize());

        input = &input[4..];

        let src_size = varint::read_bytes(&mut input).ok_or_else(|| {
            UpsParseError::FormatMismatch("Error reading source file size".into())
        })?;
        let dst_size = varint::read_bytes(&mut input)
            .ok_or_else(|| UpsParseError::FormatMismatch("Error reading dest file size".into()))?;

        if input.len() < 12 {
            return Err(UpsParseError::FormatMismatch(
                "Failed to read checksums".into(),
            ));
        }
        let (mut body, mut checksums) = input.split_at(input.len() - 12);

        let mut hunks = Vec::new();
        while !body.is_empty() {
            let offset = match varint::read_bytes(&mut body) {
                Some(o) => o,
                None => break,
            };
            let (xor_data, next_body) = match memchr(0, &body) {
                Some(i) => body.split_at(i + 1),
                None => (body, [].as_ref()),
            };
            body = next_body;
            hunks.push(Hunk {
                offset,
                xor_data: xor_data.to_vec(),
            });
        }

        let src_checksum = read_checksum(&mut checksums)?;
        let dst_checksum = read_checksum(&mut checksums)?;
        let patch_checksum = read_checksum(&mut checksums)?;

        let parsed_patch = Patch {
            hunks,
            src_size,
            src_checksum,
            dst_size,
            dst_checksum,
            patch_checksum,
        };

        if actual_patch_checksum != patch_checksum {
            Err(UpsParseError::PatchChecksumMismatch {
                parsed_patch,
                actual: actual_patch_checksum,
            })
        } else {
            Ok(parsed_patch)
        }
    }

    /// Apply patch to source data. Returns the contents of the patched file.
    pub fn apply(&self, src: &[u8]) -> UpsApplyResult<Vec<u8>> {
        if src.len() != self.src_size {
            return Err(UpsApplyError::SourceSizeMismatch {
                expected: self.src_size,
                actual: src.len(),
            });
        }

        let src_checksum = Checksum::from_bytes(&src);
        if src_checksum != self.src_checksum {
            return Err(UpsApplyError::SourceChecksumMismatch {
                expected: self.src_checksum,
                actual: src_checksum,
            });
        }

        let mut output = src.to_vec();
        output.resize(self.dst_size, 0);

        let mut output_ptr: &mut [u8] = &mut output;
        for hunk in &self.hunks {
            output_ptr = &mut output_ptr[hunk.offset..];
            for (out_byte, patch_byte) in output_ptr.iter_mut().zip(&hunk.xor_data) {
                *out_byte ^= patch_byte;
            }
            if hunk.xor_data.len() >= output_ptr.len() {
                break;
            }
            output_ptr = &mut output_ptr[hunk.xor_data.len()..];
        }

        let dst_checksum = Checksum::from_bytes(&output);
        if dst_checksum != self.dst_checksum {
            return Err(UpsApplyError::DestChecksumMismatch {
                expected: self.dst_checksum,
                actual: dst_checksum,
            });
        }

        Ok(output)
    }

    /// Revert patch applied to the given buffer. Returns the contents of the reverted file.
    pub fn revert(&self, dst: &[u8]) -> UpsApplyResult<Vec<u8>> {
        if dst.len() != self.dst_size {
            return Err(UpsApplyError::DestSizeMismatch {
                expected: self.dst_size,
                actual: dst.len(),
            });
        }

        let dst_checksum = Checksum::from_bytes(&dst);
        if dst_checksum != self.dst_checksum {
            return Err(UpsApplyError::DestChecksumMismatch {
                expected: self.dst_checksum,
                actual: dst_checksum,
            });
        }

        let mut output = dst[..self.src_size].to_vec();
        let mut output_ptr: &mut [u8] = &mut output;
        for hunk in &self.hunks {
            if hunk.offset >= output_ptr.len() {
                break;
            }
            output_ptr = &mut output_ptr[hunk.offset..];
            for (out_byte, patch_byte) in output_ptr.iter_mut().zip(&hunk.xor_data) {
                *out_byte ^= patch_byte;
            }
            if hunk.xor_data.len() >= output_ptr.len() {
                break;
            }
            output_ptr = &mut output_ptr[hunk.xor_data.len()..];
        }

        let src_checksum = Checksum::from_bytes(&output);
        if src_checksum != self.src_checksum {
            return Err(UpsApplyError::SourceChecksumMismatch {
                expected: self.src_checksum,
                actual: src_checksum,
            });
        }

        Ok(output)
    }
}

struct EscapeNonAscii<'a>(&'a [u8]);

impl<'a> Display for EscapeNonAscii<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut chars = self.0.iter().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii() {
                (*c as char).fmt(f)?;
            } else {
                write!(f, "{:02X}", c)?;
            }
            if chars.peek().is_some() {
                write!(f, " ")?;
            }
        }
        Ok(())
    }
}

fn read_checksum(buf: &mut &[u8]) -> UpsParseResult<Checksum> {
    if buf.len() < 4 {
        Err(UpsParseError::FormatMismatch(
            "Unexpected EOF while reading file".into(),
        ))
    } else {
        let (checksum_bytes, rest) = buf.split_at(4);
        *buf = rest;
        Ok(Checksum(u32::from_le_bytes(
            (&*checksum_bytes).try_into().unwrap(),
        )))
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
            Patch::parse(&raw).unwrap_err();
        }

        #[test]
        fn test_patch_metadata_ok(data in patches()) {
            let parsed = Patch::parse(&data.serialized).unwrap();
            prop_assert_eq!(data.patch.src_size, parsed.src_size);
            prop_assert_eq!(data.patch.src_checksum, parsed.src_checksum);
            prop_assert_eq!(data.patch.dst_size, parsed.dst_size);
            prop_assert_eq!(data.patch.dst_checksum, parsed.dst_checksum);
        }

        #[test]
        fn test_patch_magic_err(magic in invalid_magic(), mut data in patches()) {
            data.serialized[..4].copy_from_slice(&magic);
            let err = Patch::parse(&data.serialized).unwrap_err();
            prop_assert!(matches!(err, UpsParseError::FormatMismatch(_)));
        }

        #[test]
        fn test_patch_checksum_err(mut data in patches()) {
            // Overwrite checksum
            let checksum_offset = data.serialized.len() - 4;
            data.serialized[checksum_offset..].copy_from_slice(&[0, 0, 0, 0]);
            data.patch.patch_checksum = Checksum(0);
            let err = Patch::parse(&data.serialized).unwrap_err();
            match err {
                UpsParseError::PatchChecksumMismatch { parsed_patch, actual } => {
                    prop_assert_ne!(actual, Checksum(0));
                    prop_assert_eq!(parsed_patch, data.patch);
                }
                _ => prop_assert!(false, "Expected PatchChecksumMismatch, got {}", err),
            }
        }

        #[test]
        fn test_patch_hunks_ok(data in patches()) {
            let patch = Patch::parse(&data.serialized).unwrap();
            assert_eq!(data.patch.hunks, patch.hunks);
        }
    }

    fn invalid_magic() -> impl Strategy<Value = [u8; 4]> {
        array::uniform4(any::<u8>()).prop_filter("Valid magic", |v| v != b"UPS1")
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct PatchData {
        patch: Patch,
        serialized: Vec<u8>,
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
            bytes.extend_from_slice(&varint::to_vec(src_size));
            bytes.extend_from_slice(&varint::to_vec(dst_size));
            for hunk in &hunks {
                bytes.extend_from_slice(&varint::to_vec(hunk.offset));
                bytes.extend(&hunk.xor_data);
            }

            bytes.extend_from_slice(&src_checksum.0.to_le_bytes());
            bytes.extend_from_slice(&dst_checksum.0.to_le_bytes());
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(&bytes);
            let patch_checksum = Checksum(hasher.finalize());
            bytes.extend_from_slice(&patch_checksum.0.to_le_bytes());

            PatchData {
                patch: Patch {
                    hunks,
                    src_size,
                    src_checksum,
                    dst_size,
                    dst_checksum,
                    patch_checksum,
                },
                serialized: bytes,
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
            (offset in any::<usize>(), xor_data in xor_data())
                -> Hunk
                {
                    Hunk {
                        offset,
                        xor_data,
                    }
                }
    }

    fn xor_data() -> impl Strategy<Value = Vec<u8>> {
        vec(1..=255u8, 1..64).prop_map(|mut v| {
            v.push(0);
            v
        })
    }
}
