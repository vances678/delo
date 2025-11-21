use std::collections::HashMap;

use crate::interpreter::Interpreter;
use crate::stmt::Stmt;
use crate::expr::Expr;
use crate::token::Token;
use crate::error::error_token;
use crate::stmt::FunctionStmt;

#[derive(PartialEq, Copy, Clone)]
enum FunctionType {
    None,
    Function,
}

pub struct Resolver<'a> {
    interpreter: &'a mut Interpreter,
    scopes: Vec<HashMap<String, bool>>,
    current_function: FunctionType,
}

impl<'a> Resolver<'a> {
    pub fn new(interpreter: &'a mut Interpreter) -> Self {
        Self { interpreter, scopes: Vec::new(), current_function: FunctionType::None }
    }

    pub fn resolve(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            self.resolve_stmt(stmt);
        }
    }

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Block(statements) => {
                self.begin_scope();
                self.resolve(statements);
                self.end_scope();
            }
            Stmt::Var { name, initializer } => {
                self.declare(name);
                if let Some(initializer) = initializer {
                    self.resolve_expr(initializer);
                }
                self.define(name);
            }
            Stmt::Function(function) => {
                self.declare(&function.name);
                self.define(&function.name);
                self.resolve_function(function.as_ref(), FunctionType::Function);
            }
            Stmt::Expression(expr) => self.resolve_expr(expr),
            Stmt::If { condition, then_branch, else_branch } => {
                self.resolve_expr(condition);
                self.resolve_stmt(then_branch);
                if let Some(else_branch) = else_branch {
                    self.resolve_stmt(else_branch);
                }
            }
            Stmt::Print(expr) => self.resolve_expr(expr),
            Stmt::Return { keyword, value } => {
                if self.current_function == FunctionType::None {
                    error_token(keyword, "Can't return from top-level code.");
                }
                if let Some(value) = value {
                    self.resolve_expr(value);
                }
            }
            Stmt::While { condition, body } => {
                self.resolve_expr(condition);
                self.resolve_stmt(body);
            }
        }
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Variable(name) => {
                if let Some(scope) = self.scopes.last() {
                    if scope.get(&name.lexeme) == Some(&false) {
                        error_token(name, "Can't read local variable in its own initializer.");
                    }
                }

                self.resolve_local(expr, name);
            }
            Expr::Assign { name, value } => {
                self.resolve_expr(value);
                self.resolve_local(expr, name);
            }
            Expr::Binary { left, operator: _, right } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::Call { callee, paren: _, arguments } => {
                self.resolve_expr(callee);
                for arg in arguments {
                    self.resolve_expr(arg);
                }
            }
            Expr::Grouping(expression) => self.resolve_expr(expression),
            Expr::Literal(_) => {}
            Expr::Logical { left, operator: _, right } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::Unary { operator: _, right } => {
                self.resolve_expr(right);
            }
        }
    }

    fn resolve_local(&mut self, expr: &Expr, name: &Token) {
        for (i, scope) in self.scopes.iter().enumerate().rev() {
            if scope.contains_key(&name.lexeme) {
                self.interpreter.resolve(expr, self.scopes.len() - 1 - i);
                return;
            }
        }
    }

    fn resolve_function(&mut self, function: &FunctionStmt, func_type: FunctionType) {
        let enclosing_function = self.current_function;
        self.current_function = func_type;

        self.begin_scope();
        for param in &function.params {
            self.declare(param);
            self.define(param);
        }
        self.resolve(&function.body);
        self.end_scope();

        self.current_function = enclosing_function;
    }

    fn begin_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn end_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(&name.lexeme) {
                error_token(name, "Already a variable with this name in this scope.");
            }
            scope.insert(name.lexeme.clone(), false);
        }
    }

    fn define(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.lexeme.clone(), true);
        }
    }
}
