use std::env;
use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Error: Expected exactly one argument (a local path)\nUsage: {} <path>", args[0]);
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    
    // TODO: Add path validation and processing logic here
    println!("Received path: {}", path.display());
}
