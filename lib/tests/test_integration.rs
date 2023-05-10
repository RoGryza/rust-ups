#![allow(warnings)]
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use ups::{Checksum, Patch};

type Roms = HashMap<Checksum, Vec<u8>>;
type Patches = Vec<(PathBuf, Patch)>;

#[ignore]
#[test]
fn test_samples() {
    let (srcs, dsts, patches) = read_samples();

    for (path, patch) in patches {
        let src = srcs.get(&patch.src_checksum).expect(&format!(
            "No rom found for patch \"{}\"",
            path.file_name().unwrap().to_string_lossy(),
        ));
        let dst = dsts.get(&patch.dst_checksum).expect(&format!(
            "No patched found for patch \"{}\"",
            path.file_name().unwrap().to_string_lossy(),
        ));
        let patched = patch.apply(&*src).unwrap();
        assert_eq!(dst, &patched);
        let reverted = patch.revert(&patched).unwrap();
        assert_eq!(src, &reverted);
        let diff = Patch::diff(&*src, &patched);
        match patch
            .blocks
            .iter()
            .zip(&diff.blocks)
            .enumerate()
            .find(|(_, (h1, h2))| h1 != h2)
        {
            Some((i, (orig_block, new_block))) => {
                eprintln!("First differing block: {}", i);
                eprintln!("    original: {:?}", orig_block);
                eprintln!("    from_files: {:?}", new_block);
            }
            None => eprintln!("Differing blocks after end"),
        }
        assert_eq!(patch, diff);
    }
}

fn read_samples() -> (Roms, Roms, Patches) {
    let mut srcs = HashMap::new();
    let mut dsts = HashMap::new();
    let mut patches = Vec::new();

    for entry in fs::read_dir("../samples").unwrap().map(Result::unwrap) {
        if entry.metadata().unwrap().is_file() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("ups") {
                let raw_patch = fs::read(&path).unwrap();
                let patch = Patch::parse(&raw_patch).unwrap();
                patches.push((path, patch));
            } else if path.extension().and_then(|s| s.to_str()) == Some("rom") {
                let src = fs::read(&path).unwrap();
                let checksum = Checksum::from_bytes(&src);
                srcs.insert(checksum, src);
            } else if path.extension().and_then(|s| s.to_str()) == Some("patched") {
                let dst = fs::read(&path).unwrap();
                let checksum = Checksum::from_bytes(&dst);
                dsts.insert(checksum, dst);
            } else {
                panic!("Unhandled file type {}", path.display());
            }
        }
    }

    (srcs, dsts, patches)
}
