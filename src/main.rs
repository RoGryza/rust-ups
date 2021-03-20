use std::fs::File;
use std::io::BufReader;

use ups::apply_patch;
use ups::parser::Parser;

fn main() {
    let patch = BufReader::new(File::open("samples/rr-2-2b.ups").unwrap());
    let src = BufReader::new(File::open("samples/rom.bin").unwrap());
    let mut dst = File::create("tmp.bin").unwrap();
    let parser = Parser::init(patch).unwrap();

    apply_patch(parser, src, &mut dst).unwrap();
}
