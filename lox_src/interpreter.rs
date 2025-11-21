use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;

use crate::expr::{Expr};
use crate::lox_function::LoxFunction;
use crate::stmt::Stmt;
use crate::token::{Literal, Token, TokenType};
use crate::error::runtime_error;
use crate::environment::Environment;
use crate::lox_callable::LoxCallable;
use crate::clock::Clock;

pub enum RuntimeEvent {
    Error(RuntimeError),
    Return(Return),
}

impl From<RuntimeError> for RuntimeEvent {
    fn from(error: RuntimeError) -> Self {
        Self::Error(error)
    }
}

impl From<Return> for RuntimeEvent {
    fn from(return_value: Return) -> Self {
        Self::Return(return_value)
    }
}

pub struct RuntimeError {
    pub token: Token,
    pub message: String,
}

impl RuntimeError {
    pub fn new(token: Token, message: &str) -> Self {
        Self { token, message: message.to_string() }
    }
}

pub struct Return {
    pub token: Token,
    pub value: Value,
}

impl Return {
    pub fn new(token: Token, value: Value) -> Self {
        Self { token, value }
    }
}

#[derive(Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Nil,
    Callable(Rc<dyn LoxCallable>),
}

// this is in place of the book's stringify function from chapter 7
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Number(number) => {
                if *number == 0.0 && number.is_sign_negative() {
                    write!(f, "-0")
                } else if number.fract() == 0.0 {
                    write!(f, "{}", *number as i64)
                } else {
                    write!(f, "{}", number)
                }
            }
            Value::String(string) => write!(f, "{}", string),
            Value::Boolean(boolean) => write!(f, "{}", boolean),
            Value::Callable(callable) => write!(f, "{}", callable),
        }
    }
}

pub struct Interpreter {
    pub globals: Rc<RefCell<Environment>>,
    environment: Rc<RefCell<Environment>>,
    locals: HashMap<*const Expr, usize>,
}

impl Interpreter {
    pub fn new() -> Self {
        let globals = Rc::new(RefCell::new(Environment::new()));

        globals.borrow_mut().define("clock".to_string(), Value::Callable(Rc::new(Clock)));

        Self { 
            globals: globals.clone(),
            environment: globals,
            locals: HashMap::new(),
        }
    }

