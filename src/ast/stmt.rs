use std::rc::Rc;

use crate::ast::token::Token;
use crate::ast::expr::Expr;
use crate::type_system::types::TypeExpr;

#[derive(Debug, Clone)]
pub enum VariableBinding {
    Identifier(Token),
    Tuple {
        elements: Vec<VariableBinding>,
        left_paren: Token,
    },
}

impl VariableBinding {
    pub fn first_token(&self) -> &Token {
        match self {
            VariableBinding::Identifier(token) => token,
            VariableBinding::Tuple { left_paren, .. } => left_paren,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Variable {
        binding: VariableBinding,
        type_annotation: Option<TypeExpr>,
        initializer: Option<Expr>
    },
    Enum {
        identifier: Token,
        type_parameters: Vec<Token>,
        variants: Vec<EnumVariant>,
    },
    Struct {
        identifier: Token,
        type_parameters: Vec<Token>,
        fields: Vec<(Token, TypeExpr)>,
    },
    If {
        if_token: Token,
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        while_token: Token,
        condition: Expr,
        body: Box<Stmt>,
    },
    ForIn {
        binding: VariableBinding,
        iterable: Expr,
        body: Box<Stmt>,
    },
    Function(Rc<FunctionStmt>),
    Block(Vec<Stmt>),
    Expression(Expr),
    Break(Token),
    Continue(Token),
}

#[derive(Debug, Clone)]
pub struct FunctionStmt {
    pub identifier: Token,
    pub type_parameters: Vec<Token>,
    pub parameters: Vec<(Token, TypeExpr)>,
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub identifier: Token,
    pub payload_types: Vec<TypeExpr>,
}