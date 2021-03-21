use std::fs;

use ups::Patch;

const RAW_SRC: &[u8] = include_bytes!("../samples/rom.bin");

#[ignore]
#[test]
fn test_roundtrip_rr_2_2b() {
    test_roundtrip("rr-2-2b.ups");
}

#[ignore]
#[test]
fn test_roundtrip_yafrrrofr() {
    test_roundtrip("YAFRROFR.ups");
}

#[ignore]
#[test]
fn test_roundtrip_unbound() {
    test_roundtrip("unbound.ups");
}

fn test_roundtrip(patch: &str) {
    let raw_patch = fs::read(&format!("samples/{}", patch)).unwrap();
    let patch = Patch::parse(&raw_patch).unwrap();

    let patched = patch.apply(RAW_SRC).unwrap();
    let reverted = patch.revert(&patched).unwrap();
    assert_eq!(RAW_SRC, reverted);
}
