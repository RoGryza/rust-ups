use std::convert::TryInto;
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};

use memchr::memchr;

use crate::checksum::Checksum;
use crate::varint;

const MAGIC: &[u8] = b"UPS1";

/// Possible errors when parsing an UPS patch file.
#[derive(thiserror::Error, Debug)]
pub enum UpsParseError {
    #[error("The file doesn't look like it's in UPS format: {}", .0)]
    FormatMismatch(String),
    /// Calculated patch checksum doesn't match the one from the patch metadata. You can access the
    /// patch in `parsed_patch` in case you want to ignore checksum errors.
    #[error(
        "Checksum mismatch for patch file: expected {}, got {}",
        .expected, .actual,
    )]
    PatchChecksumMismatch {
        parsed_patch: Patch,
        expected: Checksum,
        actual: Checksum,
    },
}

pub type UpsParseResult<T> = Result<T, UpsParseError>;

/// Collection of errors returned from patching. You can access the patched file in `output` in
/// case you want to ignore the errors. Use [`errors`] and [`into_errors`] to inspect errors.
pub struct UpsPatchErrors {
    /// Possibly invalid output from the patch operation.
    pub output: Vec<u8>,
    // Standalone error so we can't represent invalid states (empty error list).
    fst_error: UpsPatchError,
    errors: Vec<UpsPatchError>,
}

impl UpsPatchErrors {
    /// Smart constructor, returns `Err` if there are any errors in `errors`, else returns
    /// `Ok(output)`.
    pub fn check_errors(output: Vec<u8>, mut errors: Vec<UpsPatchError>) -> Result<Vec<u8>, Self> {
        // There's no order for errors, so we just pop fst_error from errors.
        match errors.pop() {
            Some(fst_error) => Err(UpsPatchErrors {
                output,
                fst_error,
                errors,
            }),
            None => Ok(output),
        }
    }

    /// Iterate over all patching errors by reference.
    pub fn errors(&self) -> impl Iterator<Item = &UpsPatchError> {
        std::iter::once(&self.fst_error).chain(&self.errors)
    }

    /// Consume self and return and iterator over all patching errors.
    pub fn into_errors(self) -> impl Iterator<Item = UpsPatchError> {
        std::iter::once(self.fst_error).chain(self.errors)
    }
}

impl Debug for UpsPatchErrors {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut dbg_errors = vec![&self.fst_error];
        dbg_errors.extend(&self.errors);
        f.debug_struct("UpsPatchErrors")
            .field("errors", &dbg_errors)
            .finish()
    }
}

impl Display for UpsPatchErrors {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.errors.is_empty() {
            write!(f, "{}", self.fst_error)?;
        } else {
            write!(f, "Multiple errors: {}", self.fst_error)?;
            for err in &self.errors {
                write!(f, ", {}", err)?;
            }
        }
        Ok(())
    }
}

impl Error for UpsPatchErrors {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.fst_error)
    }
}

/// Possible errors when applying or reverting an UPS patch.
#[derive(thiserror::Error, Debug)]
pub enum UpsPatchError {
    #[error("Source file {}", .0)]
    SourceMetadataMismatch(MetadataMismatch),
    #[error("Destination file {}", .0)]
    DestMetadataMismatch(MetadataMismatch),
}

pub type UpsPatchResult<T> = Result<T, UpsPatchErrors>;

/// Kinds of metadata mismatches for [`UpsPatchError`].
#[derive(Debug)]
pub enum MetadataMismatch {
    Size {
        expected: usize,
        actual: usize,
    },
    Checksum {
        expected: Checksum,
        actual: Checksum,
    },
}

impl MetadataMismatch {
    pub fn size(expected: usize, actual: usize) -> Option<Self> {
        if expected == actual {
            None
        } else {
            Some(MetadataMismatch::Size { expected, actual })
        }
    }

    pub fn checksum(expected: Checksum, actual: Checksum) -> Option<Self> {
        if expected == actual {
            None
        } else {
            Some(MetadataMismatch::Checksum { expected, actual })
        }
    }
}

