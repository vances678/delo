use core::fmt;

use crate::ast::token::Token;
use crate::ast::stmt::EnumVariant;

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named {
        identifier: Token,
        type_parameters: Option<Vec<TypeExpr>>,
        type_arguments: Vec<TypeExpr>,
        is_optional: bool,
        enum_variants: Option<Vec<EnumVariant>>,
        struct_fields: Option<Vec<(Token, TypeExpr)>>
    },
    Function {
        parameter_types: Vec<TypeExpr>,
        return_type: Box<TypeExpr>,
    },
    TypeVar {
        id: usize,
    }
}

impl fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Named { identifier, type_arguments, is_optional, .. } => {
                write!(f, "{}", identifier.lexeme)?;

                if !type_arguments.is_empty() {
                    write!(f, "<")?;
                    for (index, argument) in type_arguments.iter().enumerate() {
                        if index > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{argument}")?;
                    }
                    write!(f, ">")?;
                }

                if *is_optional {
                    write!(f, "?")?;
                }

                Ok(())
            }
            Self::Function { parameter_types, return_type } => {
                write!(f, "(")?;
                for (index, parameter_type) in parameter_types.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{parameter_type}")?;
                }
                write!(f, ") -> {return_type}")
            }
            Self::TypeVar { id } => {
                write!(f, "?{id}")
            }
        }
    }
}