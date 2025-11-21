use std::rc::Rc;

use crate::expr::Expr;
use crate::token::Token;

#[derive(Clone)]
pub enum Stmt {
    Block(Vec<Stmt>),
    Expression(Expr),
    Function(Rc<FunctionStmt>),
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    Print(Expr),
    Return {
        keyword: Token,
        value: Option<Expr>,
    },
    Var { 
        name: Token, 
        initializer: Option<Expr> 
    },
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
}

#[derive(Clone)]
pub struct FunctionStmt {
    pub name: Token,
    pub params: Vec<Token>,
    pub body: Vec<Stmt>,
}