impl Display for MetadataMismatch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            MetadataMismatch::Size { expected, actual } => {
                write!(f, "size mismatch: expected {}, got {}", expected, actual)
            }
            MetadataMismatch::Checksum { expected, actual } => write!(
                f,
                "checksum mismatch: expected {}, got {}",
                expected, actual
            ),
        }
    }
}

/// Parsed UPS patch file. Use [`parse`] or [`from_files`] to instantiate, [`apply`] and
/// [`revert`] to use the patch.
///
/// Besides metadata for validation, a patch consists of a series of [`Hunk`]s. Each hunk contains
/// an offset from the current _file pointer_ to the next different byte and a sequence of source
/// file bytes XORed with destination file bytes, meaning the patch can be applied by calculating
/// _src XOR patch_ and reverted by calculating _dst XOR patch_.
#[derive(Clone, PartialEq, Eq)]
pub struct Patch {
    /// All hunks for the patch, in order.
    pub hunks: Vec<Hunk>,
    /// Source file size.
    pub src_size: usize,
    /// Source file checksum.
    pub src_checksum: Checksum,
    /// Destination file size.
    pub dst_size: usize,
    /// Destination file checksum.
    pub dst_checksum: Checksum,
}

/// The body of UPS patches is a sequence of [`Hunk`]s. See documentation for [`Patch`] for how to
/// use hunks.
#[derive(Clone, PartialEq, Eq)]
pub struct Hunk {
    pub offset: usize,
    pub xor_data: Vec<u8>,
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
    pub fn from_files(src: &[u8], dst: &[u8]) -> Self {
        use crate::util::SliceDiffs;

        let mut hunks = Vec::new();
        let mut prev_end = 0;
        for diff_range in SliceDiffs::new(src, dst) {
            let offset = diff_range.start - prev_end;
            let xor_data = src[diff_range.clone()]
                .iter()
                .zip(&dst[diff_range.clone()])
                .map(|(a, b)| a ^ b)
                .collect();
            // We know that data doesn't contain zeroes, because that would imply we got a
            // SliceDiff with some equal bytes.
            hunks.push(Hunk { offset, xor_data });
            prev_end = diff_range.end;
        }

        // Emit leftover hunks if either file has pending data.
        let (min_len, max_slice) = if src.len() < dst.len() {
            (src.len(), dst)
        } else {
            (dst.len(), src)
        };
        let mut offset = min_len - prev_end;
        // We need to split on 0 because UPS hunks are zero-terminated.
        let mut pending_data = &max_slice[min_len..];
        while !pending_data.is_empty() {
            let split_pos = memchr::memchr(0, pending_data).map_or(pending_data.len(), |x| x + 1);
            let (xor_data, next_pending_data) = pending_data.split_at(split_pos);
            pending_data = next_pending_data;
            hunks.push(Hunk {
                offset,
                xor_data: xor_data.to_vec(),
            });
            // Only the first offset is possibly non-zero, the remaining hunks are contiguous.
            offset = 0;
        }

        Patch {
            hunks,
            src_size: src.len(),
            src_checksum: Checksum::from_bytes(src),
            dst_size: dst.len(),
            dst_checksum: Checksum::from_bytes(dst),
        }
    }

    /// Serialize this patch to the UPS patch file contents.
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = b"UPS1".to_vec();
        varint::write_bytes(&mut bytes, self.src_size);
        varint::write_bytes(&mut bytes, self.dst_size);
        for hunk in &self.hunks {
            varint::write_bytes(&mut bytes, hunk.offset);
            bytes.extend(&hunk.xor_data);
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
                "hunks",
                &MaybeTruncate {
                    max_elements: 16,
                    slice: &self.hunks,
                },
            )
            .finish()
    }
}

impl Debug for Hunk {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Hunk")
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

#[cfg(test)]
mod test {
    use super::*;

    use std::matches;

    use proptest::array;
    use proptest::collection::vec;
    use proptest::prelude::*;

