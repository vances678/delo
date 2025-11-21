use std::time::{SystemTime, UNIX_EPOCH};
use std::fmt;

use crate::interpreter::{Interpreter, RuntimeError, Value};
use crate::lox_callable::LoxCallable;

pub struct Clock;

impl LoxCallable for Clock {
    fn arity(&self) -> usize {
        0
    }

    fn call(&self, _interpreter: &mut Interpreter, _arguments: Vec<Value>) -> Result<Value, RuntimeError> {
        Ok(Value::Number(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()))
    }
}

impl fmt::Display for Clock {     
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<native fn>")
    }
}