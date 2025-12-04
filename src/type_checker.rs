use crate::token::Token;

#[derive(Clone)]
pub enum TypeExpr {
    Named {
        name: Token,
        type_args: Vec<TypeExpr>,
        is_optional: bool,
    },
    Function {
        parameter_types: Vec<TypeExpr>,
        return_type: Box<TypeExpr>,
    },
}