    proptest! {
        // TODO generate problematic data for testing, this is just a placeholder dumb generator
        #[test]
        fn test_garbage_valid_magic(mut raw in vec(any::<u8>(), 0..4096)) {
            if raw.len() >= 4 {
                // Set magic preamble to test other failure cases
                raw[..4].copy_from_slice(b"UPS1");
            }
            Patch::parse(&raw).unwrap_err();
        }

        #[test]
        fn test_patch_invalid_magic(magic in invalid_magic(), patch in patches()) {
            let mut serialized = patch.serialize();
            serialized[..4].copy_from_slice(&magic);
            let err = Patch::parse(&serialized).unwrap_err();
            prop_assert!(matches!(err, UpsParseError::FormatMismatch(_)));
        }

        #[test]
        fn test_parse_serialize_roundtrip(patch in patches()) {
            let serialized = patch.serialize();
            let parsed = Patch::parse(&serialized).unwrap();
            prop_assert_eq!(patch.src_size, parsed.src_size);
            prop_assert_eq!(patch.src_checksum, parsed.src_checksum);
            prop_assert_eq!(patch.dst_size, parsed.dst_size);
            prop_assert_eq!(patch.dst_checksum, parsed.dst_checksum);
            prop_assert_eq!(patch.hunks, parsed.hunks);
        }

        #[test]
        fn test_from_equal_files_results_in_empty_patch(f in files()) {
            let patch = Patch::from_files(&f, &f);
            prop_assert_eq!(patch.hunks, Vec::new());
        }

        #[test]
        fn test_from_files_apply_results_in_dst(src in files(), dst in files()) {
            let patch = Patch::from_files(&src, &dst);
            let applied = patch.apply(&src).unwrap();
            prop_assert_eq!(applied, dst);
        }

        #[test]
        fn test_from_files_revert_results_in_src(src in files(), dst in files()) {
            let patch = Patch::from_files(&src, &dst);
            let applied = patch.revert(&dst).unwrap();
            prop_assert_eq!(applied, src);
        }

        #[test]
        fn test_patch_checksum_err(patch in patches(), checksum in file_checksums()) {
            let mut serialized = patch.serialize();
            // Overwrite patch checksum
            let offset = serialized.len() - 4;
            let real_checksum = Checksum(u32::from_le_bytes(serialized[offset..].try_into().unwrap()));
            serialized[offset..].copy_from_slice(&checksum.0.to_le_bytes());
            let err = Patch::parse(&serialized).unwrap_err();
            match err {
                UpsParseError::PatchChecksumMismatch { parsed_patch, expected, actual } => {
                    prop_assert_ne!(actual, checksum);
                    prop_assert_ne!(expected, real_checksum);
                    prop_assert_eq!(parsed_patch, patch);
                }
                _ => prop_assert!(false, "Expected PatchChecksumMismatch, got {}", err),
            }
        }
    }

    fn invalid_magic() -> impl Strategy<Value = [u8; 4]> {
        array::uniform4(any::<u8>()).prop_filter("Valid magic", |v| v != b"UPS1")
    }

    prop_compose! {
        fn patches()
            (hunks in vec(patch_hunks(usize::MAX), 1..64),
             src_size in file_sizes(),
             src_checksum in file_checksums(),
             dst_size in file_sizes(),
             dst_checksum in file_checksums())
            -> Patch
        {
            Patch {
                hunks,
                src_size,
                src_checksum,
                dst_size,
                dst_checksum,
            }
        }
    }

    fn files() -> impl Strategy<Value = Vec<u8>> {
        vec(any::<u8>(), 0..512)
    }

    fn file_sizes() -> impl Strategy<Value = usize> {
        1..32usize
    }

    fn file_checksums() -> impl Strategy<Value = Checksum> {
        (0..32u32).prop_map(Checksum)
    }

    prop_compose! {
        fn patch_hunks(max_offset: usize)
            (offset in 0..max_offset, xor_data in xor_data())
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
