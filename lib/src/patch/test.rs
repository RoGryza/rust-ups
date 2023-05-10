use super::*;

use std::matches;

use proptest::array;
use proptest::collection::vec;
use proptest::prelude::*;

use crate::util::ProptestUnwrapExt;

proptest! {
    // TODO generate problematic data for testing, this is just a placeholder dumb generator
    #[test]
    fn test_garbage_valid_magic(mut raw in vec(any::<u8>(), 0..4096)) {
        if raw.len() >= 4 {
            // Set magic preamble to test other failure cases
            raw[..4].copy_from_slice(b"UPS1");
        }
        Patch::parse(&raw).prop_unwrap_err()?;
    }

    #[test]
    fn test_patch_invalid_magic(magic in invalid_magic(), patch in patches()) {
        let mut serialized = patch.serialize();
        serialized[..4].copy_from_slice(&magic);
        let err = Patch::parse(&serialized).prop_unwrap_err()?;
        prop_assert!(matches!(err, UpsParseError::FormatMismatch(_)));
    }

    #[test]
    fn test_parse_serialize_roundtrip(patch in patches()) {
        let serialized = patch.serialize();
        let parsed = Patch::parse(&serialized).prop_unwrap()?;
        prop_assert_eq!(patch.src_size, parsed.src_size);
        prop_assert_eq!(patch.src_checksum, parsed.src_checksum);
        prop_assert_eq!(patch.dst_size, parsed.dst_size);
        prop_assert_eq!(patch.dst_checksum, parsed.dst_checksum);
        prop_assert_eq!(patch.blocks, parsed.blocks);
    }

    #[test]
    fn test_from_equal_files_results_in_empty_patch(f in files()) {
        let patch = Patch::diff(&f, &f);
        prop_assert_eq!(patch.blocks, Vec::new());
    }

    #[test]
    fn test_diff_apply_results_in_dst(src in files(), dst in files()) {
        let patch = Patch::diff(&src, &dst);
        match patch.apply(&src) {
            Ok(p) => prop_assert_eq!(p, dst),
            Err(e) => prop_assert!(false, "{:?}", e.output),
        }
    }

    #[test]
    fn test_diff_revert_results_in_src(src in files(), dst in files()) {
        let patch = Patch::diff(&src, &dst);
        let applied = patch.revert(&dst).prop_unwrap()?;
        prop_assert_eq!(applied, src);
    }

    #[test]
    fn test_diff_blocks_xor_data_should_end_in_0(src in files(), dst in files()) {
        let patch = Patch::diff(&src, &dst);
        for block in patch.blocks {
            prop_assert_eq!(
                block.xor_data.last(), Some(&0),
                "block should end in 0: {:?}", block.xor_data,
            );
        }
    }

    #[test]
    fn test_diff_empty_src_should_result_in_dst_split_by_0(blocks in vec(xor_data(), 0..8usize)) {
        let dst: Vec<_> = blocks.iter().flatten().copied().collect();
        let patch = Patch::diff(&[], &dst);
        let expected_blocks: Vec<_> = blocks.into_iter().map(|xor_data| {
            Block { offset: 0, xor_data }
        }).collect();
        prop_assert_eq!(patch.blocks, expected_blocks);
    }

    #[test]
    fn test_patch_checksum_err(patch in patches(), checksum in file_checksums()) {
        let mut serialized = patch.serialize();
        // Overwrite patch checksum
        let offset = serialized.len() - 4;
        let real_checksum = Checksum(u32::from_le_bytes(serialized[offset..].try_into().prop_unwrap()?));
        serialized[offset..].copy_from_slice(&checksum.0.to_le_bytes());
        let err = Patch::parse(&serialized).prop_unwrap_err()?;
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
        (blocks in vec(patch_blocks(usize::MAX), 1..64),
         src_size in file_sizes(),
         src_checksum in file_checksums(),
         dst_size in file_sizes(),
         dst_checksum in file_checksums())
        -> Patch
    {
        Patch {
            blocks,
            src_size,
            src_checksum,
            dst_size,
            dst_checksum,
        }
    }
}

fn files() -> impl Strategy<Value = Vec<u8>> {
    vec(any::<u8>(), 0..32)
}

fn file_sizes() -> impl Strategy<Value = usize> {
    1..32usize
}

fn file_checksums() -> impl Strategy<Value = Checksum> {
    (0..32u32).prop_map(Checksum)
}

prop_compose! {
    fn patch_blocks(max_offset: usize)
        (offset in 0..max_offset, xor_data in xor_data())
            -> Block
            {
                Block {
                    offset,
                    xor_data,
                }
            }
}

fn xor_data() -> impl Strategy<Value = Vec<u8>> {
    vec(1..=255u8, 1..64).prop_map(|mut v| {
        // blocks are zero-terminated
        v.push(0);
        v
    })
}
