mod checksum;
mod patch;
mod varint;

pub use checksum::{Checksum, ChecksumStream};
pub use patch::{Patch, UpsParseError};

#[derive(thiserror::Error, Debug)]
pub enum UpsApplyError {
    #[error(transparent)]
    PatchRead(#[from] UpsParseError),
    #[error("Source file size mismatch: expected {}, got {}", .expected, .actual)]
    SourceSizeMismatch { expected: usize, actual: usize },
    #[error("Source file checksum mismatch: expected {}, got {}", .expected, .actual)]
    SourceChecksumMismatch {
        expected: Checksum,
        actual: Checksum,
    },
    #[error("Destination file checksum mismatch: expected {}, got {}", .expected, .actual)]
    DestChecksumMismatch {
        expected: Checksum,
        actual: Checksum,
    },
}

pub type UpsApplyResult<T> = Result<T, UpsApplyError>;

pub fn apply_patch(patch: patch::Patch, src: &[u8]) -> UpsApplyResult<Vec<u8>> {
    if src.len() != patch.src_size {
        return Err(UpsApplyError::SourceSizeMismatch {
            expected: patch.src_size,
            actual: src.len(),
        });
    }

    let src_checksum = Checksum::from_bytes(&src);
    if src_checksum != patch.src_checksum {
        return Err(UpsApplyError::SourceChecksumMismatch {
            expected: patch.src_checksum,
            actual: src_checksum,
        });
    }

    let mut output = src.to_vec();
    output.resize(patch.dst_size, 0);

    let mut output_ptr: &mut [u8] = &mut output;
    for hunk in patch.hunks {
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
    if dst_checksum != patch.dst_checksum {
        return Err(UpsApplyError::DestChecksumMismatch {
            expected: patch.dst_checksum,
            actual: dst_checksum,
        });
    }

    Ok(output)
}
