use std::fs::File;
use std::io::BufReader;

use ups::apply_patch;
use ups::parser::Parser;

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
    let patch = BufReader::new(File::open(&format!("samples/{}", patch)).unwrap());
    let src = BufReader::new(File::open("samples/rom.bin").unwrap());
    let mut dst = Vec::new();
    let parser = Parser::init(patch).unwrap();

    apply_patch(parser, src, &mut dst).unwrap();
}
