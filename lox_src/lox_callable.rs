use std::fmt;

use crate::interpreter::{Interpreter, RuntimeError, Value};

pub trait LoxCallable: fmt::Display {
    fn arity(&self) -> usize;
    fn call(&self, interpreter: &mut Interpreter, arguments: Vec<Value>) -> Result<Value, RuntimeError>;
}