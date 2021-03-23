use std::fs;

use ups::Patch;

#[ignore]
#[test]
fn test_samples() {
    let raw_src = fs::read("../samples/rom.bin").unwrap();
    for entry in fs::read_dir("../samples").unwrap().map(Result::unwrap) {
        if entry.metadata().unwrap().is_file() {
            let filename = entry.file_name().into_string().unwrap();
            if filename.ends_with(".ups") {
                test_roundtrip(&raw_src, &filename);
            }
        }
    }
}

fn test_roundtrip(raw_src: &[u8], patch: &str) {
    println!("Testing file {}", patch);
    let raw_patch = fs::read(&format!("../samples/{}", patch)).unwrap();
    let patch = Patch::parse(&raw_patch).unwrap();

    let patched = patch.apply(&raw_src).unwrap();
    let reverted = patch.revert(&patched).unwrap();
    assert_eq!(raw_src, reverted);
}
