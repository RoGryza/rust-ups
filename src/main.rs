use std::fs::File;
use std::io::BufReader;

use ups::apply_patch;
use ups::parser::Parser;

fn main() {
    for name in &["rr-2-2b", "YAFRROFR", "unbound"] {
        println!("Running {}...", name);
        let patch = BufReader::new(File::open(&format!("samples/{}.ups", name)).unwrap());
        let src = BufReader::new(File::open("samples/rom.bin").unwrap());
        let mut dst = File::create(&format!("out/{}.bin", name)).unwrap();
        let parser = Parser::init(patch).unwrap();

        if let Err(e) = apply_patch(parser, src, &mut dst) {
            println!("ERR: {}", e);
        }
    }
}