    pub fn interpret(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            match self.execute(stmt) {
                Ok(_) => {}
                Err(event) => match event {
                    RuntimeEvent::Error(error) => runtime_error(error),
                    RuntimeEvent::Return(r) => runtime_error(RuntimeError::new(r.token, "Unexpected return in top-level code.")),
                },
            }
        }
    }

    pub fn resolve(&mut self, expr: &Expr, depth: usize) {
        self.locals.insert(expr as *const Expr, depth);
    }

    fn lookup_variable(&self, name: &Token, expr: &Expr) -> Result<Value, RuntimeError> {
        let distance = self.locals.get(&(expr as *const Expr));
        if let Some(distance) = distance {
            Environment::get_at(self.environment.clone(), *distance, name)
        } else {
            self.globals.borrow().get(name)
        }
    }

    fn execute(&mut self, stmt: &Stmt) -> Result<(), RuntimeEvent> {
        match stmt {
            Stmt::Block(statements) => {
                self.execute_block(statements, Rc::new(RefCell::new(Environment::new_enclosed(self.environment.clone()))))?;
                Ok(())
            }
            Stmt::Expression(expr) => {
                self.evaluate(expr)?;
                Ok(())
            }
            Stmt::Function(function) => {
                let func = LoxFunction::new(function.clone(), self.environment.clone());
                self.environment.borrow_mut().define(function.name.lexeme.clone(), Value::Callable(Rc::new(func)));
                Ok(())
            }
            Stmt::If { condition, then_branch, else_branch } => {
                if Self::is_truthy(&self.evaluate(condition)?) {
                    self.execute(then_branch)?;
                } else if let Some(else_branch) = else_branch {
                    self.execute(else_branch)?;
                }
                Ok(())
            }
            Stmt::Print(expr) => {
                let value = self.evaluate(expr)?;
                println!("{value}");
                Ok(())
            }
            Stmt::Return { keyword, value } => {
                let value = if let Some(expr) = value {
                    self.evaluate(expr)?
                } else {
                    Value::Nil
                };
                
                Err(RuntimeEvent::Return(Return::new(keyword.clone(), value)))
            }
            Stmt::Var { name, initializer } => {
                let value = if let Some(expr) = initializer {
                    self.evaluate(expr)?
                } else {
                    Value::Nil
                };

                self.environment.borrow_mut().define(name.lexeme.clone(), value);
                Ok(())
            }
            Stmt::While { condition, body } => {
                while Self::is_truthy(&self.evaluate(condition)?) {
                    self.execute(body)?;
                }
                Ok(())
            }
        }
    }

    pub fn execute_block(&mut self, statements: &[Stmt], environment: Rc<RefCell<Environment>>) -> Result<(), RuntimeEvent> {
        let previous = self.environment.clone();
        self.environment = environment;
        
        let result = (|| {
            for stmt in statements {
                self.execute(stmt)?;
            }
            Ok(())
        })();

        self.environment = previous;
        
        result
    }

    fn evaluate(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Literal(literal) => match literal {
                Literal::Number(number) => Ok(Value::Number(*number)),
                Literal::String(string) => Ok(Value::String(string.clone())),
                Literal::Boolean(boolean) => Ok(Value::Boolean(*boolean)),
                Literal::Nil => Ok(Value::Nil),
            },
            Expr::Variable(name) => self.lookup_variable(name, expr),
            Expr::Assign { name, value } => {
                let value = self.evaluate(value)?;

                let distance = self.locals.get(&(expr as *const Expr));
                if let Some(distance) = distance {
                    Environment::assign_at(self.environment.clone(), *distance, name, &value)?;
                } else {
                    self.globals.borrow_mut().assign(name, &value)?;
                }

                Ok(value)
            }
            Expr::Grouping(expression) => self.evaluate(expression),
            Expr::Unary { operator, right } => {
                let right_value = self.evaluate(right)?;
                
                match operator.token_type {
                    TokenType::Bang => Ok(Value::Boolean(!Self::is_truthy(&right_value))),
                    TokenType::Minus => {
                        let n = Self::check_number_operand(&operator, &right_value)?;
                        Ok(Value::Number(-n))
                    }
                    _ => Ok(Value::Nil),
                }
            }
            Expr::Binary { left, operator, right } => {
                let left_value = self.evaluate(left)?;
                let right_value = self.evaluate(right)?;

                match operator.token_type {
                    TokenType::BangEqual => Ok(Value::Boolean(!Self::is_equal(&left_value, &right_value))),
                    TokenType::EqualEqual => Ok(Value::Boolean(Self::is_equal(&left_value, &right_value))),
                    TokenType::Greater => {
                        let (l, r) = Self::check_number_operands(&operator, &left_value, &right_value)?;
                        Ok(Value::Boolean(l > r))
                    }
                    TokenType::GreaterEqual => {
                        let (l, r) = Self::check_number_operands(&operator, &left_value, &right_value)?;
                        Ok(Value::Boolean(l >= r))
                    }
                    TokenType::Less => {
                        let (l, r) = Self::check_number_operands(&operator, &left_value, &right_value)?;
                        Ok(Value::Boolean(l < r))
                    }
                    TokenType::LessEqual => {
                        let (l, r) = Self::check_number_operands(&operator, &left_value, &right_value)?;
                        Ok(Value::Boolean(l <= r))
                    }
                    TokenType::Minus => {
                        let (l, r) = Self::check_number_operands(&operator, &left_value, &right_value)?;
                        Ok(Value::Number(l - r))
                    }
                    TokenType::Plus => match (left_value, right_value) {
                            (Value::Number(l), Value::Number(r)) => Ok(Value::Number(l + r)),
                            (Value::String(l), Value::String(r)) => Ok(Value::String(format!("{l}{r}"))),
                            _ => Err(RuntimeError::new(operator.clone(), "Operands must be two numbers or two strings.")),
                        }
                    
                    TokenType::Slash => {
                        let (l, r) = Self::check_number_operands(&operator, &left_value, &right_value)?;
                        Ok(Value::Number(l / r))
                    }
                    TokenType::Star => {
                        let (l, r) = Self::check_number_operands(&operator, &left_value, &right_value)?;
                        Ok(Value::Number(l * r))
                    }
                    _ => Ok(Value::Nil),
                }
            }
            Expr::Logical { left, operator, right } => {
                let left_value = self.evaluate(left)?;
                
                if operator.token_type == TokenType::Or {
                    if Self::is_truthy(&left_value) {
                        return Ok(left_value);
                    }
                } else {
                    if !Self::is_truthy(&left_value) {
                        return Ok(left_value);
                    }
                }
                
                self.evaluate(right)
            }
            Expr::Call { callee, paren, arguments } => {
                let callee = self.evaluate(callee)?;

                let mut args = Vec::new();
                for arg in arguments {
                    args.push(self.evaluate(arg)?);
                }
                
                let function = match callee {
                    Value::Callable(func) => func,
                    _ => return Err(RuntimeError::new(paren.clone(), "Can only call functions and classes.")),
                };

                if args.len() != function.arity() {
                    return Err(RuntimeError::new(paren.clone(), &format!("Expected {} arguments but got {}.", function.arity(), args.len())));
                }

                function.call(self, args)
            }
        }
    }

    fn check_number_operand(operator: &Token, operand: &Value) -> Result<f64, RuntimeError> {
        match operand {
            Value::Number(number) => Ok(*number),
            _ => Err(RuntimeError::new(operator.clone(), "Operand must be a number.")),
        }
    }

    fn check_number_operands(operator: &Token, left: &Value, right: &Value) -> Result<(f64, f64), RuntimeError> {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => Ok((*l, *r)),
            _ => Err(RuntimeError::new(operator.clone(), "Operands must be numbers.")),
        }
    }

    fn is_truthy(value: &Value) -> bool {
        match value {
            Value::Nil => false,
            Value::Boolean(boolean) => *boolean,
            _ => true,
        }
    }

    fn is_equal(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Nil, Value::Nil) => true,
            (Value::Nil, _) => false,
            (Value::Number(l), Value::Number(r)) => l == r,
            (Value::String(l), Value::String(r)) => l == r,
            (Value::Boolean(l), Value::Boolean(r)) => l == r,
            (Value::Callable(l), Value::Callable(r)) => Rc::ptr_eq(l, r),
            _ => false,
        }
    }
}