use std::fs::File;
use std::io::Read;

use ups::{apply_patch, revert_patch, Patch};

const RAW_SRC: &[u8] = include_bytes!("../samples/rom.bin");

#[test]
fn test_roundtrip_rr_2_2b() {
    test_roundtrip("rr-2-2b.ups");
}

#[test]
fn test_roundtrip_yafrrrofr() {
    test_roundtrip("YAFRROFR.ups");
}

#[test]
fn test_roundtrip_unbound() {
    test_roundtrip("unbound.ups");
}

fn test_roundtrip(patch: &str) {
    let mut raw_patch = Vec::new();
    File::open(&format!("samples/{}", patch))
        .unwrap()
        .read_to_end(&mut raw_patch)
        .unwrap();
    let patch = Patch::parse(&raw_patch).unwrap();

    let patched = apply_patch(&patch, RAW_SRC).unwrap();
    let reverted = revert_patch(&patch, &patched).unwrap();
    assert_eq!(RAW_SRC, reverted);
}
