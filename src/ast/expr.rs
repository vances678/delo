use crate::ast::token::{Token};
use crate::ast::stmt::Stmt;
use crate::type_system::types::TypeExpr;

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Token),
    ArrayLiteral {
        elements: Vec<Expr>,
        left_bracket: Token,
    },
    MapLiteral {
        elements: Vec<(Expr, Expr)>,
        left_brace: Token,
    },
    RangeLiteral {
        start: Box<Expr>,
        end: Box<Expr>,
        is_inclusive: bool,
        range_token: Token,
    },
    Variable(Token),
    StructInstantiation {
        identifier: Token,
        fields: Vec<(Token, Expr)>,
    },
    Assign {
        identifier: Token,
        value: Box<Expr>,
    },
    AssignIndex {
        target: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
        left_bracket: Token,
    },
    AssignTimeTravelAbsolute {
        target: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
        at_token: Token,
    },
    AssignTimeTravelRelative {
        target: Box<Expr>,
        offset: Box<Expr>,
        value: Box<Expr>,
        at_token: Token,
    },
    Grouping(Box<Expr>),
    Unary {
        operator: Token,
        right: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: Token,
        right: Box<Expr>,
    },
    Logical {
        left: Box<Expr>,
        operator: Token,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        paren: Token,
        arguments: Vec<Expr>,
    },
    If {
        if_token: Token,
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Match {
        match_token: Token,
        subject: Box<Expr>,
        cases: Vec<MatchCase>,
    },
    Lambda {
        parameters: Vec<(Token, TypeExpr)>,
        return_type: Option<TypeExpr>,
        body: Vec<Stmt>,
    },
    Block {
        stmts: Vec<Stmt>,
        left_brace: Token,
    },
    IndexAccess {
        target: Box<Expr>,
        index: Box<Expr>,
        left_bracket: Token,
    },
    FieldAccess {
        target: Box<Expr>,
        field: Token,
    },
    TimeTravelAbsolute {
        target: Box<Expr>,
        index: Box<Expr>,
        at_token: Token,
    },
    TimeTravelRelative {
        target: Box<Expr>,
        offset: Box<Expr>,
        at_token: Token,
    },
}

#[derive(Debug, Clone)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard_token: Option<Token>,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard(Token),
    Literal(Token),
    Variable(Token),
    EnumVariant {
        identifier: Token,
        arguments: Vec<Pattern>,
    },
    Range {
        start: Token,
        end: Token,
        is_inclusive: bool,
    },
}