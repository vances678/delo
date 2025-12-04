use std::sync::atomic::{AtomicBool, Ordering};

use crate::token::{Token, TokenType};

pub static HAD_ERROR: AtomicBool = AtomicBool::new(false);
// pub static HAD_RUNTIME_ERROR: AtomicBool = AtomicBool::new(false);

pub struct ParseError;

// pub struct RuntimeError {
//     pub token: Token,
//     pub message: String,
// }

// impl RuntimeError {
//     pub fn new(token: Token, message: &str) -> Self {
//         Self { token, message: message.to_string() }
//     }
// }

pub fn error(line: u32, message: &str) {
    report(line, "", message);
}

pub fn error_token(token: &Token, message: &str) {
    if token.token_type == TokenType::Eof {
        report(token.line, " at end", message);
    } else {
        report(token.line, &format!(" at '{}'", token.lexeme), message);
    }
}

// pub fn runtime_error(error: RuntimeError) {
//     eprintln!("{}\n[line {}]", error.message, error.token.line);
//     HAD_RUNTIME_ERROR.store(true, Ordering::SeqCst);
// }

fn report(line: u32, location: &str, message: &str) {
    eprintln!("[line {line}] Error{location}: {message}");
    HAD_ERROR.store(true, Ordering::SeqCst);
}