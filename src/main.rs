mod token;
mod scanner;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: delo <program.delo>");
        std::process::exit(64);
    }
}