use std::fs::File;
use std::io::{Read, Write};

use ups::{apply_patch, Patch};

fn main() {
    let mut raw_src = Vec::new();
    File::open("samples/rom.bin")
        .unwrap()
        .read_to_end(&mut raw_src)
        .unwrap();
    for name in &["rr-2-2b", "YAFRROFR", "unbound"] {
        println!("Running {}...", name);
        let mut raw_patch = Vec::new();
        File::open(&format!("samples/{}.ups", name))
            .unwrap()
            .read_to_end(&mut raw_patch)
            .unwrap();
        let mut dst = File::create(&format!("out/{}.bin", name)).unwrap();
        let patch = Patch::parse(&raw_patch).unwrap();

        match apply_patch(patch, &raw_src) {
            Ok(d) => dst.write_all(&d).unwrap(),
            Err(e) => println!("ERR: {}", e),
        }
    }
}
