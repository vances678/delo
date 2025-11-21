use std::fmt;

use crate::token::{Token, Literal};

#[derive(Clone)]
pub enum Expr {
    Literal(Literal),
    Variable(Token),
    Assign {
        name: Token,
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
}

// this is in place of the book's AstPrinter print(Expr expr) method
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Literal(literal) => match literal {
                Literal::Number(number) => write!(f, "{}", number),
                Literal::String(string) => write!(f, "{}", string),
                Literal::Boolean(boolean) => write!(f, "{}", boolean),
                Literal::Nil => write!(f, "nil"),
            }
            Expr::Variable(name) => write!(f, "{}", name.lexeme),
            Expr::Assign { name, value } => {
                write!(f, "(= {} {})", name.lexeme, value)
            }
            Expr::Grouping(expr) => {
                write!(f, "(group {})", expr)
            }
            Expr::Unary { operator, right } => {
                write!(f, "({} {})", operator.lexeme, right)
            }
            Expr::Binary { left, operator, right } =>  {
                write!(f, "({} {} {})", operator.lexeme, left, right)
            }
            Expr::Logical { left, operator, right } => {
                write!(f, "({} {} {})", operator.lexeme, left, right)
            }
            Expr::Call { callee, paren: _, arguments } => {
                write!(f, "(call {}", callee)?;

                for arg in arguments {
                    write!(f, " {}", arg)?;
                }

                write!(f, ")")
            }
        }
    }
}