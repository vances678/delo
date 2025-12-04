mod error;
mod token;
mod scanner;
mod stmt;
mod expr;
mod parser;
mod type_checker;

use std::env;
use std::fs;

use crate::scanner::Scanner;
pub use crate::parser::Parser;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: delo <program.delo>");
        std::process::exit(64);
    }
    
    let path = &args[1];
    let file_content = fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Error reading file '{path}': {err}");
        std::process::exit(66);
    });

    let mut scanner = Scanner::new(file_content);
    let tokens = scanner.scan_tokens();
    
    let mut parser = Parser::new(tokens);
    let _statements = parser.parse();
}