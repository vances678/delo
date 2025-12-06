use std::collections::{HashMap, HashSet};
use std::{mem, vec};
use std::rc::Rc;

use crate::ast::token::{Literal, Token, TokenType};
use crate::ast::expr::{Expr, MatchCase, Pattern};
use crate::ast::stmt::{EnumVariant, FunctionStmt, Stmt};
use crate::error::TypeError;
use crate::type_system::types::TypeExpr;
use crate::type_system::unifier::Unifier;

struct SymbolTable {
    table: HashMap<String, TypeExpr>
}

impl SymbolTable {
    fn new() -> Self {
        Self {
            table: HashMap::new()
        }
    }

    fn insert(&mut self, name: String, value: TypeExpr) {
        self.table.insert(name, value);
    }

    // fn remove(&mut self, identifier: &Token) {
    //     self.table.remove(&identifier.lexeme);
    // }

    fn lookup(&self, identifier: &Token) -> Option<&TypeExpr> {
        self.table.get(&identifier.lexeme)
    }

    // fn contains(&self, identifier: &Token) -> bool {
    //     self.table.contains_key(&identifier.lexeme)
    // }
}

struct TypeEnvironment {
    stack: Vec<SymbolTable>
}

impl TypeEnvironment {
    fn new() -> Self {
        Self {
            stack: Vec::new()
        }
    }

    fn push_scope(&mut self) {
        self.stack.push(SymbolTable::new());
    }

    fn pop_scope(&mut self) {
        self.stack.pop();
    }

    fn insert(&mut self, identifier: &Token, type_expr: TypeExpr) {
        if let Some(current_scope) = self.stack.last_mut() {
            current_scope.insert(identifier.lexeme.clone(), type_expr);
        }
    }

    // fn remove(&mut self, identifier: &Token) {
    //     if let Some(current_scope) = self.stack.last_mut() {
    //         current_scope.remove(identifier);
    //     }
    // }

    fn lookup(&self, identifier: &Token) -> Option<&TypeExpr> {
        for scope in self.stack.iter().rev() {
            if let Some(type_expr) = scope.lookup(identifier) {
                return Some(type_expr);
            }
        }

        None
    }

    // fn contains(&mut self, identifier: &Token) -> bool {
    //     self.lookup(identifier).is_some()
    // }
}

#[derive(Default)]
pub struct TypeInfo {
    pub expr_types: HashMap<*const Expr, TypeExpr>,
    pub function_types: HashMap<String, TypeExpr>,
    pub lambda_captures: HashMap<*const Expr, Vec<(String, TypeExpr)>>
}

pub struct TypeChecker {
    environment: TypeEnvironment,
    enum_variants: HashMap<String, TypeExpr>,
    loop_depth: usize,
    lambda_captures_stack: Vec<Vec<(String, TypeExpr)>>,
    lambda_locals_stack: Vec<HashSet<String>>,
    unifier: Unifier,
    types: TypeInfo,
    errors: Vec<TypeError>
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut environment = TypeEnvironment::new();
        environment.push_scope();
        
        let mut type_checker = Self {
            environment,
            enum_variants: HashMap::new(),
            loop_depth: 0,
            lambda_captures_stack: Vec::new(),
            lambda_locals_stack: Vec::new(),
            unifier: Unifier::new(),
            types: TypeInfo {
                expr_types: HashMap::new(),
                function_types: HashMap::new(),
                lambda_captures: HashMap::new(),
            },
            errors: Vec::new()
        };

        type_checker.inject_builtin_enums().unwrap();
        type_checker.inject_builtin_functions().unwrap();

