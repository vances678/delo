mod error;
mod token;
mod scanner;
mod expr;
mod parser;
mod interpreter;
mod stmt;
mod environment;
mod lox_callable;
mod clock;
mod lox_function;
mod resolver;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::sync::atomic::Ordering;

use error::{HAD_ERROR, HAD_RUNTIME_ERROR};
use scanner::Scanner;
use parser::Parser;
use interpreter::Interpreter;
use resolver::Resolver;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 2 {
        println!("Usage: lox_interpreter [script]");
        std::process::exit(64);
    } 
    
    let mut interpreter = Interpreter::new();
    if args.len() == 2 {
        run_file(&args[1], &mut interpreter);
    } else {
        run_prompt(&mut interpreter);
    }
}

fn run_file(path: &str, interpreter: &mut Interpreter) {
    let file_content = fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Error reading file '{path}': {err}");
        std::process::exit(66);
    });
    
    run(&file_content, interpreter);

    if HAD_ERROR.load(Ordering::SeqCst) {
        std::process::exit(65);
    }
    if HAD_RUNTIME_ERROR.load(Ordering::SeqCst) {
        std::process::exit(70);
    }
}

fn run_prompt(interpreter: &mut Interpreter) {
    let mut line = String::new();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        line.clear();
        match io::stdin().read_line(&mut line) {
            Ok(0) => break, // EOF (ctrl+d on unix, ctrl+z on windows)
            Ok(_) => {
                let trimmed_line = line.trim();
                run(trimmed_line, interpreter);
                HAD_ERROR.store(false, Ordering::SeqCst);
            },
            Err(err) => {
                eprintln!("Error reading input: {err}");
                continue;
            }
        }
    }
}

fn run(source: &str, interpreter: &mut Interpreter) {
    let mut scanner = Scanner::new(source.to_string());
    let tokens = scanner.scan_tokens();
    
    let mut parser = Parser::new(tokens);
    let statements = parser.parse();

    if HAD_ERROR.load(Ordering::SeqCst) {
        return;
    }

    let mut resolver = Resolver::new(interpreter);
    resolver.resolve(&statements);

    if HAD_ERROR.load(Ordering::SeqCst) {
        return;
    }

    interpreter.interpret(&statements);
}