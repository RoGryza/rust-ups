use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use ups::{Checksum, Patch};

type Roms = HashMap<Checksum, Vec<u8>>;
type Patches = Vec<(PathBuf, Patch)>;

#[ignore]
#[test]
fn test_samples() {
    let (roms, patches) = read_samples();

    for (path, patch) in patches {
        let rom = roms.get(&patch.src_checksum).expect(&format!(
            "No rom found for patch \"{}\"",
            path.file_name().unwrap().to_string_lossy(),
        ));
        let patched = patch.apply(&*rom).unwrap();
        let reverted = patch.revert(&patched).unwrap();
        assert_eq!(rom, &reverted);
        let from_files = Patch::from_files(&*rom, &patched);
        match patch
            .hunks
            .iter()
            .zip(&from_files.hunks)
            .enumerate()
            .find(|(_, (h1, h2))| h1 != h2)
        {
            Some((i, (orig_hunk, new_hunk))) => {
                eprintln!("First differing hunk: {}", i);
                eprintln!("    original: {:?}", orig_hunk);
                eprintln!("    from_files: {:?}", new_hunk);
            }
            None => eprintln!("Differing hunks after end"),
        }
        assert_eq!(patch, from_files);
    }
}

fn read_samples() -> (Roms, Patches) {
    let mut roms = HashMap::new();
    let mut patches = Vec::new();

    for entry in fs::read_dir("../samples").unwrap().map(Result::unwrap) {
        if entry.metadata().unwrap().is_file() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("ups") {
                let raw_patch = fs::read(&path).unwrap();
                let patch = Patch::parse(&raw_patch).unwrap();
                patches.push((path, patch));
            } else {
                let rom = fs::read(&path).unwrap();
                let checksum = Checksum::from_bytes(&rom);
                roms.insert(checksum, rom);
            }
        }
    }

    (roms, patches)
}
