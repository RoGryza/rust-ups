use std::fs::File;
use std::io::Read;

use ups::{apply_patch, Patch};

const RAW_SRC: &[u8] = include_bytes!("../samples/rom.bin");

#[test]
fn test_apply_rr_2_2b() {
    test_apply("rr-2-2b.ups");
}

#[test]
fn test_apply_yafrrrofr() {
    test_apply("YAFRROFR.ups");
}

#[test]
fn test_apply_unbound() {
    test_apply("unbound.ups");
}

fn test_apply(patch: &str) {
    let mut raw_patch = Vec::new();
    File::open(&format!("samples/{}", patch))
        .unwrap()
        .read_to_end(&mut raw_patch)
        .unwrap();
    let patch = Patch::parse(&raw_patch).unwrap();

    apply_patch(patch, RAW_SRC).unwrap();
}
