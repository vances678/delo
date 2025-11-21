use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::interpreter::Value;
use crate::token::Token;
use crate::interpreter::RuntimeError;

pub struct Environment {
    enclosing: Option<Rc<RefCell<Environment>>>,
    values: HashMap<String, Value>,
}

impl Environment {
    pub fn new() -> Self {
        Self { 
            enclosing: None,
            values: HashMap::new()
        }
    }

    pub fn new_enclosed(enclosing: Rc<RefCell<Environment>>) -> Self {
        Self { 
            enclosing: Some(enclosing),
            values: HashMap::new()
        }
    }

    pub fn get(&self, name: &Token) -> Result<Value, RuntimeError> {
        if self.values.contains_key(&name.lexeme) {
            return Ok(self.values.get(&name.lexeme).unwrap().clone());
        } else if let Some(enclosing) = &self.enclosing {
            return enclosing.borrow().get(name);
        } else {
            return Err(RuntimeError::new(name.clone(), &format!("Undefined variable '{}'.", name.lexeme)));
        }
    }

    pub fn get_at(environment: Rc<RefCell<Environment>>, distance: usize, name: &Token) -> Result<Value, RuntimeError> {
        Ok(Self::ancestor(environment, distance).borrow().values.get(&name.lexeme).unwrap().clone())
    }
    
    pub fn define(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    pub fn assign(&mut self, name: &Token, value: &Value) -> Result<(), RuntimeError> {
        if self.values.contains_key(&name.lexeme) {
            self.values.insert(name.lexeme.clone(), value.clone());
            Ok(())
        } else if let Some(enclosing) = &self.enclosing {
            enclosing.borrow_mut().assign(name, value)
        } else {
            Err(RuntimeError::new(name.clone(), &format!("Undefined variable '{}'.", name.lexeme)))
        }
    }

    pub fn assign_at(environment: Rc<RefCell<Environment>>, distance: usize, name: &Token, value: &Value) -> Result<(), RuntimeError> {
        Self::ancestor(environment, distance).borrow_mut().values.insert(name.lexeme.clone(), value.clone());
        Ok(())
    }

    fn ancestor(environment: Rc<RefCell<Environment>>, distance: usize) -> Rc<RefCell<Environment>> {
        let mut env = environment;
        for _ in 0..distance {
            let next_env = env.borrow().enclosing.as_ref().unwrap().clone();
            env = next_env;
        }

        env
    }
}