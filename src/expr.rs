use crate::stmt::Stmt;
use crate::token::{Token, Literal};
use crate::type_checker::TypeExpr;

#[derive(Clone)]
pub enum Expr {
    Literal(Literal),
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
    },
    Variable(Token),
    Assign {
        name: Token,
        value: Box<Expr>,
    },
    AssignIndex {
        target: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
    AssignTimeTravelAbsolute {
        target: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
    AssignTimeTravelRelative {
        target: Box<Expr>,
        offset: Box<Expr>,
        value: Box<Expr>,
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
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Match {
        expr: Box<Expr>,
        cases: Vec<MatchCase>,
    },
    Lambda {
        parameters: Vec<(Token, TypeExpr)>,
        return_type: Option<TypeExpr>,
        body: Vec<Stmt>,
    },
    Block(Vec<Stmt>),
    IndexAccess {
        target: Box<Expr>,
        index: Box<Expr>
    },
    FieldAccess {
        target: Box<Expr>,
        field: Token,
    },
    TimeTravelAbsolute {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    TimeTravelRelative {
        target: Box<Expr>,
        offset: Box<Expr>,
    },
}

#[derive(Clone)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Clone)]
pub enum Pattern {
    Wildcard(Token),
    Literal(Literal),
    Variable(Token),
    EnumVariant {
        name: Token,
        args: Vec<Pattern>,
    },
    Range {
        start: Literal,
        end: Literal,
        is_inclusive: bool,
    },
}