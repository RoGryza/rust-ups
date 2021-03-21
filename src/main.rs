use std::process::exit;

use ups::cli::Args;

fn main() {
    let args = Args::from_args();
    match args.run() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        }
    }
}
