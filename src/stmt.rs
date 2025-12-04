use std::rc::Rc;

use crate::expr::Expr;
use crate::token::Token;
use crate::type_checker::TypeExpr;

#[derive(Clone)]
pub enum Stmt {
    Var { 
        name: Token, 
        type_annotation: Option<TypeExpr>,
        initializer: Option<Expr> 
    },
    Enum {
        name: Token,
        type_parameters: Vec<Token>,
        variants: Vec<EnumVariant>,
    },
    Struct {
        name: Token,
        type_parameters: Vec<Token>,
        fields: Vec<(Token, TypeExpr)>,
    },
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    Function(Rc<FunctionStmt>),
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
    Block(Vec<Stmt>),
    Expression(Expr),
    Break(Token),
    Continue(Token),
    ForRange {
        name: Token,
        range: Expr,
        body: Box<Stmt>,
    }
}

#[derive(Clone)]
pub struct FunctionStmt {
    pub name: Token,
    pub type_parameters: Vec<Token>,
    pub parameters: Vec<(Token, TypeExpr)>,
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Stmt>,
}

#[derive(Clone)]
pub struct EnumVariant {
    pub name: Token,
    pub payload_types: Vec<TypeExpr>,
}