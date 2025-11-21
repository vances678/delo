use std::rc::Rc;
use std::cell::RefCell;
use std::fmt;

use crate::environment::Environment;
use crate::lox_callable::LoxCallable;
use crate::interpreter::{Interpreter, RuntimeError, RuntimeEvent, Value};
use crate::stmt::FunctionStmt;

pub struct LoxFunction {
    declaration: Rc<FunctionStmt>,
    closure: Rc<RefCell<Environment>>,
}

impl LoxFunction {
    pub fn new(declaration: Rc<FunctionStmt>, closure: Rc<RefCell<Environment>>) -> Self {
        Self { declaration, closure }
    }
}

impl LoxCallable for LoxFunction {
    fn arity(&self) -> usize {
        self.declaration.params.len()
    }

    fn call(&self, interpreter: &mut Interpreter, arguments: Vec<Value>) -> Result<Value, RuntimeError> {
        let mut environment = Environment::new_enclosed(self.closure.clone());
        for (i, arg) in arguments.iter().enumerate() {
            environment.define(self.declaration.params[i].lexeme.clone(), arg.clone());
        }

        match interpreter.execute_block(&self.declaration.body, Rc::new(RefCell::new(environment))) {
            Ok(_) => Ok(Value::Nil),
            Err(event) => match event {
                RuntimeEvent::Error(error) => Err(error),
                RuntimeEvent::Return(return_value) => Ok(return_value.value),
            },
        }
    }
}

impl fmt::Display for LoxFunction {     
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<fn {}>", self.declaration.name.lexeme)
    }
}