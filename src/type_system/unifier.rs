use std::collections::HashMap;

use crate::ast::token::Token;
use crate::error::{TypeError, UnifyError};
use crate::type_system::types::TypeExpr;

pub struct Unifier {
    next_type_var_id: usize,
    substitutions: HashMap<usize, TypeExpr>,
}

impl Unifier {
    pub fn new() -> Self {
        Self {
            next_type_var_id: 0,
            substitutions: HashMap::new(),
        }
    }

    pub fn new_type_var(&mut self) -> TypeExpr {
        let id = self.next_type_var_id;
        self.next_type_var_id += 1;
        TypeExpr::TypeVar { id }
    }

    pub fn instantiate_type(&mut self, type_expr: &TypeExpr) -> TypeExpr {
        let mut mapping = HashMap::new();
        self.instantiate_type_with_mapping(type_expr, &mut mapping)
    }

    pub fn apply_substitutions(&self, type_expr: &TypeExpr) -> TypeExpr {
        match type_expr {
            TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields } => {
                TypeExpr::Named { 
                    identifier: identifier.clone(),
                    type_parameters: type_parameters.clone(),
                    type_arguments: type_arguments.iter().map(|t| self.apply_substitutions(t)).collect(), 
                    is_optional: *is_optional, 
                    enum_variants: enum_variants.clone(), 
                    struct_fields: struct_fields.clone() 
                }
            }
            TypeExpr::Function { parameter_types, return_type } => {
                TypeExpr::Function { 
                    parameter_types: parameter_types.iter().map(|t| self.apply_substitutions(t)).collect(), 
                    return_type: Box::new(self.apply_substitutions(return_type)) 
                }
            }
            TypeExpr::TypeVar { id } => {
                if let Some(substitution) = self.substitutions.get(id) {
                    self.apply_substitutions(substitution)
                } else {
                    type_expr.clone()
                }
            }
        }
    }

    pub fn unify_at(&mut self, type_a: &TypeExpr, type_b: &TypeExpr, token: &Token) -> Result<TypeExpr, TypeError> {
        self.unify(type_a, type_b).map_err(|e| match e {
            UnifyError::MismatchedTypes { expected, found } => TypeError::MismatchedTypes { 
                line: token.line,
                column: token.column,
                expected, 
                found 
            },
            UnifyError::InfiniteType { type_var_id, type_expr } => TypeError::InfiniteType { 
                line: token.line,
                column: token.column,
                type_var_id, 
                type_expr 
            },
        })
    }

    fn unify(&mut self, type_a: &TypeExpr, type_b: &TypeExpr) -> Result<TypeExpr, UnifyError> {
        let type_a = self.resolve_type_var_chains(type_a);
        let type_b = self.resolve_type_var_chains(type_b);

        match (&type_a, &type_b) {
            (TypeExpr::TypeVar { id: type_var_id_a }, other_type) => {
                self.bind_type_var_to_type(*type_var_id_a, other_type)
            }
            (other_type, TypeExpr::TypeVar { id: type_var_id_b }) => {
                self.bind_type_var_to_type(*type_var_id_b, other_type)
            }
            (TypeExpr::Named {
                identifier: identifier_a,
                type_parameters: type_parameters_a,
                type_arguments: type_arguments_a,
                is_optional: is_optional_a,
                enum_variants: enum_variants_a,
                struct_fields: struct_fields_a,
            },
            TypeExpr::Named {
                identifier: identifier_b,
                type_arguments: type_arguments_b,
                is_optional: is_optional_b,
                ..
            }) => {
                if identifier_a.lexeme != identifier_b.lexeme {
                    return Err(UnifyError::MismatchedTypes { expected: type_a, found: type_b });
                }

                if type_arguments_a.len() != type_arguments_b.len() {
                    return Err(UnifyError::MismatchedTypes { expected: type_a, found: type_b });
                }

                if is_optional_a != is_optional_b {
                    return Err(UnifyError::MismatchedTypes { expected: type_a, found: type_b });
                }

                let mut unified_type_arguments = Vec::new();
                for (argument_a, argument_b) in type_arguments_a.iter().zip(type_arguments_b.iter()) {
                    unified_type_arguments.push(self.unify(argument_a, argument_b)?);
                }

                Ok(TypeExpr::Named {
                    identifier: identifier_a.clone(),
                    type_parameters: type_parameters_a.clone(),
                    type_arguments: unified_type_arguments,
                    is_optional: *is_optional_a,
                    enum_variants: enum_variants_a.clone(),
                    struct_fields: struct_fields_a.clone(),
                })
            }
            (TypeExpr::Function {
                parameter_types: parameter_types_a,
                return_type: return_type_a,
            },
            TypeExpr::Function {
                parameter_types: parameter_types_b,
                return_type: return_type_b,
            }) => {
                if parameter_types_a.len() != parameter_types_b.len() {
                    return Err(UnifyError::MismatchedTypes { expected: type_a, found: type_b });
                }

                let mut unified_parameter_types = Vec::new();
                for (parameter_a, parameter_b) in parameter_types_a.iter().zip(parameter_types_b.iter()) {
                    unified_parameter_types.push(self.unify(parameter_a, parameter_b)?);
                }

                let unified_return_type = self.unify(&return_type_a, &return_type_b)?;

                Ok(TypeExpr::Function {
                    parameter_types: unified_parameter_types,
                    return_type: Box::new(unified_return_type),
                })
            }
            _ => Err(UnifyError::MismatchedTypes { expected: type_a, found: type_b })
        }
    }

    fn bind_type_var_to_type(&mut self, type_var_id: usize, type_expr: &TypeExpr) -> Result<TypeExpr, UnifyError> {
        let resolved_type = self.resolve_type_var_chains(type_expr);

        if let TypeExpr::TypeVar { id } = resolved_type {
            if id == type_var_id {
                return Ok(TypeExpr::TypeVar { id });
            }
        }

        if self.type_var_occurs_in_type(type_var_id, &resolved_type) {
            return Err(UnifyError::InfiniteType { type_var_id, type_expr: resolved_type });
        }

        self.substitutions.insert(type_var_id, resolved_type.clone());

        Ok(resolved_type)
    }

    fn type_var_occurs_in_type(&self, type_var_id: usize, type_expr: &TypeExpr) -> bool {
        let resolved_type = self.resolve_type_var_chains(type_expr);

        match resolved_type {
            TypeExpr::Named { type_arguments, .. } => {
                type_arguments.iter().any(|t| self.type_var_occurs_in_type(type_var_id, t))
            }
            TypeExpr::Function { parameter_types, return_type } => {
                parameter_types.iter().any(|t| self.type_var_occurs_in_type(type_var_id, t))
                || self.type_var_occurs_in_type(type_var_id, &return_type)
            }
            TypeExpr::TypeVar { id } => id == type_var_id,
        }
    }

    fn resolve_type_var_chains(&self, type_expr: &TypeExpr) -> TypeExpr {
        match type_expr {
            TypeExpr::TypeVar { id } => {
                if let Some(substitution) = self.substitutions.get(id) {
                    self.resolve_type_var_chains(substitution)
                } else {
                    type_expr.clone()
                }
            }
            _ => type_expr.clone()
        }
    }

    fn instantiate_type_with_mapping(&mut self, type_expr: &TypeExpr, mapping: &mut HashMap<usize, TypeExpr>) -> TypeExpr {
        match type_expr {
            TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields } => {
                TypeExpr::Named {
                    identifier: identifier.clone(),
                    type_parameters: type_parameters.clone(),
                    type_arguments: type_arguments.iter().map(|t| self.instantiate_type_with_mapping(t, mapping)).collect(),
                    is_optional: *is_optional,
                    enum_variants: enum_variants.clone(),
                    struct_fields: struct_fields.as_ref().map(|fields| {
                        fields.iter().map(|(identifier, field_type)| {
                            (identifier.clone(), self.instantiate_type_with_mapping(field_type, mapping))
                        }).collect()
                    }),
                }
            }
            TypeExpr::Function { parameter_types, return_type } => {
                TypeExpr::Function {
                    parameter_types: parameter_types.iter().map(|t| self.instantiate_type_with_mapping(t, mapping)).collect(),
                    return_type: Box::new(self.instantiate_type_with_mapping(return_type, mapping)),
                }
            }
            TypeExpr::TypeVar { id } => {
                if let Some(fresh) = mapping.get(id) {
                    fresh.clone()
                } else {
                    let fresh = self.new_type_var();
                    mapping.insert(*id, fresh.clone());
                    fresh
                }
            }
        }
    }
}