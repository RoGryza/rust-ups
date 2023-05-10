use std::convert::TryInto;
use std::fmt::{self, Debug, Display, Formatter};

use memchr::memchr;

use crate::checksum::Checksum;
use crate::util::SliceDiffs;
use crate::varint;

mod error;
#[cfg(test)]
mod test;

pub use error::*;

const MAGIC: &[u8] = b"UPS1";

/// UPS patch. Use [`parse`](Patch::parse) to read from a file and [`diff`](Patch::diff) to compute
/// a new patch from two files.
///
/// A patch encodes the difference between some `src` and some `dst` files. It contains metadata
/// from `src` and `dst` and a series of diff [`Block`]s.
///
/// You can [`apply`](Patch::apply) a patch to compute `dst` from `src` and
/// [`revert`](Patch::revert) it to compute `src` from `dst`.
///
/// # Reference
///
/// http://individual.utoronto.ca/dmeunier/ups-spec.pdf
#[derive(Clone, PartialEq, Eq)]
pub struct Patch {
    /// All blocks for the patch, in order.
    pub blocks: Vec<Block>,
    /// Source file size.
    pub src_size: usize,
    /// Source file checksum.
    pub src_checksum: Checksum,
    /// Destination file size.
    pub dst_size: usize,
    /// Destination file checksum.
    pub dst_checksum: Checksum,
}

/// Diff block in a [`Patch`].
#[derive(Clone, PartialEq, Eq)]
pub struct Block {
    /// Offset from the end of the previous diff block.
    offset: usize,
    /// Diff for this block, encoded as a zero-terminated XOR of `src` and `dst`.
    xor_data: Vec<u8>,
}

/// Patching direction, either from source to patched file or back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatchDirection {
    /// Apply the patch to the source file.
    Apply,
    /// Get source file from patched file.
    Revert,
}

// Struct to help implement apply/revert as a single function in Patch::patch.
// input is the input file, src for Apply and dst for Revert. output is the other way around, dst
// for Apply and src for Revert.
struct DirectionMetadata {
    input_size: usize,
    input_checksum: Checksum,
    output_size: usize,
    output_checksum: Checksum,
}

impl PatchDirection {
    fn metadata(&self, patch: &Patch) -> DirectionMetadata {
        match self {
            PatchDirection::Apply => DirectionMetadata {
                input_size: patch.src_size,
                input_checksum: patch.src_checksum,
                output_size: patch.dst_size,
                output_checksum: patch.dst_checksum,
            },
            PatchDirection::Revert => DirectionMetadata {
                input_size: patch.dst_size,
                input_checksum: patch.dst_checksum,
                output_size: patch.src_size,
                output_checksum: patch.src_checksum,
            },
        }
    }

    fn input_metadata_error(&self, mismatch: MetadataMismatch) -> UpsPatchError {
        match self {
            PatchDirection::Apply => UpsPatchError::SourceMetadataMismatch(mismatch),
            PatchDirection::Revert => UpsPatchError::DestMetadataMismatch(mismatch),
        }
    }

    fn output_metadata_error(&self, mismatch: MetadataMismatch) -> UpsPatchError {
        match self {
            PatchDirection::Apply => UpsPatchError::DestMetadataMismatch(mismatch),
            PatchDirection::Revert => UpsPatchError::SourceMetadataMismatch(mismatch),
        }
    }
}

impl Patch {
    /// Parses an UPS file.
    pub fn parse(mut input: &[u8]) -> UpsParseResult<Self> {
        if !input.starts_with(MAGIC) {
            return Err(UpsParseError::FormatMismatch(format!(
                "invalid preamble, expected \"{}\", found \"{}\"",
                EscapeNonAscii(MAGIC),
                EscapeNonAscii(&input[..std::cmp::min(4, input.len())]),
            )));
        }

        // Calculate patch checksum before doing any changes to input
        let actual_patch_checksum = Checksum::from_bytes(&input[..input.len() - 4]);
        input = &input[4..];

        let src_size = varint::read_bytes(&mut input).ok_or_else(|| {
            UpsParseError::FormatMismatch("error reading source file size".into())
        })?;
        let dst_size = varint::read_bytes(&mut input)
            .ok_or_else(|| UpsParseError::FormatMismatch("error reading dest file size".into()))?;

        if input.len() < 12 {
            return Err(UpsParseError::FormatMismatch(
                "failed to read checksums".into(),
            ));
        }
        let (mut body, mut checksums) = input.split_at(input.len() - 12);

        let mut blocks = Vec::new();
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
            blocks.push(Block {
                offset,
                xor_data: xor_data.to_vec(),
            });
        }

