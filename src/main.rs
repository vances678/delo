mod error;
mod ast;
mod frontend;
mod type_system;
mod backend;

use std::env;
use std::fs;
use std::path::Path;

use crate::backend::compiler_runner::CompilerRunner;
use crate::error::unwrap_or_exit;
use crate::frontend::scanner::Scanner;
pub use crate::frontend::parser::Parser;
use crate::type_system::type_checker::TypeChecker;
use crate::backend::code_generator::CodeGenerator;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: delo <program.delo>");
        std::process::exit(64);
    }
    
    let file_path = &args[1];
    let file_content = fs::read_to_string(file_path).unwrap_or_else(|err| {
        eprintln!("Error reading file '{file_path}': {err}");
        std::process::exit(66);
    });
    let lines: Vec<&str> = file_content.lines().collect();

    let mut scanner = Scanner::new(file_content.clone());
    let tokens = unwrap_or_exit(scanner.scan_tokens(), &lines, file_path);
    
    let mut parser = Parser::new(tokens);
    let statements = unwrap_or_exit(parser.parse(), &lines, file_path);

    let mut type_checker = TypeChecker::new();
    let types = unwrap_or_exit(type_checker.check_program(&statements), &lines, file_path);

    let mut code_generator = CodeGenerator::new(&types);
    let c_src = unwrap_or_exit(code_generator.generate_program(&statements).map_err(|error| vec![error]), &lines, file_path);
    println!("{}", c_src);

    let path = Path::new(file_path);
    let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
    let compiler_runner = CompilerRunner::new();
    let executable_path = unwrap_or_exit(compiler_runner.compile(&c_src, &file_name).map_err(|error| vec![error]), &lines, file_path);

    println!("COMPILATION SUCCESSFUL! Run the program with: {}", executable_path);
}