        type_checker
    }

    pub fn check_program(&mut self, stmts: &[Stmt]) -> Result<TypeInfo, Vec<TypeError>> {
        for stmt in stmts {
            if let Err(error) = self.check_stmt(stmt) {
                self.errors.push(error);
            }
        }

        if self.errors.is_empty() {
            for type_expr in self.types.expr_types.values_mut() {
                let substituted = self.unifier.apply_substitutions(type_expr);
                *type_expr = substituted
            }
            for func_type in self.types.function_types.values_mut() {
                let substituted = self.unifier.apply_substitutions(func_type);
                *func_type = substituted
            }
            for lambda_captures in self.types.lambda_captures.values_mut() {
                for (_, type_expr) in lambda_captures.iter_mut() {
                    let substituted = self.unifier.apply_substitutions(type_expr);
                    *type_expr = substituted
                }
            }
            let types = mem::take(&mut self.types);
            
            Ok(types)
        } else {
            Err(mem::take(&mut self.errors))
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), TypeError> {
        match stmt {
            Stmt::Variable { identifier, type_annotation, initializer } => self.check_var(identifier, type_annotation, initializer),
            Stmt::Enum { identifier, type_parameters, variants } => self.check_enum(identifier, type_parameters, variants),
            Stmt::Struct { identifier, type_parameters, fields } => self.check_struct(identifier, type_parameters, fields),
            Stmt::If { if_token, condition, then_branch, else_branch } => self.check_if(if_token, condition, then_branch, else_branch),
            Stmt::While { while_token, condition, body } => self.check_while(while_token, condition, body),
            Stmt::ForRange { identifier, range, body } => self.check_for_range(identifier, range, body),
            Stmt::Function(function) => self.check_function(function),
            Stmt::Block(statements) => self.check_block(statements),
            Stmt::Expression(expr) => {
                self.check_expr(expr)?;
                Ok(())
            },
            Stmt::Break(token) => {
                if self.loop_depth == 0 {
                    return Err(TypeError::BreakOutsideLoop { line: token.line, column: token.column });
                }
                Ok(())
            }
            Stmt::Continue(token) => {
                if self.loop_depth == 0 {
                    return Err(TypeError::ContinueOutsideLoop { line: token.line, column: token.column });
                }
                Ok(())
            }
        }
    }

    fn check_var(&mut self, identifier: &Token, type_annotation: &Option<TypeExpr>, initializer: &Option<Expr>) -> Result<(), TypeError> {
        let initializer_type = if let Some(initializer_expr) = initializer {
            Some(self.check_expr(initializer_expr)?)
        } else {
            None
        };

        let var_type = match (type_annotation, initializer_type) {
            (Some(annotation), Some(initializer)) => {
                self.unifier.unify_at(&initializer, &annotation, identifier)?
            }
            (Some(annotation), None) => annotation.clone(),
            (None, Some(initializer)) => initializer,
            (None, None) => {
                return Err(TypeError::MissingTypeAnnotationAndInitializer { 
                    line: identifier.line, 
                    column: identifier.column, 
                    identifier: identifier.lexeme.clone()  
                });
            }
        };

        self.environment.insert(identifier, var_type);
        
        if let Some(lambda_locals) = self.lambda_locals_stack.last_mut() {
            lambda_locals.insert(identifier.lexeme.clone());
        }

        Ok(())
    }

    fn check_enum(&mut self, identifier: &Token, type_parameters: &Vec<Token>, variants: &Vec<EnumVariant>) -> Result<(), TypeError> {
        let type_parameters: Vec<TypeExpr> = type_parameters
            .iter()
            .map(|_| self.unifier.new_type_var())
            .collect();
        
        let enum_type = TypeExpr::Named {
            identifier: identifier.clone(),
            type_parameters: Some(type_parameters),
            type_arguments: Vec::new(),
            is_optional: false,
            enum_variants: Some(variants.clone()),
            struct_fields: None,
        };

        self.register_enum_variants(&enum_type)?;
        self.environment.insert(identifier, enum_type);

        Ok(())
    }

    fn check_struct(&mut self, identifier: &Token, type_parameters: &Vec<Token>, fields: &Vec<(Token, TypeExpr)>) -> Result<(), TypeError> {
        let new_type_parameters: Vec<TypeExpr> = type_parameters
            .iter()
            .map(|_| self.unifier.new_type_var())
            .collect();

        let instantiated_fields: Vec<(Token, TypeExpr)> = fields
            .iter()
            .map(|(field_identifier, field_type)| {
                (field_identifier.clone(), self.substitute_type_parameters(field_type, type_parameters, &new_type_parameters))
            })
            .collect();
        
        let struct_type = TypeExpr::Named {
            identifier: identifier.clone(),
            type_parameters: Some(new_type_parameters),
            type_arguments: Vec::new(),
            is_optional: false,
            enum_variants: None,
            struct_fields: Some(instantiated_fields),
        };

        self.environment.insert(identifier, struct_type);

        Ok(())
    }

    fn check_if(&mut self, if_token: &Token, condition: &Expr, then_branch: &Stmt, else_branch: &Option<Box<Stmt>>) -> Result<(), TypeError> {
        let condition_type = self.check_expr(condition)?;
        match condition_type {
            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Bool" => {}
            _ => return Err(TypeError::ConditionNotBoolean { 
                line: if_token.line, 
                column: if_token.column, 
                condition_type 
            })
        }

        self.check_stmt(then_branch)?;
        if let Some(else_stmt) = else_branch {
            self.check_stmt(else_stmt)?;
        }

        Ok(())
    }

    fn check_while(&mut self, while_token: &Token, condition: &Expr, body: &Stmt) -> Result<(), TypeError> {
        let condition_type = self.check_expr(condition)?;
        match condition_type {
            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Bool" => {}
            _ => return Err(TypeError::ConditionNotBoolean { 
                line: while_token.line, 
                column: while_token.column, 
                condition_type
            }),
        }

        self.loop_depth += 1;
        let result = (|| { 
            self.check_stmt(body)
        })();
        self.loop_depth -= 1;

        result
    }

    fn check_for_range(&mut self, identifier: &Token, range: &Expr, body: &Stmt) -> Result<(), TypeError> {
        let range_type = self.check_expr(range)?;

        match range_type {
            TypeExpr::Named { identifier: type_identifier, mut type_arguments, .. }
                if (type_identifier.lexeme == "Range" || type_identifier.lexeme == "InclusiveRange") && type_arguments.len() == 1 =>
            {
                let element_type = type_arguments.remove(0);

                self.environment.push_scope();
                self.environment.insert(identifier, element_type);

                self.loop_depth += 1;
                let result = (|| { 
                    self.check_stmt(body)
                })();
                self.loop_depth -= 1;

                self.environment.pop_scope();

                result
            }
            _ => Err(TypeError::InvalidForRangeTarget {
                line: identifier.line,
                column: identifier.column,
                target_type: range_type,
            }),
        }
    }

    fn check_function(&mut self, function: &Rc<FunctionStmt>) -> Result<(), TypeError> {
        let new_type_parameters: Vec<TypeExpr> = function.type_parameters.iter().map(|_| self.unifier.new_type_var()).collect();
        
        let parameter_types: Vec<TypeExpr> = function.parameters
            .iter()
            .map(|(_, t)| { 
                self.substitute_type_parameters(t, &function.type_parameters, &new_type_parameters)
            })
            .collect();

        let annotated_return_type = function.return_type
            .as_ref()
            .map(|t| {
                self.substitute_type_parameters(t, &function.type_parameters, &new_type_parameters)
            });

        let recursion_return_type = annotated_return_type.clone()
            .unwrap_or_else(|| self.create_void_type(function.identifier.line, function.identifier.column));

        self.environment.insert(&function.identifier, TypeExpr::Function {
            parameter_types: parameter_types.clone(),
            return_type: Box::new(recursion_return_type.clone()),
        });

        self.environment.push_scope();
        for (parameter_name, parameter_type) in &function.parameters {
            let instantiated_param_type = self.substitute_type_parameters(parameter_type, &function.type_parameters, &new_type_parameters);
            self.environment.insert(parameter_name, instantiated_param_type);
        }

        let inferred_return_type = self.infer_block_return_type(&function.body, function.identifier.line, function.identifier.column)?;

        let unified_return_type = match annotated_return_type {
            Some(annotation) => self.unifier.unify_at(&inferred_return_type, &annotation, &function.identifier)?,
            None => inferred_return_type,
        };

        self.environment.pop_scope();

        self.environment.insert(&function.identifier, TypeExpr::Function {
            parameter_types: parameter_types.clone(),
            return_type: Box::new(unified_return_type.clone()),
        });

        self.types.function_types.insert(function.identifier.lexeme.clone(), TypeExpr::Function {
            parameter_types,
            return_type: Box::new(unified_return_type),
        });

        Ok(())
    }

    fn check_block(&mut self, statements: &[Stmt]) -> Result<(), TypeError> {
        self.environment.push_scope();

        for stmt in statements {
            self.check_stmt(stmt)?;
        }

        self.environment.pop_scope();
        Ok(())
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<TypeExpr, TypeError> {
        let expr_type = match expr {
            Expr::Literal(literal) => self.check_literal_expr(literal),
            Expr::ArrayLiteral { elements, left_bracket } => self.check_array_literal_expr(elements, left_bracket),
            Expr::MapLiteral { elements, left_brace } => self.check_map_literal_expr(elements, left_brace),
            Expr::RangeLiteral { start, end, is_inclusive, range_token } => self.check_range_literal_expr(start, end, is_inclusive, range_token),
            Expr::Variable(identifier) => self.check_variable_expr(identifier),
            Expr::StructInstantiation { identifier, fields } => self.check_struct_instantiation_expr(identifier, fields),
            Expr::Assign { identifier, value } => self.check_assign_expr(identifier, value),
            Expr::AssignIndex { target, index, value, left_bracket } => self.check_assign_index_expr(target, index, value, left_bracket),
            Expr::AssignTimeTravelAbsolute { target, index, value, at_token } => self.check_assign_time_travel_absolute_expr(target, index, value, at_token),
            Expr::AssignTimeTravelRelative { target, offset, value, at_token } => self.check_assign_time_travel_relative_expr(target, offset, value, at_token),
            Expr::Grouping(expr) => self.check_expr(expr),
            Expr::Unary { operator, right } => self.check_unary_expr(operator, right),
            Expr::Binary { left, operator, right } => self.check_binary_expr(left, operator, right),
            Expr::Logical { left, operator, right } => self.check_logical_expr(left, operator, right),
            Expr::Call { callee, paren, arguments } => {
                if let Expr::Variable(ref identifier) = **callee {
                    if identifier.lexeme == "print" {
                        if arguments.len() != 1 {
                            return Err(TypeError::ArgumentCountMismatch {
                                line: identifier.line,
                                column: identifier.column,
                                expected: 1,
                                found: arguments.len(),
                            });
                        }

                        self.check_expr(&arguments[0])?;
                       
                        let void_type = self.create_void_type(identifier.line, identifier.column);
                        return Ok(void_type);
                    }
                }

                self.check_call_expr(callee, paren, arguments) 
            },
            Expr::If { if_token, condition, then_branch, else_branch } => self.check_if_expr(if_token, condition, then_branch, else_branch),
            Expr::Match { match_token, subject, cases } => self.check_match_expr(match_token, subject, cases),
            Expr::Lambda { parameters, return_type, body } => {
                self.lambda_captures_stack.push(Vec::new());
                self.lambda_locals_stack.push(HashSet::new());

                let locals = self.lambda_locals_stack.last_mut().unwrap();
                for (parameter_identifier, _parameter_type) in parameters {
                    locals.insert(parameter_identifier.lexeme.clone());
                }

                let lambda_type = self.check_lambda_expr(parameters, return_type, body)?;

                let captures = self.lambda_captures_stack.pop().unwrap();
                self.lambda_locals_stack.pop().unwrap();
                
                self.types.lambda_captures.insert(expr as *const Expr, captures);

                Ok(lambda_type)
            },
            Expr::Block { stmts, left_brace } => self.check_block_expr_expr(stmts, left_brace),
            Expr::IndexAccess { target, index, left_bracket } => self.check_index_access_expr(target, index, left_bracket),
            Expr::FieldAccess { target, field } => self.check_field_access_expr(target, field),
            Expr::TimeTravelAbsolute { target, index, at_token } => self.check_time_travel_absolute_expr(target, index, at_token),
            Expr::TimeTravelRelative { target, offset, at_token } => self.check_time_travel_relative_expr(target, offset, at_token),
        }?;

        self.types.expr_types.insert(expr as *const Expr, expr_type.clone());
        
        Ok(expr_type)
    }

    fn check_literal_expr(&mut self, literal: &Token) -> Result<TypeExpr, TypeError> {
        match literal.literal {
            Some(Literal::Number(_)) => {
                if literal.lexeme.contains(".") {
                    Ok(self.create_double_type(literal.line, literal.column))
                } else {
                    Ok(self.create_int_type(literal.line, literal.column))
                }
            }
            Some(Literal::String(_)) => {
                Ok(self.create_string_type(literal.line, literal.column))
            }
            None => Err(TypeError::UnknownLiteral { line: literal.line, column: literal.column, token: literal.clone() }),
        }
    }

    fn check_array_literal_expr(&mut self, elements: &Vec<Expr>, left_bracket: &Token) -> Result<TypeExpr, TypeError> {
        if elements.is_empty() {
            return Err(TypeError::EmptyArrayWithoutTypeAnnotation { 
                line: left_bracket.line, 
                column: left_bracket.column 
            });
        }
        
        let mut element_type = self.check_expr(&elements[0])?;
        for element in elements.iter().skip(1) {
            let current_type = self.check_expr(element)?;
            element_type = self.unifier.unify_at(&element_type, &current_type, left_bracket)?;
        }
        
        Ok(self.create_array_type(left_bracket.line, left_bracket.column, element_type))
    }

    fn check_map_literal_expr(&mut self, elements: &Vec<(Expr, Expr)>, left_brace: &Token) -> Result<TypeExpr, TypeError> {
        if elements.is_empty() {
            return Err(TypeError::EmptyMapWithoutTypeAnnotation { 
                line: left_brace.line, 
                column: left_brace.column 
            });
        }
        
        let (first_key, first_value) = &elements[0];
        let mut key_type = self.check_expr(first_key)?;
        let mut value_type = self.check_expr(first_value)?;
        
        for (key, value) in elements.iter().skip(1) {
            let current_key_type = self.check_expr(key)?;
            let current_value_type = self.check_expr(value)?;
            key_type = self.unifier.unify_at(&key_type, &current_key_type, left_brace)?;
            value_type = self.unifier.unify_at(&value_type, &current_value_type, left_brace)?;
        }
        
        Ok(self.create_map_type(left_brace.line, left_brace.column, key_type, value_type))
    }

    fn check_range_literal_expr(&mut self, start: &Box<Expr>, end: &Box<Expr>, is_inclusive: &bool, range_token: &Token) -> Result<TypeExpr, TypeError> {
        let start_type = self.check_expr(start)?;
        let end_type = self.check_expr(end)?;
        
        let unified_type = self.unifier.unify_at(&start_type, &end_type, range_token)?;

        if *is_inclusive {
            Ok(self.create_inclusive_range_type(range_token.line, range_token.column, unified_type))
        } else {
            Ok(self.create_range_type(range_token.line, range_token.column, unified_type))
        }
    }

    fn check_variable_expr(&mut self, identifier: &Token) -> Result<TypeExpr, TypeError> {
        if let Some(type_expr) = self.environment.lookup(identifier) {
            let type_expression = type_expr.clone();

            if let (Some(captures), Some(locals)) = (self.lambda_captures_stack.last_mut(), self.lambda_locals_stack.last()) {
                if !locals.contains(&identifier.lexeme) {
                    let already_exists_in_captures = captures.iter().any(|(existing_identifier, _)| existing_identifier == &identifier.lexeme);
                    if !already_exists_in_captures {
                        captures.push((identifier.lexeme.clone(), type_expression.clone()));
                    }
                }
            }

            return Ok(type_expression);
        }

        if let Some(enum_template) = self.enum_variants.get(&identifier.lexeme) {
            let mut enum_type = self.unifier.instantiate_type(enum_template);

            
            if let TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields } = enum_type {
                let mut type_arguments = type_arguments;
                if type_arguments.is_empty() {
                    if let Some(parameters) = &type_parameters {
                        type_arguments = parameters.iter().map(|_| self.unifier.new_type_var()).collect();
                    }
                }

                enum_type = TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields }
            }
            

            if let TypeExpr::Named { enum_variants: Some(variants), .. } = &enum_type {
                if let Some(variant) = variants.iter().find(|v| v.identifier.lexeme == identifier.lexeme) {
                    if variant.payload_types.is_empty() {
                        return Ok(enum_type);
                    } else {
                        return Ok(TypeExpr::Function {
                            parameter_types: variant.payload_types.clone(),
                            return_type: Box::new(enum_type),
                        });
                    }
                }
            }
        }

        Err(TypeError::UndefinedVariable {
            line: identifier.line,
            column: identifier.column,
            identifier: identifier.lexeme.clone(),
        })
    }

    fn check_struct_instantiation_expr(&mut self, identifier: &Token, fields: &Vec<(Token, Expr)>) -> Result<TypeExpr, TypeError> {
        let struct_template = self
            .environment
            .lookup(identifier)
            .cloned()
            .ok_or(TypeError::UndefinedVariable {
                line: identifier.line,
                column: identifier.column,
                identifier: identifier.lexeme.clone(),
        })?;

        let mut struct_type = self.unifier.instantiate_type(&struct_template);

        if let TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields } = struct_type
        {
            let mut type_arguments = type_arguments;
            if type_arguments.is_empty() {
                if let Some(parameters) = &type_parameters {
                    type_arguments = parameters.iter().map(|_| self.unifier.new_type_var()).collect(); 
                }
            }

            struct_type = TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields };
        }

        let declared_fields = match &struct_type {
            TypeExpr::Named { struct_fields: Some(fields), .. } => fields,
            _ => {
                return Err(TypeError::InvalidFieldAccessTarget {
                    line: identifier.line,
                    column: identifier.column,
                    target_type: struct_type,
                });
            }
        };

        for (field_identifier, field_expr) in fields {
            let value_type = self.check_expr(field_expr)?;

            if let Some((_, declared_type)) = declared_fields.iter().find(|(decl_ident, _)| decl_ident.lexeme == field_identifier.lexeme) {
                self.unifier.unify_at(declared_type, &value_type, field_identifier)?;
            } else {
                return Err(TypeError::UnknownField {
                    line: field_identifier.line,
                    column: field_identifier.column,
                    target_type: struct_type.clone(),
                    field_name: field_identifier.lexeme.clone(),
                });
            }
        }

        for (declared_identifier, _) in declared_fields {
            let provided = fields.iter().any(|(field_identifier, _)| field_identifier.lexeme == declared_identifier.lexeme);

            if !provided {
                return Err(TypeError::UnknownField {
                    line: identifier.line,
                    column: identifier.column,
                    target_type: struct_type.clone(),
                    field_name: declared_identifier.lexeme.clone(),
                });
            }
        }

        Ok(struct_type)
    }

    fn check_assign_expr(&mut self, identifier: &Token, value: &Box<Expr>) -> Result<TypeExpr, TypeError> {
        let value_type = self.check_expr(value)?;
        let var_type = self.check_variable_expr(identifier)?;
        
        self.unifier.unify_at(&var_type, &value_type, identifier)
    }

    fn check_assign_index_expr(&mut self, target: &Box<Expr>, index: &Box<Expr>, value: &Box<Expr>, left_bracket: &Token) -> Result<TypeExpr, TypeError> {
        let target_type = self.check_expr(target)?;
        let index_type = self.check_expr(index)?;
        let value_type = self.check_expr(value)?;

        match &target_type {
            TypeExpr::Named { identifier, type_arguments, .. } if identifier.lexeme == "Array" => {
                if type_arguments.len() != 1 {
                    return Err(TypeError::InvalidIndexTarget {
                        line: left_bracket.line,
                        column: left_bracket.column,
                        target_type,
                    });
                }

                match index_type {
                    TypeExpr::Named { identifier: index_identifier, .. } if index_identifier.lexeme == "Int" => {}
                    _ => return Err(TypeError::InvalidIndexType { 
                        line: left_bracket.line, 
                        column: left_bracket.column, 
                        target_type,
                        index_type,
                    }),
                }

                let element_type = type_arguments[0].clone();
                self.unifier.unify_at(&value_type, &element_type, left_bracket)?;
                Ok(element_type)
            }

            TypeExpr::Named { identifier, type_arguments, .. } if identifier.lexeme == "Map" => {
                if type_arguments.len() != 2 {
                    return Err(TypeError::InvalidIndexTarget { 
                        line: left_bracket.line, 
                        column: left_bracket.column, 
                        target_type,
                    });
                }

                let key_type = type_arguments[0].clone();
                let expected_value_type = type_arguments[1].clone();

                self.unifier.unify_at(&index_type, &key_type, left_bracket)?;
                self.unifier.unify_at(&value_type, &expected_value_type, left_bracket)?;
                Ok(expected_value_type)
            }

            _ => Err(TypeError::InvalidIndexTarget { 
                line: left_bracket.line, 
                column: left_bracket.column, 
                target_type 
            }),
        }
    }

    fn check_assign_time_travel_absolute_expr(&mut self, target: &Box<Expr>, index: &Box<Expr>, value: &Box<Expr>, at_token: &Token) -> Result<TypeExpr, TypeError> {
        let target_type = self.check_expr(target)?;
        let index_type = self.check_expr(index)?;
        let value_type = self.check_expr(value)?;

        match index_type {
            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Int" => {}
            _ => return Err(TypeError::InvalidIndexType { 
                line: at_token.line, 
                column: at_token.column, 
                target_type, 
                index_type 
            }),
        }

        self.unifier.unify_at(&target_type, &value_type, at_token)
    }

    fn check_assign_time_travel_relative_expr(&mut self, target: &Box<Expr>, offset: &Box<Expr>, value: &Box<Expr>, at_token: &Token) -> Result<TypeExpr, TypeError> {
        let target_type = self.check_expr(target)?;
        let offset_type = self.check_expr(offset)?;
        let value_type  = self.check_expr(value)?;

        match offset_type {
            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Int" => {}
            _ => return Err(TypeError::InvalidIndexType { 
                line: at_token.line, 
                column: at_token.column, 
                target_type, 
                index_type: offset_type 
            }),
        }

        self.unifier.unify_at(&target_type, &value_type, at_token)
    }

    fn check_unary_expr(&mut self, operator: &Token, right: &Box<Expr>) -> Result<TypeExpr, TypeError> {
        let right_type = self.check_expr(right)?;

        match operator.token_type {
            TokenType::Minus => {
                match &right_type {
                    TypeExpr::Named { identifier, .. } if identifier.lexeme == "Int" || identifier.lexeme == "Double" => {
                        Ok(right_type)
                    }
                    _ => Err(TypeError::InvalidUnaryOperand {
                        line: operator.line,
                        column: operator.column,
                        operator: operator.lexeme.clone(),
                        operand_type: right_type
                    })
                }
            }
            TokenType::Exclamation => {
                match &right_type {
                    TypeExpr::Named { identifier, .. } if identifier.lexeme == "Bool" => {
                        Ok(right_type)
                    }
                    _ => Err(TypeError::InvalidUnaryOperand {
                        line: operator.line,
                        column: operator.column,
                        operator: operator.lexeme.clone(),
                        operand_type: right_type
                    })
                }
            }
            _ => Err(TypeError::InvalidUnaryOperand {
                line: operator.line,
                column: operator.column,
                operator: operator.lexeme.clone(),
                operand_type: right_type
            })
        }
    }

    fn check_binary_expr(&mut self, left: &Box<Expr>, operator: &Token, right: &Box<Expr>) -> Result<TypeExpr, TypeError> {
        let left_type = self.check_expr(left)?;
        let right_type = self.check_expr(right)?;

        match operator.token_type {
            TokenType::Plus | TokenType::Minus | TokenType::Star | TokenType::Slash | TokenType::Percent => {
                if operator.token_type == TokenType::Plus {
                    if let (
                        TypeExpr::Named { identifier: left_identifier, .. },
                        TypeExpr::Named { identifier: right_identifier, .. },
                    ) = (&left_type, &right_type) {
                        if left_identifier.lexeme == "String" && right_identifier.lexeme == "String" {
                            return Ok(left_type)
                        }
                    }
                }

                let unified_left_type = self.unifier.apply_substitutions(&left_type);
                let unified_right_type = self.unifier.apply_substitutions(&right_type);

                match (&unified_left_type, &unified_right_type) {
                    (TypeExpr::Named { identifier: left_identifier, .. }, TypeExpr::Named { identifier: right_identifier, .. }) => {
                        let left = left_identifier.lexeme.as_str();
                        let right = right_identifier.lexeme.as_str();

                        if (left == "Int" || left == "Double") && (right == "Int" || right == "Double") {
                            if left == right {
                                return Ok(left_type);
                            } else {
                                return Ok(self.create_double_type(operator.line, operator.column));
                            }
                        }
                    }
                    _ => {
                        let unified = self.unifier.unify_at(&left_type, &right_type, operator)?;

                        if let TypeExpr::Named { identifier, .. } = &unified {
                            let type_identifier = identifier.lexeme.as_str();
                            if type_identifier == "Int" || type_identifier == "Double" {
                                return Ok(unified);
                            }
                        }
                    }
                }

                Err(TypeError::InvalidBinaryOperands {
                    line: operator.line,
                    column: operator.column,
                    operator: operator.lexeme.clone(),
                    left_type,
                    right_type,
                })
            }
            TokenType::EqualEqual | TokenType::ExclamationEqual | TokenType::Less | TokenType::LessEqual | TokenType::Greater | TokenType::GreaterEqual => {
                self.unifier.unify_at(&left_type, &right_type, operator)?;
                
                Ok(self.create_bool_type(operator.line, operator.column))
            }
            _ => Err(TypeError::InvalidBinaryOperands {
                line: operator.line,
                column: operator.column,
                operator: operator.lexeme.clone(),
                left_type,
                right_type
            })
        }
    }

    fn check_logical_expr(&mut self, left: &Box<Expr>, operator: &Token, right: &Box<Expr>) -> Result<TypeExpr, TypeError> {
        let left_type = self.check_expr(left)?;
        let right_type = self.check_expr(right)?;

        match (&left_type, &right_type) {
            (TypeExpr::Named { identifier: left_identifier, .. }, TypeExpr::Named { identifier: right_identifier, .. })
                if left_identifier.lexeme == "Bool" && right_identifier.lexeme == "Bool" => 
            {    
                Ok(self.create_bool_type(operator.line, operator.column))
            }
            _ => Err(TypeError::InvalidBinaryOperands {
                line: operator.line,
                column: operator.column,
                operator: operator.lexeme.clone(),
                left_type,
                right_type
            }),
        }
    }

    fn check_call_expr(&mut self, callee: &Box<Expr>, paren: &Token, arguments: &Vec<Expr>) -> Result<TypeExpr, TypeError> {
        let callee_type = self.check_expr(callee)?;

        match callee_type {
            TypeExpr::Function { parameter_types, return_type } => {
                if arguments.len() != parameter_types.len() {
                    return Err(TypeError::ArgumentCountMismatch {
                        line: paren.line,
                        column: paren.column,
                        expected: parameter_types.len(),
                        found: arguments.len()
                    });
                }
                
                for (argument, parameter_type) in arguments.iter().zip(parameter_types.iter()) {
                    let argument_type = self.check_expr(argument)?;
                    self.unifier.unify_at(&argument_type, parameter_type, paren)?;
                }
                
                Ok(*return_type)
            }
            _ => Err(TypeError::InvalidCallTarget {
                line: paren.line,
                column: paren.column,
                target_type: callee_type
            })
        }
    }

    fn check_if_expr(&mut self, if_token: &Token, condition: &Box<Expr>, then_branch: &Box<Expr>, else_branch: &Option<Box<Expr>>) -> Result<TypeExpr, TypeError> {
        let condition_type = self.check_expr(condition)?;

        match condition_type {
            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Bool" => {}
            _ => return Err(TypeError::ConditionNotBoolean {
                line: if_token.line,
                column: if_token.column,
                condition_type
            }),
        }
        
        let then_branch_type = self.check_expr(then_branch)?;
        
        match else_branch {
            Some(else_expr) => {
                let else_branch_type = self.check_expr(else_expr)?;
                self.unifier.unify_at(&then_branch_type, &else_branch_type, if_token)
            }
            None => Err(TypeError::MissingElseBranch {
                line: if_token.line,
                column: if_token.column
            })
        }
    }

    fn check_match_expr(&mut self, match_token: &Token, subject: &Box<Expr>, cases: &Vec<MatchCase>) -> Result<TypeExpr, TypeError> {
        let subject_type = self.check_expr(subject)?;

        if cases.is_empty() {
            return Err(TypeError::EmptyMatch {
                line: match_token.line,
                column: match_token.column
            });
        }

        let mut result_type = None;
        for case in cases {
            self.check_pattern(&case.pattern, &subject_type)?;

            if let Some(guard) = &case.guard {
                let guard_type = self.check_expr(guard)?;
                match guard_type {
                    TypeExpr::Named { identifier, .. } if identifier.lexeme == "Bool" => {}
                    _ => {
                        let (line, column) = if let Some(token) = &case.guard_token {
                            (token.line, token.column)
                        } else {
                            (match_token.line, match_token.column)
                        };
                        
                        return Err(TypeError::ConditionNotBoolean {
                            line,
                            column,
                            condition_type: guard_type
                        })
                    }
                }
            }

            let case_type = self.check_expr(&case.body)?;

            match result_type {
                None => result_type = Some(case_type),
                Some(ref mut prev_type) => {
                    *prev_type = self.unifier.unify_at(prev_type, &case_type, match_token)?;
                }
            }
        }
        
        Ok(result_type.unwrap())
    }

    fn check_pattern(&mut self, pattern: &Pattern, subject_type: &TypeExpr) -> Result<(), TypeError> {
        match pattern {
            Pattern::Wildcard(_) => {}
            Pattern::Literal(literal) => {
                let literal_type = self.check_literal_expr(literal)?;
                self.unifier.unify_at(&literal_type, subject_type, literal)?;
            }
            Pattern::Variable(identifier) => {
                self.environment.insert(identifier, subject_type.clone());
            }
            Pattern::EnumVariant { identifier: variant_identifier, arguments } => {
                match subject_type {
                    TypeExpr::Named { identifier: subject_type_identifier, type_arguments: subject_type_type_arguments, .. } => {
                        if let Some(enum_definition) = self.environment.lookup(subject_type_identifier) {
                            match enum_definition {
                                TypeExpr::Named { type_parameters: definition_type_parameters, enum_variants: Some(variants), .. } => {
                                    let raw_variant_payload_types = variants
                                        .iter()
                                        .find(|v| v.identifier.lexeme == variant_identifier.lexeme)
                                        .map(|v| v.payload_types.clone())
                                        .ok_or(TypeError::InvalidEnumVariant {
                                            line: variant_identifier.line,
                                            column: variant_identifier.column,
                                            enum_type: subject_type.clone(),
                                            variant_identifier: variant_identifier.lexeme.clone(),
                                    })?;
                                    
                                    let mut substitutions: HashMap<usize, TypeExpr> = HashMap::new();
                                    
                                    if let Some(definition_parameters) = definition_type_parameters {
                                        for (definition_parameter, subject_argument) in definition_parameters.iter().zip(subject_type_type_arguments.iter()) {
                                            if let TypeExpr::TypeVar { id } = definition_parameter {
                                                substitutions.insert(*id, subject_argument.clone());
                                            }
                                        }
                                    }

                                    let variant_payload_types: Vec<TypeExpr> = raw_variant_payload_types.iter().map(|p| self.substitute_typevars(p, &substitutions)).collect();
                                    
                                    if arguments.len() != variant_payload_types.len() {
                                        return Err(TypeError::EnumVariantPayloadCountMismatch {
                                            line: variant_identifier.line,
                                            column: variant_identifier.column,
                                            variant_identifier: variant_identifier.lexeme.clone(),
                                            expected: variant_payload_types.len(),
                                            found: arguments.len(),
                                        });
                                    }
                                    
                                    for (argument_pattern, payload_type) in arguments.iter().zip(variant_payload_types.iter()) {
                                        self.check_pattern(argument_pattern, payload_type)?;
                                    }
                                }
                                _ => return Err(TypeError::InvalidEnumPatternTarget {
                                    line: variant_identifier.line,
                                    column: variant_identifier.column,
                                    subject_type: subject_type.clone(),
                                })
                            }
                        } else {
                            return Err(TypeError::InvalidEnumPatternTarget {
                                line: variant_identifier.line,
                                column: variant_identifier.column,
                                subject_type: subject_type.clone(),
                            });
                        }
                    }
                    _ => return Err(TypeError::InvalidEnumPatternTarget {
                        line: variant_identifier.line,
                        column: variant_identifier.column,
                        subject_type: subject_type.clone(),
                    })
                }
            }
            Pattern::Range { start, end, .. } => {
                let start_type = self.check_literal_expr(start)?;
                let end_type = self.check_literal_expr(end)?;
                let unified_bound_type = self.unifier.unify_at(&start_type, &end_type, start)?;
                self.unifier.unify_at(&unified_bound_type, subject_type, start)?;
            }
        }

        Ok(())
    }

    fn substitute_typevars(&self, type_expr: &TypeExpr, substitutions: &HashMap<usize, TypeExpr>) -> TypeExpr {
        match type_expr {
            TypeExpr::TypeVar { id } => {
                substitutions.get(id).cloned().unwrap_or(type_expr.clone())
            }
            TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields } => {
                TypeExpr::Named {
                    identifier: identifier.clone(),
                    type_parameters: type_parameters.clone(),
                    type_arguments: type_arguments.iter().map(|t| self.substitute_typevars(t, substitutions)).collect(),
                    is_optional: *is_optional,
                    enum_variants: enum_variants.clone(),
                    struct_fields: struct_fields.clone(),
                }
            }
            TypeExpr::Function { parameter_types, return_type } => {
                TypeExpr::Function {
                    parameter_types: parameter_types.iter().map(|t| self.substitute_typevars(t, substitutions)).collect(),
                    return_type: Box::new(self.substitute_typevars(return_type, substitutions)),
                }
            }
        }
    }

    fn check_lambda_expr(&mut self, parameters: &Vec<(Token, TypeExpr)>, return_type: &Option<TypeExpr>, body: &Vec<Stmt>) -> Result<TypeExpr, TypeError> {
        self.environment.push_scope();

        let error_line = parameters.first().map(|(token, _)| token.line).unwrap_or(0);
        let error_column = parameters.first().map(|(token, _)| token.column).unwrap_or(0);

        let error_token = parameters
            .first()
            .map(|(t, _)| t.clone())
            .unwrap_or(Token {
                token_type: TokenType::Identifier,
                lexeme: "<lambda>".to_string(),
                literal: None,
                line: error_line,
                column: error_column,
        });

        let result = (|| {
            for (parameter_identifier, parameter_type) in parameters {
                self.environment.insert(parameter_identifier, parameter_type.clone());
            }

            let actual_return_type = self.infer_block_return_type(body, error_line, error_column)?;

            let unified_return_type = match return_type {
                Some(annotated_return_type) => self.unifier.unify_at(&actual_return_type, &annotated_return_type, &error_token)?,
                None => actual_return_type
            };

            Ok(TypeExpr::Function {
                parameter_types: parameters.iter().map(|(_, t)| t.clone()).collect(),
                return_type: Box::new(unified_return_type)
            })
        })();

        self.environment.pop_scope();
        result
    }

    fn check_block_expr_expr(&mut self, statements: &Vec<Stmt>, left_brace: &Token) -> Result<TypeExpr, TypeError> {
        self.environment.push_scope();

        let result = (|| {
            if let Some((last_index, _)) = statements.iter().enumerate().last() {
                for (index, stmt) in statements.iter().enumerate() {
                    if index == last_index {
                        if let Stmt::Expression(expr) = stmt {
                            return self.check_expr(expr);
                        } else {
                            return Ok(self.create_void_type(left_brace.line, left_brace.column));
                        }
                    } else {
                        self.check_stmt(stmt)?;
                    }
                }
            }

            Ok(self.create_void_type(left_brace.line, left_brace.column))
        })();

        self.environment.pop_scope();
        result
    }

    fn check_index_access_expr(&mut self, target: &Box<Expr>, index: &Box<Expr>, left_bracket: &Token) -> Result<TypeExpr, TypeError> {
        let target_type = self.check_expr(target)?;
        let index_type = self.check_expr(index)?;

        match &target_type {
            TypeExpr::Named { identifier, type_arguments, .. } if identifier.lexeme == "Array" => {
                if type_arguments.len() != 1 {
                    return Err(TypeError::InvalidIndexTarget {
                        line: left_bracket.line,
                        column: left_bracket.column,
                        target_type: target_type.clone(),
                    });
                }

                match index_type {
                    TypeExpr::Named { identifier: index_ident, .. } if index_ident.lexeme == "Int" => {
                        Ok(type_arguments[0].clone())
                    }
                    _ => Err(TypeError::InvalidIndexType {
                        line: left_bracket.line,
                        column: left_bracket.column,
                        target_type: target_type,
                        index_type: index_type,
                    }),
                }
            }
            TypeExpr::Named { identifier, type_arguments, .. } if identifier.lexeme == "Map" => {
                if type_arguments.len() != 2 {
                    return Err(TypeError::InvalidIndexTarget {
                        line: left_bracket.line,
                        column: left_bracket.column,
                        target_type: target_type.clone(),
                    });
                }

                let key_type = type_arguments[0].clone();
                let value_type = type_arguments[1].clone();
                self.unifier.unify_at(&index_type, &key_type, left_bracket)?;

                Ok(value_type)
            }
            _ => Err(TypeError::InvalidIndexTarget {
                line: left_bracket.line,
                column: left_bracket.column,
                target_type: target_type,
            }),
        }
    }

    fn check_field_access_expr(&mut self, target: &Box<Expr>, field: &Token) -> Result<TypeExpr, TypeError> {
        let target_type = self.check_expr(target)?;

        match &target_type {
            TypeExpr::Named { struct_fields: Some(fields), .. } => {
                if let Some((_, field_type)) = fields.iter().find(|(identifier, _)| identifier.lexeme == field.lexeme) {
                    Ok(field_type.clone())
                } else {
                    Err(TypeError::UnknownField {
                        line: field.line,
                        column: field.column,
                        target_type: target_type,
                        field_name: field.lexeme.clone(),
                    })
                }
            }
            _ => Err(TypeError::InvalidFieldAccessTarget {
                line: field.line,
                column: field.column,
                target_type: target_type,
            }),
        }
    }

    fn check_time_travel_absolute_expr(&mut self, target: &Box<Expr>, index: &Box<Expr>, at_token: &Token) -> Result<TypeExpr, TypeError> {
        let target_type = self.check_expr(target)?;
        let index_type  = self.check_expr(index)?;

        match index_type {
            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Int" => {}
            _ => return Err(TypeError::InvalidIndexType {
                line: at_token.line,
                column: at_token.column,
                target_type: target_type,
                index_type: index_type,
            }),
        }

        Ok(target_type)
    }

    fn check_time_travel_relative_expr(&mut self, target: &Box<Expr>, offset: &Box<Expr>, at_token: &Token) -> Result<TypeExpr, TypeError> {
        let target_type = self.check_expr(target)?;
        let offset_type = self.check_expr(offset)?;

        match offset_type {
            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Int" => {}
            _ => return Err(TypeError::InvalidIndexType {
                line: at_token.line,
                column: at_token.column,
                target_type: target_type,
                index_type: offset_type,
            }),
        }

        Ok(target_type)
    }

    fn infer_block_return_type(&mut self, body: &Vec<Stmt>, void_line: u32, void_column: u32) -> Result<TypeExpr, TypeError> {
        for stmt in body {
            self.check_stmt(stmt)?;
        }

        let return_type = if let Some(last_stmt) = body.last() {
            match last_stmt {
                Stmt::Expression(expr) => self.check_expr(expr)?,
                _ => self.create_void_type(void_line, void_column),
            }
        } else {
            self.create_void_type(void_line, void_column)
        };

        Ok(return_type)
    }
    
    fn substitute_type_parameters(&self, type_expr: &TypeExpr, type_parameters: &Vec<Token>, type_arguments: &Vec<TypeExpr>) -> TypeExpr {
        match type_expr {
            TypeExpr::Named { identifier, type_parameters: inner_type_parameters, type_arguments: inner_type_arguments, is_optional, enum_variants, struct_fields } => {
                if let Some(position) = type_parameters.iter().position(|p| p.lexeme == identifier.lexeme) {
                    return type_arguments[position].clone();
                }

                TypeExpr::Named {
                    identifier: identifier.clone(),
                    type_parameters: inner_type_parameters.clone(),
                    type_arguments: inner_type_arguments
                        .iter()
                        .map(|t| self.substitute_type_parameters(t, type_parameters, type_arguments))
                        .collect(),
                    is_optional: *is_optional,
                    enum_variants: enum_variants.clone(),
                    struct_fields: struct_fields.clone(),
                }
            }
            TypeExpr::Function { parameter_types, return_type } => {
                TypeExpr::Function {
                    parameter_types: parameter_types
                        .iter()
                        .map(|t| self.substitute_type_parameters(t, type_parameters, type_arguments))
                        .collect(),
                    return_type: Box::new(self.substitute_type_parameters(return_type, type_parameters, type_arguments)),
                }
            }
            TypeExpr::TypeVar { .. } => type_expr.clone(),
        }
    }

    fn register_enum_variants(&mut self, enum_type: &TypeExpr) -> Result<(), TypeError> {
        if let TypeExpr::Named { enum_variants: Some(variants), .. } = enum_type {
            for v in variants {
                let identifier = v.identifier.lexeme.clone();

                if self.enum_variants.contains_key(&identifier) {
                    return Err(TypeError::DuplicateEnumVariant {
                        line: v.identifier.line,
                        column: v.identifier.column,
                        variant_identifier: identifier,
                    });
                }

                self.enum_variants.insert(identifier, enum_type.clone());
            }
        }

        Ok(())
    }

    fn inject_builtin_enums(&mut self) -> Result<(), TypeError> {
        self.inject_builtin_enum("Bool", Vec::new(), vec![
            EnumVariant { identifier: Self::create_builtin_identifier("True"),  payload_types: vec![] },
            EnumVariant { identifier: Self::create_builtin_identifier("False"),  payload_types: vec![] },
        ])?;

        let t_type = self.unifier.new_type_var();
        self.inject_builtin_enum("Optional", vec![t_type.clone()], vec![
            EnumVariant {
                identifier: Self::create_builtin_identifier("Some"),
                payload_types: vec![t_type.clone()],
            },
            EnumVariant {
                identifier: Self::create_builtin_identifier("None"),
                payload_types: vec![],
            },
        ])?;

        Ok(())
    } 

    fn inject_builtin_functions(&mut self) -> Result<(), TypeError> {
        let parameter_type = self.unifier.new_type_var();
        let return_type = self.create_void_type(0, 0);

        let print_type = TypeExpr::Function {
            parameter_types: vec![parameter_type],
            return_type: Box::new(return_type)
        };

        let print_token = Token {
            token_type: TokenType::Identifier,
            lexeme: "print".to_string(),
            literal: None,
            line: 0,
            column: 0
        };

        self.environment.insert(&print_token, print_type.clone());
        self.types.function_types.insert("print".to_string(), print_type);

        Ok(())
    }

    fn inject_builtin_enum(&mut self, enum_identifier: &str, type_parameters: Vec<TypeExpr>, variants: Vec<EnumVariant>) -> Result<(), TypeError> {
        let identifier = Self::create_builtin_identifier(enum_identifier);

        let enum_type = TypeExpr::Named {
            identifier: identifier.clone(),
            type_parameters: Some(type_parameters),
            type_arguments: Vec::new(),
            is_optional: false,
            enum_variants: Some(variants),
            struct_fields: None,
        };

        self.register_enum_variants(&enum_type)?;
        self.environment.insert(&identifier, enum_type);

        Ok(())
    }

    fn create_builtin_identifier(identifier: &str) -> Token {
        Token {
            token_type: TokenType::Identifier,
            lexeme: identifier.to_string(),
            literal: None,
            line: 0,
            column: 0,
        }
    }

    fn create_type(&self, lexeme: String, line: u32, column: u32, type_arguments: Vec<TypeExpr>) -> TypeExpr {
        TypeExpr::Named {
            identifier: Token { 
                token_type: TokenType::Identifier, 
                lexeme, 
                literal: None, 
                line, 
                column 
            },
            type_parameters: None,
            type_arguments,
            is_optional: false,
            enum_variants: None,
            struct_fields: None,
        }
    }

    fn create_int_type(&self, line: u32, column: u32) -> TypeExpr {
        self.create_type("Int".to_string(), line, column, Vec::new())
    }

    fn create_double_type(&self, line: u32, column: u32) -> TypeExpr {
        self.create_type("Double".to_string(), line, column, Vec::new())
    }

    fn create_string_type(&self, line: u32, column: u32) -> TypeExpr {
        self.create_type("String".to_string(), line, column, Vec::new())
    }

    fn create_bool_type(&self, line: u32, column: u32) -> TypeExpr {
        self.create_type("Bool".to_string(), line, column, Vec::new())
    }

    fn create_void_type(&self, line: u32, column: u32) -> TypeExpr {
        self.create_type("Void".to_string(), line, column, Vec::new())
    }

    fn create_array_type(&self, line: u32, column: u32, element_type: TypeExpr) -> TypeExpr {
        self.create_type("Array".to_string(), line, column, vec![element_type])
    }

    fn create_map_type(&self, line: u32, column: u32, key_type: TypeExpr, value_type: TypeExpr) -> TypeExpr {
        self.create_type("Map".to_string(), line, column, vec![key_type, value_type])
    }

    fn create_range_type(&self, line: u32, column: u32, bounds_type: TypeExpr) -> TypeExpr {
        self.create_type("Range".to_string(), line, column, vec![bounds_type])
    }

    fn create_inclusive_range_type(&self, line: u32, column: u32, bounds_type: TypeExpr) -> TypeExpr {
        self.create_type("InclusiveRange".to_string(), line, column, vec![bounds_type])
    }
}