        let src_checksum = read_checksum(&mut checksums)?;
        let dst_checksum = read_checksum(&mut checksums)?;
        let patch_checksum = read_checksum(&mut checksums)?;

        let parsed_patch = Patch {
            blocks,
            src_size,
            src_checksum,
            dst_size,
            dst_checksum,
        };

        if actual_patch_checksum != patch_checksum {
            Err(UpsParseError::PatchChecksumMismatch {
                parsed_patch,
                expected: patch_checksum,
                actual: actual_patch_checksum,
            })
        } else {
            Ok(parsed_patch)
        }
    }

    /// Calculate a patch by comparing the source and destination files.
    pub fn diff(src: &[u8], dst: &[u8]) -> Self {
        let mut blocks = Vec::new();
        // Index into the end of the previous block's data.
        let mut prev_end = 0;
        for diff_range in SliceDiffs::new(src, dst) {
            let offset = diff_range.start - prev_end;
            let mut xor_data: Vec<_> = src[diff_range.clone()]
                .iter()
                .zip(&dst[diff_range.clone()])
                .map(|(a, b)| a ^ b)
                .collect();
            // We know that `xor_data` doesn't contain zeroes, because that would imply we got a
            // SliceDiff with some equal bytes.
            assert!(memchr::memchr(0, &xor_data).is_none());
            xor_data.push(0);
            blocks.push(Block { offset, xor_data });
            // prev_end needs to account for the appended 0.
            prev_end = diff_range.end + 1;
        }

        let (min_len, max_slice) = if src.len() < dst.len() {
            (src.len(), dst)
        } else {
            (dst.len(), src)
        };

        let mut pending_data = &max_slice[min_len..];
        let split_pos = memchr::memchr(0, pending_data).unwrap_or(pending_data.len());
        let (last_block_data, next_pending) = pending_data.split_at(split_pos);
        // Account for 0 byte
        pending_data = next_pending.split_first().map_or(&[], |s| s.1);
        // The last block may have more data after the end of the source file.
        if prev_end == min_len + 1 {
            if let Some(block) = blocks.last_mut() {
                // Remove the last 0 byte so we can append to xor_data.
                block.xor_data.pop();
                block.xor_data.extend_from_slice(last_block_data);
                block.xor_data.push(0);
            }
        } else if !last_block_data.is_empty() {
            let mut xor_data = last_block_data.to_vec();
            xor_data.push(0);
            blocks.push(Block {
                offset: min_len - prev_end,
                xor_data,
            });
        }

        // Emit leftover blocks if either file has pending data.
        while !pending_data.is_empty() {
            let offset = match pending_data.iter().position(|x| *x != 0) {
                Some(p) => p,
                // All remaining bytes are 0.
                None => break,
            };
            pending_data = &pending_data[offset..];
            let split_pos = memchr::memchr(0, pending_data).map_or(pending_data.len(), |x| x + 1);
            let (xor_data, next_pending) = pending_data.split_at(split_pos);
            pending_data = next_pending;
            blocks.push(Block {
                offset,
                xor_data: xor_data.to_vec(),
            });
        }
        // Last block may be missing a trailing 0.
        if let Some(block) = blocks.last_mut() {
            if block.xor_data.last() != Some(&0) {
                block.xor_data.push(0);
            }
        }

        Patch {
            blocks,
            src_size: src.len(),
            src_checksum: Checksum::from_bytes(src),
            dst_size: dst.len(),
            dst_checksum: Checksum::from_bytes(dst),
        }
    }

    /// Serialize this patch as an UPS file.
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = b"UPS1".to_vec();
        varint::write_bytes(&mut bytes, self.src_size);
        varint::write_bytes(&mut bytes, self.dst_size);
        for block in &self.blocks {
            varint::write_bytes(&mut bytes, block.offset);
            bytes.extend(&block.xor_data);
        }

        bytes.extend_from_slice(&self.src_checksum.0.to_le_bytes());
        bytes.extend_from_slice(&self.dst_checksum.0.to_le_bytes());
        let patch_checksum = Checksum::from_bytes(&bytes);
        bytes.extend_from_slice(&patch_checksum.0.to_le_bytes());
        bytes
    }

    /// Applies or reverts a patch on the given buffer and return the raw output bytes.
    pub fn patch(&self, direction: PatchDirection, input: &[u8]) -> UpsPatchResult<Vec<u8>> {
        let metadata = direction.metadata(self);
        let mut errors = Vec::new();

        if let Some(err) = MetadataMismatch::size(metadata.input_size, input.len()) {
            errors.push(direction.input_metadata_error(err));
        }
        let input_checksum = Checksum::from_bytes(input);
        if let Some(err) = MetadataMismatch::checksum(metadata.input_checksum, input_checksum) {
            errors.push(direction.input_metadata_error(err));
        }

        let mut output = vec![0; metadata.output_size];
        let input_copy_len = std::cmp::min(metadata.output_size, metadata.input_size);
        output[..input_copy_len].copy_from_slice(&input[..input_copy_len]);

        let mut output_ptr: &mut [u8] = &mut output;
        for block in &self.blocks {
            if block.offset >= output_ptr.len() {
                break;
            }
            output_ptr = &mut output_ptr[block.offset..];
            for (out_byte, patch_byte) in output_ptr.iter_mut().zip(&block.xor_data) {
                *out_byte ^= patch_byte;
            }
            if block.xor_data.len() >= output_ptr.len() {
                break;
            }
            output_ptr = &mut output_ptr[block.xor_data.len()..];
        }

        let output_checksum = Checksum::from_bytes(&output);
        if let Some(err) = MetadataMismatch::checksum(metadata.output_checksum, output_checksum) {
            errors.push(direction.output_metadata_error(err));
        }

        UpsPatchErrors::check_errors(output, errors)
    }

    /// Apply patch to source data. Returns the contents of the patched file.
    pub fn apply(&self, src: &[u8]) -> UpsPatchResult<Vec<u8>> {
        self.patch(PatchDirection::Apply, src)
    }

    /// Revert patch applied to the given buffer. Returns the contents of the reverted file.
    pub fn revert(&self, dst: &[u8]) -> UpsPatchResult<Vec<u8>> {
        self.patch(PatchDirection::Revert, dst)
    }
}

/// Helper to display a byte string as ASCII, hex encoding non-ASCII chars.
struct EscapeNonAscii<'a>(&'a [u8]);

impl<'a> Display for EscapeNonAscii<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut chars = self.0.iter().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii() {
                write!(f, "{}", *c as char)?;
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

impl Debug for Patch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Patch")
            .field("src_size", &self.src_size)
            .field("src_checksum", &self.src_checksum)
            .field("dst_size", &self.dst_size)
            .field("dst_checksum", &self.dst_checksum)
            .field(
                "blocks",
                &MaybeTruncate {
                    max_elements: 16,
                    slice: &self.blocks,
                },
            )
            .finish()
    }
}

impl Debug for Block {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Block")
            .field("offset", &self.offset)
            .field(
                "xor_data",
                &MaybeTruncate {
                    max_elements: 16,
                    slice: &self.xor_data,
                },
            )
            .finish()
    }
}

// Debug impl for slices which switches to "<size: {size}>" if `slice` has over `max_elements`.
struct MaybeTruncate<'a, T> {
    max_elements: usize,
    slice: &'a [T],
}

impl<'a, T: Debug> Debug for MaybeTruncate<'a, T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.slice.len() <= self.max_elements {
            Debug::fmt(self.slice, f)
        } else {
            write!(f, "<size: {}>", self.slice.len())
        }
    }
}
