use std::collections::{BTreeSet, HashMap};
use std::mem;

use crate::ast::token::{Token};
use crate::ast::expr::{Expr, MatchCase, Pattern};
use crate::ast::stmt::{EnumVariant, FunctionStmt, Stmt};
use crate::error::CodegenError;
use crate::type_system::types::TypeExpr;
use crate::type_system::type_checker::TypeInfo;

struct ExprCode {
    pre_expr_stmts: Vec<String>,
    expr: String,
}

#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct UserTypeInstantiation {
    identifier: String,
    argument_c_types: Vec<String>,
}

impl UserTypeInstantiation {
    fn c_identifier(&self) -> String {
        let mut parts = vec![self.identifier.clone()];
        for c_argument in &self.argument_c_types {
            parts.push(Self::suffix(&c_argument));
        }
        parts.join("_")
    }

    fn suffix(c_type: &str) -> String {
        c_type.strip_prefix("struct ")
            .unwrap_or(c_type)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct FunctionInstantiation {
    base_identifier: String,
    argument_c_types: Vec<String>,
}

impl FunctionInstantiation {
    fn c_identifier(&self) -> String {
        let mut parts = vec![self.base_identifier.clone()];
        for t in &self.argument_c_types {
            parts.push(UserTypeInstantiation::suffix(t));
        }
        parts.join("_")
    }
}

pub struct CodeGenerator<'a> {
    types: &'a TypeInfo,
    output: String,
    indent_level: usize,
    temp_id_counter: usize,
    array_instantiations: BTreeSet<String>,
    range_instantiations: BTreeSet<String>,
    user_type_instantiations: BTreeSet<UserTypeInstantiation>,
    function_instantiations: BTreeSet<FunctionInstantiation>,
    ast_structs: HashMap<String, Stmt>,
    ast_enums: HashMap<String, Stmt>,
    ast_functions: HashMap<String, FunctionStmt>,
}

impl<'a> CodeGenerator<'a> {
    pub fn new(types: &'a TypeInfo) -> Self {
        Self {
            types,
            output: String::new(),
            indent_level: 0,
            temp_id_counter: 0,
            array_instantiations: BTreeSet::new(),
            range_instantiations: BTreeSet::new(),
            user_type_instantiations: BTreeSet::new(),
            function_instantiations: BTreeSet::new(),
            ast_structs: HashMap::new(),
            ast_enums: HashMap::new(),
            ast_functions: HashMap::new(),
        }
    }

    pub fn generate_program(&mut self, stmts: &[Stmt]) -> Result<String, CodegenError> {
        self.write_line("#include <stdio.h>");
        self.write_line("#include <stdlib.h>");
        self.write_line("#include <stdbool.h>");
        self.write_line("#include <string.h>");
        self.write_line("#include <stddef.h>");
        self.write_line("");

        for stmt in stmts {
            self.generate_declarations(stmt);
        }

        self.write_line("typedef struct Fn {");
        self.indent();
        self.write_line("void* env;");
        self.write_line("void* fn;");
        self.un_indent();
        self.write_line("} Fn;");
        self.write_line("");

        self.write_line("typedef struct { void* impl; } Map;");
        self.write_line("");

        self.write_helper_functions();

        let mut main_output = String::new();
        mem::swap(&mut self.output, &mut main_output);
        self.generate_main(stmts)?;
        mem::swap(&mut self.output, &mut main_output);

        self.write_array_types();
        self.write_range_types();
        self.write_user_type_instantiations();
        self.write_function_instantiations()?;

        self.output.push_str(&main_output);

        Ok(self.output.clone())
    }

    fn write_helper_functions(&mut self) {
        self.write_string_concat_helper();
        self.write_print_helpers();
    }

    fn write_string_concat_helper(&mut self) {
        self.write_line("const char* string_concat(const char* a, const char* b) {");
        self.indent();
        self.write_line("size_t len_a = strlen(a);");
        self.write_line("size_t len_b = strlen(b);");
        self.write_line("char* result = (char*)malloc(len_a + len_b + 1);");
        self.write_line("if (!result) { fprintf(stderr, \"Out of memory in string_concat\\n\"); exit(1); }");
        self.write_line("memcpy(result, a, len_a);");
        self.write_line("memcpy(result + len_a, b, len_b);");
        self.write_line("result[len_a + len_b] = '\\0';");
        self.write_line("return result;");
        self.un_indent();
        self.write_line("}");
        self.write_line("");
    }

    fn write_print_helpers(&mut self) {
        self.write_line("void print_string(const char* s) {");
        self.indent();
        self.write_line("printf(\"%s\\n\", s);");
        self.un_indent();
        self.write_line("}");
        self.write_line("");

        self.write_line("void print_int(int x) {");
        self.indent();
        self.write_line("printf(\"%d\\n\", x);");
        self.un_indent();
        self.write_line("}");
        self.write_line("");

        self.write_line("void print_double(double x) {");
        self.indent();
        self.write_line("printf(\"%g\\n\", x);");
        self.un_indent();
        self.write_line("}");
        self.write_line("");

        self.write_line("void print_bool(bool b) {");
        self.indent();
        self.write_line("printf(\"%s\\n\", b ? \"True\" : \"False\");");
        self.un_indent();
        self.write_line("}");
        self.write_line("");
    }

    fn write_array_types(&mut self) {
        let array_instantiations = std::mem::take(&mut self.array_instantiations);
        for array_instantiation in array_instantiations {
            let suffix = UserTypeInstantiation::suffix(&array_instantiation);
            self.write_line(&format!("typedef struct {{ size_t length; size_t capacity; {array_instantiation}* data; }} Array_{suffix};"));
            self.write_line(&format!("Array_{suffix} make_Array_{suffix}(size_t length, {array_instantiation} const* elems) {{"));
            self.indent();
            self.write_line(&format!("Array_{suffix} arr; arr.length = length; arr.capacity = length;"));
            self.write_line(&format!("arr.data = ({array_instantiation}*)malloc(sizeof({array_instantiation}) * length);"));
            self.write_line("for (size_t i = 0; i < length; ++i) { arr.data[i] = elems[i]; }");
            self.write_line("return arr;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");
        }
    }

    fn write_range_types(&mut self) {
        let range_instantiations = std::mem::take(&mut self.range_instantiations);
        for range_instantiation in range_instantiations {
            let suffix = UserTypeInstantiation::suffix(&range_instantiation);
            self.write_line(&format!("typedef struct {{ {range_instantiation} start; {range_instantiation} end; bool inclusive; }} Range_{suffix};"));
            self.write_line(&format!("Range_{suffix} make_Range_{suffix}({range_instantiation} start, {range_instantiation} end, bool inclusive) {{"));
            self.indent();
            self.write_line(&format!("Range_{suffix} r; r.start = start; r.end = end; r.inclusive = inclusive;"));
            self.write_line("return r;");
            self.un_indent();
            self.write_line("}");
            self.write_line("");
        }
    }

    fn write_user_type_instantiations(&mut self) {
        let user_type_instantiations = std::mem::take(&mut self.user_type_instantiations);

        for instantiation in user_type_instantiations {
            let c_identifier = instantiation.c_identifier();
            let identifier = &instantiation.identifier;

            if identifier == "Optional" {
                let inner_c_type = &instantiation.argument_c_types[0];
                let tag_name = format!("{c_identifier}_Tag");

                self.write_line(&format!("typedef enum {tag_name} {{"));
                self.indent();
                self.write_line(&format!("{}_Some,", c_identifier));
                self.write_line(&format!("{}_None,", c_identifier));
                self.un_indent();
                self.write_line(&format!("}} {tag_name};"));
                self.write_line("");

                self.write_line(&format!("typedef struct {c_identifier} {{"));
                self.indent();
                self.write_line(&format!("{tag_name} tag;"));
                self.write_line("union {");
                self.indent();
                self.write_line("struct {");
                self.indent();
                self.write_line(&format!("{inner_c_type} value;"));
                self.un_indent();
                self.write_line("} Some;");
                self.un_indent();
                self.write_line("} data;");
                self.un_indent();
                self.write_line(&format!("}} {c_identifier};"));
                self.write_line("");

                continue;
            }

            if let Some(Stmt::Struct { type_parameters, fields, .. }) = self.ast_structs.get(identifier) {
                let type_parameters = type_parameters.clone();
                let fields = fields.clone();
                self.write_struct_instantiation(&c_identifier, &type_parameters, &fields, &instantiation);
                continue;
            }

            if let Some(Stmt::Enum { type_parameters, variants, .. }) = self.ast_enums.get(identifier) {
                let type_parameters = type_parameters.clone();
                let variants = variants.clone();
                self.write_enum_instantiation(&c_identifier, &type_parameters, &variants, &instantiation);
                continue;
            }
        }
    }

    fn write_struct_instantiation(&mut self, c_identifier: &str, type_parameters: &Vec<Token>, fields: &Vec<(Token, TypeExpr)>, instantiation: &UserTypeInstantiation) {
            let bindings: Vec<(String, String)> = type_parameters
                .iter()
                .zip(instantiation.argument_c_types.iter())
                .map(|(parameter_token, c_type)| (parameter_token.lexeme.clone(), c_type.clone()))
                .collect();

            self.write_line(&format!("typedef struct {c_identifier} {{"));
            self.indent();
            for (field_identifier, field_type) in fields {
                let field_c_type = self.map_type_with_parameters(&field_type, &bindings);
                self.write_line(&format!("{} {};", field_c_type, field_identifier.lexeme));
            }
            self.un_indent();
            self.write_line(&format!("}} {c_identifier};"));
            self.write_line("");
    }

    fn write_enum_instantiation(&mut self, c_identifier: &str, type_parameters: &Vec<Token>, variants: &Vec<EnumVariant>, instantiation: &UserTypeInstantiation) {
        let bindings: Vec<(String, String)> = type_parameters
            .iter()
            .zip(instantiation.argument_c_types.iter())
            .map(|(parameter, c_type)| (parameter.lexeme.clone(), c_type.clone()))
            .collect();

        let tag_name = format!("{c_identifier}_Tag");
        self.write_line(&format!("typedef enum {tag_name} {{"));
        self.indent();
        for variant in variants {
            let tag_const = format!("{}_{}", c_identifier, variant.identifier.lexeme);
            self.write_line(&format!("{tag_const},"));
        }
        self.un_indent();
        self.write_line(&format!("}} {tag_name};"));
        self.write_line("");

        self.write_line(&format!("typedef struct {c_identifier} {{"));
        self.indent();
        self.write_line(&format!("{tag_name} tag;"));
        self.write_line("union {");
        self.indent();

        for variant in variants {
            let variant_name = &variant.identifier.lexeme;
            if variant.payload_types.is_empty() {
                continue;
            }

            self.write_line(&format!("struct {{"));
            self.indent();
            for (index, payload_type) in variant.payload_types.iter().enumerate() {
                let field_c_type = self.map_type_with_parameters(payload_type, &bindings);
                let field_name = if variant.payload_types.len() == 1 {
                    "value".to_string()
                } else {
                    format!("field{index}")
                };
                self.write_line(&format!("{field_c_type} {field_name};"));
            }
            self.un_indent();
            self.write_line(&format!("}} {variant_name};"));
        }

        self.un_indent();
        self.write_line("} data;");
        self.un_indent();
        self.write_line(&format!("}} {c_identifier};"));
        self.write_line("");
    }

    fn write_function_instantiations(&mut self) -> Result<(), CodegenError> {
        let instantiations = mem::take(&mut self.function_instantiations);

        for instantiation in instantiations {
            let c_identifier = instantiation.c_identifier();
            let function = self.ast_functions.get(&instantiation.base_identifier).unwrap().clone();
            let argument_types = &instantiation.argument_c_types;
            self.write_function_instantiation(&c_identifier, &function, argument_types)?;
        }

        Ok(())
    }

    fn write_function_instantiation(&mut self, c_identifier: &str, function: &FunctionStmt, argument_c_types: &Vec<String>) -> Result<(), CodegenError> {
        let bindings: Vec<(String, String)> = function
            .type_parameters
            .iter()
            .zip(argument_c_types.iter())
            .map(|(parameter, argument_c_type)| (parameter.lexeme.clone(), argument_c_type.clone()))
            .collect();

        let return_c_type = if let Some(TypeExpr::Function { return_type, .. }) = self.types.function_types.get(&function.identifier.lexeme) {
            self.map_type_with_parameters(return_type, &bindings)
        } else {
            match &function.return_type {
                Some(return_type) => self.map_type_with_parameters(&return_type, &bindings),
                None => "void".to_string(),
            }
        };

        let mut parameters_code = Vec::new();
        for (parameter_identifier, parameter_type) in &function.parameters {
            let parameter_c_type = self.map_type_with_parameters(parameter_type, &bindings);
            parameters_code.push(format!("{} {}", parameter_c_type, parameter_identifier.lexeme));
        }
        let parameters_code = if parameters_code.is_empty() {
            "void".to_string()
        } else {
            parameters_code.join(", ")
        };

        self.write_line(&format!("{return_c_type} {c_identifier}({parameters_code}) {{"));
        self.indent();

        for (index, stmt) in function.body.iter().enumerate() {
            let is_last = index == function.body.len() - 1;

            if return_c_type != "void" && is_last {
                if let Stmt::Expression(last_expr) = stmt {
                    let last_code = self.generate_expr(last_expr)?;
                    for line in last_code.pre_expr_stmts {
                        self.write_line(&line);
                    }
                    self.write_line(&format!("return {};", last_code.expr));
                    continue;
                }
            }

            let mut lines = Vec::new();
            self.generate_stmt_to(&mut lines, stmt)?;
            for line in lines {
                self.output.push_str(&line);
                self.output.push('\n');
            }
        }

        self.un_indent();
        self.write_line("}");
        self.write_line("");

        Ok(())
    }

    fn generate_main(&mut self, stmts: &[Stmt]) -> Result<(), CodegenError> {
        self.write_line("int main(void) {");
        self.indent();

        for stmt in stmts {
            let mut lines = Vec::new();
            self.generate_stmt_to(&mut lines, stmt)?;
            for line in lines {
                self.output.push_str(&line);
                self.output.push('\n');
            }
        }

        self.write_line("return 0;");
        self.un_indent();
        self.write_line("}");
        self.write_line("");

        Ok(())
    }

    fn generate_stmt_to(&mut self, out: &mut Vec<String>, stmt: &Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Variable { identifier, type_annotation, initializer } => {
                let delo_type = if let Some(annotation) = type_annotation.as_ref() {
                    annotation
                } else if let Some(initializer_expr) = initializer {
                    if let Some(initializer_type) = self.types.expr_types.get(&(initializer_expr as *const Expr)) {
                        initializer_type
                    } else {
                        return Err(CodegenError::MissingType {
                            line: identifier.line,
                            column: identifier.column,
                            identifier: identifier.lexeme.clone(),
                        });
                    }
                } else {
                    return Err(CodegenError::MissingType {
                        line: identifier.line,
                        column: identifier.column,
                        identifier: identifier.lexeme.clone(),
                    });
                };
                let c_type = self.map_type(delo_type);

                let code = if let Some(expr) = initializer {
                    let expr_code = self.generate_expr(expr)?;
                    out.extend(expr_code.pre_expr_stmts);
                    format!(" = {}", expr_code.expr)
                } else {
                    String::new()
                };

                self.write_indented_to(out, self.indent_level, format!("{} {}{};", c_type, identifier.lexeme, code));
            }
            Stmt::Enum { .. } => {}
            Stmt::Struct { .. } => {}
            Stmt::If { condition, then_branch, else_branch, .. } => {
                let condition_code = self.generate_expr(condition)?;
                for line in condition_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("if ({}) {{", condition_code.expr));
                self.indent();
                self.generate_stmt_to(out, then_branch)?;
                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());

                if let Some(else_stmt) = else_branch {
                    self.write_indented_to(out, self.indent_level, "else {".to_string());
                    self.indent();
                    self.generate_stmt_to(out, else_stmt)?;
                    self.un_indent();
                    self.write_indented_to(out, self.indent_level, "}".to_string());
                }
            }
            Stmt::While { condition, body, .. } => {
                let condition_code = self.generate_expr(condition)?;
                for line in condition_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("while ({}) {{", condition_code.expr));
                self.indent();
                self.generate_stmt_to(out, body)?;
                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Stmt::ForRange { identifier, range, body } => {
                let range_code = self.generate_expr(range)?;
                for line in range_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                let range_expr = range_code.expr;

                let range_type = &self.types.expr_types[&(range as *const Expr)];
                let element_type = match range_type {
                    TypeExpr::Named { identifier, type_arguments, .. }
                        if (identifier.lexeme == "Range" || identifier.lexeme == "InclusiveRange") && type_arguments.len() == 1 =>
                    {
                        &type_arguments[0]
                    }
                    _ => {
                        return Err(CodegenError::InvalidRangeType {
                            line: identifier.line,
                            column: identifier.column,
                            range_type: range_type.clone(),
                        });
                    }
                };

                let range_c_type = self.map_type(range_type);
                let element_c_type = self.map_type(element_type);
                let range_temp = self.new_temp_id("range_");
                let loop_variable = &identifier.lexeme;

                self.write_indented_to(out, self.indent_level, format!("{range_c_type} {range_temp} = {range_expr};"));

                self.write_indented_to(
                    out, 
                    self.indent_level, 
                    format!(
                        "for ({t} {i} = {r}.start; ({r}.inclusive ? {i} <= {r}.end : {i} < {r}.end); ++{i}) {{",
                        t = element_c_type,
                        i = loop_variable,
                        r = range_temp, 
                    )
                );

                self.indent();
                self.generate_stmt_to(out, body)?;
                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Stmt::Function(_) => {}
            Stmt::Block(stmts) => {
                for stmt in stmts {
                    self.generate_stmt_to(out,stmt)?;
                }
            }
            Stmt::Expression(expr) => {
                let code = self.generate_expr(expr)?;
                out.extend(code.pre_expr_stmts);
                self.write_indented_to(out, self.indent_level, format!("{};", code.expr));
            }
            Stmt::Break(_) => {
                self.write_indented_to(out, self.indent_level, "break;".to_string());
            }
            Stmt::Continue(_) => {
                self.write_indented_to(out, self.indent_level, "continue;".to_string());
            }
        }

        Ok(())
    }

    fn generate_expr(&mut self, expr: &Expr) -> Result<ExprCode, CodegenError> {
        match expr {
            Expr::Literal(token) => {
                Ok(ExprCode {
                    pre_expr_stmts: Vec::new(),
                    expr: token.lexeme.clone()
                })
            }
            Expr::ArrayLiteral { elements, left_bracket } => {
                let mut pre_expr_stmts = Vec::new();
                let mut element_exprs = Vec::new();

                for element in elements {
                    let expr_code = self.generate_expr(element)?;
                    pre_expr_stmts.extend(expr_code.pre_expr_stmts);
                    element_exprs.push(expr_code.expr);
                }

                let length = elements.len();
                let code = element_exprs.join(", ");

                let array_type = &self.types.expr_types[&(expr as *const Expr)];

                let element_type = match array_type {
                    TypeExpr::Named { identifier, type_arguments, .. } 
                        if identifier.lexeme == "Array" && type_arguments.len() == 1 =>
                    {
                        &type_arguments[0]
                    }
                    _ => {
                        return Err(CodegenError::InvalidArrayLiteralType {
                            line: left_bracket.line,
                            column: left_bracket.column,
                            array_type: array_type.clone() 
                        });
                    }
                };

                let element_c_type = self.map_type(element_type);
                self.array_instantiations.insert(element_c_type.clone());

                let suffix = UserTypeInstantiation::suffix(&element_c_type);
                let expr_string = format!("make_Array_{suffix}({length}, ({element_c_type}[]){{ {code} }})");

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::MapLiteral { left_brace, .. } => {
                Err(CodegenError::UnsupportedMapLiteral { line: left_brace.line, column: left_brace.column })
            }
            Expr::RangeLiteral { start, end, is_inclusive, range_token } => {
                let mut pre_expr_stmts = Vec::new();

                let start_code = self.generate_expr(start)?;
                pre_expr_stmts.extend(start_code.pre_expr_stmts);
                let start_expr = start_code.expr;

                let end_code = self.generate_expr(end)?;
                pre_expr_stmts.extend(end_code.pre_expr_stmts);
                let end_expr = end_code.expr;

                let inclusive_code = if *is_inclusive { "true" } else { "false" };
                
                let range_type = &self.types.expr_types[&(expr as *const Expr)];

                let element_type = match range_type {
                    TypeExpr::Named { identifier, type_arguments, .. } 
                        if (identifier.lexeme == "Range" || identifier.lexeme == "InclusiveRange")
                            && type_arguments.len() == 1 => 
                    {
                        &type_arguments[0]
                    }
                    _ => {
                        return Err(CodegenError::InvalidRangeType {
                            line: range_token.line,
                            column: range_token.column,
                            range_type: range_type.clone() 
                        });
                    }
                };

                let element_c_type = self.map_type(element_type);
                self.range_instantiations.insert(element_c_type.clone());

                let suffix = UserTypeInstantiation::suffix(&element_c_type);
                let expr_string = format!("make_Range_{suffix}({start_expr}, {end_expr}, {inclusive_code})");

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Variable(identifier) => {
                let var_type = self.types.expr_types.get(&(expr as *const Expr));

                let expr_string = if let Some(var_type) = var_type {
                    match var_type {
                        TypeExpr::Named { identifier: type_identifier, type_arguments, .. }
                            if type_identifier.lexeme == "Optional"
                                && type_arguments.len() == 1
                                && identifier.lexeme == "None" =>
                        {
                            let enum_c_type = self.map_type(var_type);
                            let tag_const = format!("{enum_c_type}_None");
                            format!("({enum_c_type}){{ .tag = {tag_const} }}")
                        }

                        TypeExpr::Named { identifier: type_identifier, .. } if type_identifier.lexeme == "Bool" => {
                            match identifier.lexeme.as_str() {
                                "True" => "true".to_string(),
                                "False" => "false".to_string(),
                                _ => identifier.lexeme.clone(),
                            }
                        }

                        _ => identifier.lexeme.clone(),
                    }
                } else {
                    identifier.lexeme.clone()
                };

                Ok(ExprCode {
                    pre_expr_stmts: Vec::new(),
                    expr: expr_string
                })
            }
            Expr::StructInstantiation { fields, .. } => {
                let mut pre_expr_stmts = Vec::new();

                let struct_type = &self.types.expr_types[&(expr as *const Expr)];
                let struct_c_type = self.map_type(struct_type);

                let mut field_strings = Vec::new();
                for (field_name, field_expr) in fields {
                    let expr_code = self.generate_expr(field_expr)?;
                    pre_expr_stmts.extend(expr_code.pre_expr_stmts);
                    field_strings.push(format!(".{} = {}", field_name.lexeme, expr_code.expr));
                }

                let expr_string = format!("({}){{{}}}", struct_c_type, field_strings.join(", "));

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Assign { identifier, value } => {
                let mut pre_expr_stmts = Vec::new();
                let expr_code = self.generate_expr(value)?;
                pre_expr_stmts.extend(expr_code.pre_expr_stmts);
                
                let expr_string = format!("({} = {})", identifier.lexeme, expr_code.expr);
                
                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::AssignIndex { target, index, value, .. } => {
                let mut pre_expr_stmts = Vec::new();
                
                let target_code = self.generate_expr(target)?;
                pre_expr_stmts.extend(target_code.pre_expr_stmts);
                let target_expr = target_code.expr;
                
                let index_code = self.generate_expr(index)?;
                pre_expr_stmts.extend(index_code.pre_expr_stmts);
                let index_expr = index_code.expr;

                let value_code = self.generate_expr(value)?;
                pre_expr_stmts.extend(value_code.pre_expr_stmts);
                let value_expr = value_code.expr;

                let expr_string = format!("({target_expr}.data[{index_expr}] = {value_expr})");
                
                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::AssignTimeTravelAbsolute { .. } => {
                todo!()
            }
            Expr::AssignTimeTravelRelative { .. } => {
                todo!()
            }
            Expr::Grouping(expr) => {
                let mut pre_expr_stmts = Vec::new();
                let expr_code = self.generate_expr(expr)?;
                pre_expr_stmts.extend(expr_code.pre_expr_stmts);

                let expr_string = format!("({})", expr_code.expr); 

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Unary { operator, right } => {
                let mut pre_expr_stmts = Vec::new();
                let expr_code = self.generate_expr(right)?;
                pre_expr_stmts.extend(expr_code.pre_expr_stmts);
                
                let expr_string = format!("({}{})", operator.lexeme, expr_code.expr);

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Binary { left, operator, right } => {
                let mut pre_expr_stmts = Vec::new();

                let left_code = self.generate_expr(left)?;
                pre_expr_stmts.extend(left_code.pre_expr_stmts);
                let left_expr = left_code.expr;

                let right_code = self.generate_expr(right)?;
                pre_expr_stmts.extend(right_code.pre_expr_stmts);
                let right_expr = right_code.expr;

                if operator.lexeme == "+" {
                    if let Some(TypeExpr::Named { identifier, .. }) = self.types.expr_types.get(&(expr as *const Expr)) {
                        if identifier.lexeme == "String" {
                            let expr_string = format!("string_concat({left_expr}, {right_expr})");

                            return Ok(ExprCode {
                                pre_expr_stmts,
                                expr: expr_string
                            });
                        }
                    }
                }
                
                let expr_s = format!("({} {} {})", left_expr, operator.lexeme, right_expr);
                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_s
                })
            }
            Expr::Logical { left, operator, right } => {
                let mut pre_expr_stmts = Vec::new();

                let left_code = self.generate_expr(left)?;
                pre_expr_stmts.extend(left_code.pre_expr_stmts);
                let left_expr = left_code.expr;
                
                let right_code = self.generate_expr(right)?;
                pre_expr_stmts.extend(right_code.pre_expr_stmts);
                let right_expr = right_code.expr;
                
                let expr_string = format!("({} {} {})", left_expr, operator.lexeme, right_expr);

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::Call { callee, arguments, .. } => {
                let mut pre_expr_stmts = Vec::new();

                let callee_code = self.generate_expr(callee)?;
                pre_expr_stmts.extend(callee_code.pre_expr_stmts);
                let callee_expr = callee_code.expr;

                if let Expr::Variable(ref identifier) = **callee {
                     if identifier.lexeme == "print" && arguments.len() == 1 {
                        let argument = &arguments[0];
                        let argument_code = self.generate_expr(argument)?;
                        pre_expr_stmts.extend(argument_code.pre_expr_stmts);

                        let argument_type = &self.types.expr_types[&(argument as *const Expr)];
                        let helper = match argument_type {
                            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Int" => "print_int",
                            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Double" => "print_double",
                            TypeExpr::Named { identifier, .. } if identifier.lexeme == "Bool" => "print_bool",
                            TypeExpr::Named { identifier, .. } if identifier.lexeme == "String" => "print_string",
                            _ => {
                                return Err(CodegenError::InvalidCallTarget {
                                    line: identifier.line,
                                    column: identifier.column,
                                    target_type: Some(argument_type.clone()),
                                });
                            }
                        };

                        let expr_string = format!("{helper}({})", argument_code.expr);

                        return Ok(ExprCode {
                            pre_expr_stmts,
                            expr: expr_string,
                        });
                    }
                    
                    let call_type = &self.types.expr_types.get(&(expr as *const Expr)).ok_or(CodegenError::MissingType {
                        line: identifier.line,
                        column: identifier.column,
                        identifier: identifier.lexeme.clone(),
                    })?;

                    if let TypeExpr::Named { enum_variants, .. } = call_type {
                        if let Some(variants) = enum_variants {
                            if let Some(variant) = variants.iter().find(|v| v.identifier.lexeme == identifier.lexeme) {
                                let mut argument_exprs = Vec::new();
                                for argument in arguments {
                                    let argument_code = self.generate_expr(argument)?;
                                    pre_expr_stmts.extend(argument_code.pre_expr_stmts);
                                    argument_exprs.push(argument_code.expr);
                                }

                                let enum_c_type = self.map_type(call_type);
                                let tag_const = format!("{}_{}", enum_c_type, variant.identifier.lexeme);

                                let expr_string = if variant.payload_types.is_empty() {
                                    if !argument_exprs.is_empty() {
                                        return Err(CodegenError::InvalidEnumArgumentCount {
                                            line: variant.identifier.line,
                                            column: variant.identifier.column,
                                            variant_identifier: variant.identifier.lexeme.clone(),
                                            expected: 0,
                                            found: argument_exprs.len() 
                                        });
                                    }
                                    format!("({enum_c_type}){{ .tag = {tag_const} }}")
                                } else if variant.payload_types.len() == 1 {
                                    if argument_exprs.len() != 1 {
                                        return Err(CodegenError::InvalidEnumArgumentCount {
                                            line: variant.identifier.line,
                                            column: variant.identifier.column,
                                            variant_identifier: variant.identifier.lexeme.clone(),
                                            expected: 1,
                                            found: argument_exprs.len()
                                        });
                                    }
                                    let value_expr = &argument_exprs[0];
                                    format!("({}){{ .tag = {}, .data.{} = {{ .value = {} }} }}", enum_c_type, tag_const, identifier.lexeme, value_expr)
                                } else {
                                    return Err(CodegenError::UnsupportedEnumVariantPayload {
                                        line: variant.identifier.line,
                                        column: variant.identifier.column,
                                        enum_identifier: enum_c_type.clone(),
                                        variant_identifier: variant.identifier.lexeme.clone(),
                                    });
                                };

                                return Ok(ExprCode { 
                                    pre_expr_stmts, 
                                    expr: expr_string 
                                });
                            }
                        }
                    }
                }

                let mut argument_exprs = Vec::new();
                for argument in arguments {
                    let argument_code = self.generate_expr(argument)?;
                    pre_expr_stmts.extend(argument_code.pre_expr_stmts);
                    argument_exprs.push(argument_code.expr);
                }
                let arguments_code = argument_exprs.join(", ");

                let mut callee_identifier: Option<String> = None;
                if let Expr::Variable(ref ident) = **callee {
                    callee_identifier = Some(ident.lexeme.clone());
                }

                if let Some(ref identifier) = callee_identifier {
                    if let Some(function) = self.ast_functions.get(identifier) {
                        let instantiation = if function.type_parameters.is_empty() {
                            FunctionInstantiation {
                                base_identifier: identifier.clone(),
                                argument_c_types: Vec::new(),
                            }
                        } else {
                            let call_type = &self.types.expr_types[&(expr as *const Expr)];
                            let call_c_type = self.map_type(call_type);

                            FunctionInstantiation {
                                base_identifier: identifier.clone(),
                                argument_c_types: vec![call_c_type.clone()],
                            }
                        };                        

                        let instantiated_c_identifier = instantiation.c_identifier();
                        self.function_instantiations.insert(instantiation);

                        let expr_string = format!("{instantiated_c_identifier}({arguments_code})");
                        return Ok(ExprCode { 
                            pre_expr_stmts, 
                            expr: expr_string 
                        });
                    }
                }

                let optional_callee_type = self.types.expr_types.get(&(callee.as_ref() as *const Expr));

                if let Some(TypeExpr::Function { parameter_types, return_type }) = optional_callee_type {
                    let return_c_type = self.map_type(return_type);

                    let parameter_c_types: Vec<String> =
                        parameter_types.iter().map(|t| self.map_type(t)).collect();

                    let function_parameter_types = if parameter_c_types.is_empty() {
                        "void*".to_string()
                    } else {
                        format!("void*, {}", parameter_c_types.join(", "))
                    };

                    let closure_temp = self.new_temp_id("closure_");
                    pre_expr_stmts.push(format!("Fn {closure_temp} = {callee_expr};"));

                    let mut call_arguments = Vec::new();
                    call_arguments.push(format!("{closure_temp}.env"));
                    call_arguments.extend(argument_exprs.into_iter());
                    let args_code = call_arguments.join(", ");

                    let expr_string = format!("(({return_c_type} (* )({function_parameter_types}))({closure_temp}.fn))({args_code})");

                    return Ok(ExprCode {
                        pre_expr_stmts,
                        expr: expr_string,
                    });
                }
                

                let (line, column) = match &**callee {
                    Expr::Variable(ident) => (ident.line, ident.column),
                    _ => (0, 0),
                };

                Err(CodegenError::InvalidCallTarget {
                    line,
                    column,
                    target_type: optional_callee_type.cloned() 
                })
            }
            Expr::If { condition, then_branch, else_branch, if_token } => {
                let mut pre_expr_stmts = Vec::new();

                let condition_code = self.generate_expr(condition)?;
                pre_expr_stmts.extend(condition_code.pre_expr_stmts);
                let condition_expr = condition_code.expr;

                let result_type = &self.types.expr_types[&(expr as *const Expr)];
                let result_c_type = self.map_type(result_type);
                let result_temp = self.new_temp_id("if_result_");

                pre_expr_stmts.push(format!("{result_c_type} {result_temp};"));

                pre_expr_stmts.push(format!("if ({condition_expr}) {{"));
                self.indent();
                let then_code = self.generate_expr(then_branch)?;
                for line in then_code.pre_expr_stmts {
                    pre_expr_stmts.push(line);
                }
                pre_expr_stmts.push(format!("{result_temp} = {};", then_code.expr));
                self.un_indent();
                pre_expr_stmts.push("} else {".to_string());

                let else_expr = else_branch.as_ref().ok_or(CodegenError::MissingElseInIfExpression {
                    line: if_token.line,
                    column: if_token.column,
                })?;
                self.indent();
                let else_code = self.generate_expr(else_expr)?;
                for line in else_code.pre_expr_stmts {
                    pre_expr_stmts.push(line);
                }
                pre_expr_stmts.push(format!("{result_temp} = {};", else_code.expr));
                self.un_indent();
                pre_expr_stmts.push("}".to_string());

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: result_temp,
                })
            }
            Expr::Match { subject, cases, .. } => {
                let mut pre_expr_stmts = Vec::new();

                let subject_expr = self.generate_expr(subject)?;
                pre_expr_stmts.extend(subject_expr.pre_expr_stmts);
                let subject_expr = subject_expr.expr;

                let subject_type = &self.types.expr_types[&(&**subject as *const Expr)];
                let enum_c_type = self.map_type(subject_type);

                let result_type = &self.types.expr_types[&(expr as *const Expr)];
                let result_c_type = self.map_type(result_type);

                let subject_temp = self.new_temp_id("match_subject_");
                let result_temp = self.new_temp_id("match_result_");

                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{enum_c_type} {subject_temp} = {subject_expr};"));
                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("{result_c_type} {result_temp};"));
                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, format!("switch ({subject_temp}.tag) {{"));

                self.indent();
                for case in cases {
                    self.append_match_case(&mut pre_expr_stmts, &enum_c_type, &subject_temp, &result_temp, subject_type, case)?;
                }
                self.un_indent();

                self.write_indented_to(&mut pre_expr_stmts, self.indent_level, "}".to_string());

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: result_temp,
                })
            }
            Expr::Lambda { parameters, return_type, body } => {
                let mut pre_expr_stmts = Vec::new();

                let lambda_base = self.new_temp_id("lambda_");
                let environment_name = format!("{lambda_base}_env");

                let captures = self.types.lambda_captures.get(&(expr as *const Expr)).cloned().unwrap_or_default();

                self.write_line(&format!("typedef struct {environment_name} {{"));
                self.indent();
                for (identifier, type_expr) in &captures {
                    let field_c_type = self.map_type(type_expr);
                    self.write_line(&format!("{field_c_type} {identifier};"));
                }
                self.un_indent();
                self.write_line(&format!("}} {environment_name};"));
                self.write_line("");

                let return_c_type = if let Some(TypeExpr::Function { return_type, .. }) =
                    self.types.expr_types.get(&(expr as *const Expr))
                {
                    self.map_type(return_type)
                } else if let Some(ret_type) = return_type {
                    self.map_type(ret_type)
                } else {
                    "void".to_string()
                };

                let mut parameter_declarations = Vec::new();
                parameter_declarations.push("void* env".to_string());
                for (parameter_identifier, parameter_type) in parameters {
                    let parameter_c_type = self.map_type(parameter_type);
                    parameter_declarations.push(format!("{} {}", parameter_c_type, parameter_identifier.lexeme));
                }
                let parameters_code = parameter_declarations.join(", ");

                self.write_line(&format!("{return_c_type} {lambda_base}({parameters_code}) {{"));
                self.indent();

                for (index, stmt) in body.iter().enumerate() {
                    let is_last = index == body.len() - 1;

                    if return_c_type != "void" && is_last {
                        if let Stmt::Expression(last_expr) = stmt {
                            let last_code = self.generate_expr(last_expr)?;
                            for line in last_code.pre_expr_stmts {
                                self.write_line(&line);
                            }
                            self.write_line(&format!("return {};", last_code.expr));
                            continue;
                        }
                    }

                    let mut lines = Vec::new();
                    self.generate_stmt_to(&mut lines, stmt)?;
                    for line in lines {
                        self.write_line(&line);
                    }
                }

                self.un_indent();
                self.write_line("}");

                let environment_ptr = self.new_temp_id("env_");
                self.write_line("");
                self.write_indented_to(
                    &mut pre_expr_stmts,
                    self.indent_level,
                    format!("{environment_name}* {environment_ptr} = ({environment_name}*)malloc(sizeof({environment_name}));"),
                );

                for (identifier, _) in &captures {
                    self.write_indented_to(
                        &mut pre_expr_stmts,
                        self.indent_level,
                        format!("{environment_ptr}->{identifier} = {identifier};"),
                    );
                }

                let closure_expr = format!(
                    "(Fn){{ .env = (void*){environment_ptr}, .fn = (void*)&{lambda_base} }}"
                );

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: closure_expr
                })
            }
            Expr::Block { stmts, left_brace } => {
                let mut pre_expr_stmts = Vec::new();

                if stmts.is_empty() {
                    return Err(CodegenError::BlockExpressionMissingEndExpression {
                        line: left_brace.line,
                        column: left_brace.column,
                    });
                }

                for stmt in &stmts[..stmts.len() - 1] {
                    self.generate_stmt_to(&mut pre_expr_stmts, stmt)?;
                }

                match stmts.last().unwrap() {
                    Stmt::Expression(last_expr) => {
                        let last_code = self.generate_expr(last_expr)?;
                        pre_expr_stmts.extend(last_code.pre_expr_stmts);
                        Ok(ExprCode {
                            pre_expr_stmts,
                            expr: last_code.expr,
                        })
                    }
                    _ => {
                        return Err(CodegenError::BlockExpressionMissingEndExpression {
                            line: left_brace.line,
                            column: left_brace.column,
                        });
                    }
                }
            }
            Expr::IndexAccess { target, index, .. } => {
                let mut pre_expr_stmts = Vec::new();
                
                let target_code = self.generate_expr(target)?;
                pre_expr_stmts.extend(target_code.pre_expr_stmts);
                let target_expr = target_code.expr;

                let index_code = self.generate_expr(index)?;
                pre_expr_stmts.extend(index_code.pre_expr_stmts);
                let index_expr = index_code.expr;
                
                let expr_string = format!("({target_expr}.data[{index_expr}])");

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string
                })
            }
            Expr::FieldAccess { target, field } => {
                let mut pre_expr_stmts = Vec::new();

                let target_code = self.generate_expr(target)?;
                pre_expr_stmts.extend(target_code.pre_expr_stmts);
                let target_expr = target_code.expr;

                let expr_string = format!("({target_expr}.{})", field.lexeme);

                Ok(ExprCode {
                    pre_expr_stmts,
                    expr: expr_string,
                })
            }
            Expr::TimeTravelAbsolute { .. } => {
                todo!()
            }
            Expr::TimeTravelRelative { .. } => {
                todo!()
            }
        }
    }

    fn append_match_case(&mut self, out: &mut Vec<String>, enum_c_type: &str, subject_temp: &str, result_temp: &str, subject_type: &TypeExpr, case: &MatchCase) -> Result<(), CodegenError> {
        match &case.pattern {
            pattern if self.resolve_zero_ary_variant(subject_type, pattern).is_some() => {
                let variant = self.resolve_zero_ary_variant(subject_type, pattern).unwrap();
                let variant_identifier = &variant.identifier.lexeme;
                let tag_const = format!("{enum_c_type}_{variant_identifier}");

                self.write_indented_to(out, self.indent_level, format!("case {tag_const}: {{"));
                self.indent();

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        self.write_indented_to(out, self.indent_level, line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::EnumVariant { identifier, .. } => {
                let variant_identifier = &identifier.lexeme;
                let tag_const = format!("{enum_c_type}_{variant_identifier}");
                self.write_indented_to(out, self.indent_level, format!("case {tag_const}: {{"));
                self.indent();

                self.lower_pattern(out, &case.pattern, subject_temp, subject_type)?;

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        self.write_indented_to(out, self.indent_level, line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::Variable(identifier) => {
                let var_identifier = &identifier.lexeme;
                self.write_indented_to(out, self.indent_level, "default: {".to_string());
                self.indent();

                let c_type = self.map_type(subject_type);
                self.write_indented_to(out, self.indent_level, format!("{c_type} {var_identifier} = {subject_temp};"));

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        self.write_indented_to(out, self.indent_level, line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::Wildcard(_) => {
                self.write_indented_to(out, self.indent_level, "default: {".to_string());
                self.indent();

                if let Some(guard_expr) = &case.guard {
                    let guard_code = self.generate_expr(guard_expr)?;
                    for line in guard_code.pre_expr_stmts {
                        out.push(line);
                    }
                    self.write_indented_to(out, self.indent_level, format!("if (!({})) {{ break; }}", guard_code.expr));
                }

                let body_code = self.generate_expr(&case.body)?;
                for line in body_code.pre_expr_stmts {
                    self.write_indented_to(out, self.indent_level, line);
                }
                self.write_indented_to(out, self.indent_level, format!("{result_temp} = {};", body_code.expr));
                self.write_indented_to(out, self.indent_level, "break;".to_string());

                self.un_indent();
                self.write_indented_to(out, self.indent_level, "}".to_string());
            }
            Pattern::Literal(literal) => {
                return Err(CodegenError::UnsupportedMatchPattern {
                    line: literal.line,
                    column: literal.column,
                })
            }
            Pattern::Range { start, .. } => {
                return Err(CodegenError::UnsupportedMatchPattern {
                    line: start.line,
                    column: start.column,
                })
            }
        }

        Ok(())
    }

    fn resolve_zero_ary_variant(&self, subject_type: &TypeExpr, pattern: &Pattern) -> Option<EnumVariant> {
        let variant_identifier = match pattern {
            Pattern::EnumVariant { identifier, arguments } => {
                if !arguments.is_empty() {
                    return None;
                }
                &identifier.lexeme
            }
            Pattern::Variable(identifier) => &identifier.lexeme,
            _ => return None,
        };

        let TypeExpr::Named { enum_variants, .. } = subject_type else {
            return None;
        };

        let variants = enum_variants.as_ref()?;

        variants.iter().find(|v| v.identifier.lexeme == *variant_identifier && v.payload_types.is_empty()).cloned()
    }

    fn lower_pattern(&mut self, out: &mut Vec<String>, pattern: &Pattern, subject_expr: &str, subject_type: &TypeExpr) -> Result<(), CodegenError> {
        match pattern {
            Pattern::Wildcard(_) => {}
            Pattern::Variable(identifier) => {
                let var_identifier = &identifier.lexeme;
                let c_type = self.map_type(subject_type);
                self.write_indented_to(out, self.indent_level, format!("{c_type} {var_identifier} = {subject_expr};"));
            }
            Pattern::Literal(literal) => {
                let literal_expr = literal.lexeme.clone();
                self.write_indented_to(out, self.indent_level, format!("if ({subject_expr} != {literal_expr}) {{ break; }}"));
            }
            Pattern::EnumVariant { identifier, arguments } => {
                let TypeExpr::Named { enum_variants, .. } = subject_type else {
                    return Err(CodegenError::InvalidEnumPatternSubjectType {
                        line: identifier.line,
                        column: identifier.column,
                        subject_type: subject_type.clone(),
                    });
                };
                let variants = enum_variants.as_ref().ok_or(CodegenError::MissingType { 
                    line: identifier.line, 
                    column: identifier.column, 
                    identifier: identifier.lexeme.clone() 
                })?;
                let variant = variants.iter().find(|v| v.identifier.lexeme == identifier.lexeme).ok_or(CodegenError::UnknownEnumVariantInPattern {
                    line: identifier.line,
                    column: identifier.column,
                    enum_identifier: identifier.lexeme.clone(),
                })?;

                let instantiated_variant = self.instantiate_enum_variant_payloads(subject_type, variant);

                if arguments.len() != instantiated_variant.payload_types.len() {
                    return Err(CodegenError::InvalidEnumPatternArgumentCount {
                        line: identifier.line,
                        column: identifier.column,
                        enum_identifier: identifier.lexeme.clone(),
                        variant_identifier: instantiated_variant.identifier.lexeme.clone(),
                        expected: instantiated_variant.payload_types.len(),
                        found: arguments.len(),
                    });
                }

                for (index, (argument_pattern, payload_type)) in arguments.iter().zip(instantiated_variant.payload_types.iter()).enumerate() {                    
                    let field_identifier = if instantiated_variant.payload_types.len() == 1 {
                        "value".to_string()
                    } else {
                        format!("value{index}")
                    };

                    let payload_expr = format!("{}.data.{}.{}", subject_expr, identifier.lexeme, field_identifier);

                    self.lower_pattern(out, argument_pattern, &payload_expr, &payload_type)?;
                }
            }
            Pattern::Range { start, end, is_inclusive } => {
                let start_expr = start.lexeme.clone();
                let end_expr = end.lexeme.clone();
                let check = if *is_inclusive {
                    format!("if ({subject_expr} < {start_expr} || {subject_expr} > {end_expr}) {{ break; }}")
                } else {
                    format!("if ({subject_expr} <= {start_expr} || {subject_expr} >= {end_expr}) {{ break; }}")
                };
                self.write_indented_to(out, self.indent_level, check);
            }
        }

        Ok(())
    }

    fn instantiate_enum_variant_payloads(&self, subject_type: &TypeExpr, variant: &EnumVariant) -> EnumVariant {
        let TypeExpr::Named { type_parameters, type_arguments, .. } = subject_type else {
            return variant.clone();
        };

        let type_parameters = match type_parameters {
            Some(parameters) => parameters,
            None => return variant.clone()
        };

        let mut mapping: HashMap<usize, TypeExpr> = HashMap::new();
        for (parameter, argument) in type_parameters.iter().zip(type_arguments.iter()) {
            if let TypeExpr::TypeVar { id } = parameter {
                mapping.insert(*id, argument.clone());
            }
        }

        let new_payloads = variant.payload_types.iter().map(|t| self.substitute_local(t, &mapping)).collect();

        EnumVariant {
            identifier: variant.identifier.clone(),
            payload_types: new_payloads,
        }
    }

    fn substitute_local(&self, type_expr: &TypeExpr, mapping: &HashMap<usize, TypeExpr>) -> TypeExpr {
        match type_expr {
            TypeExpr::TypeVar { id } => mapping.get(id).cloned().unwrap_or_else(|| type_expr.clone()),
            TypeExpr::Named { identifier, type_parameters, type_arguments, is_optional, enum_variants, struct_fields } => TypeExpr::Named {
                identifier: identifier.clone(),
                type_parameters: type_parameters.clone(),
                type_arguments: type_arguments.iter().map(|t| self.substitute_local(t, mapping)).collect(),
                is_optional: *is_optional,
                enum_variants: enum_variants.clone(),
                struct_fields: struct_fields.clone(),
            },
            TypeExpr::Function { parameter_types, return_type } => TypeExpr::Function {
                parameter_types: parameter_types.iter().map(|t| self.substitute_local(t, mapping)).collect(),
                return_type: Box::new(self.substitute_local(return_type, mapping)),
            },
        }
    }

    fn generate_declarations(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Struct { identifier, type_parameters, fields } => {
                self.ast_structs.insert(identifier.lexeme.clone(), stmt.clone());

                if type_parameters.is_empty() {
                    self.generate_struct_declaration(identifier, fields); 
                }
            }
            Stmt::Enum { identifier, .. } => {
                self.ast_enums.insert(identifier.lexeme.clone(), stmt.clone());
            }
            Stmt::Function(function) => {
                self.ast_functions.insert(function.identifier.lexeme.clone(), function.as_ref().clone());
            }
            _ => {}
        }
    }

    fn generate_struct_declaration(&mut self, identifier: &Token, fields: &Vec<(Token, TypeExpr)>) {
        self.write_line(&format!("typedef struct {} {{", identifier.lexeme));
        self.indent();
        for (field_identifier, field_type) in fields {
            let c_field_type = self.map_type(field_type);
            self.write_line(&format!("{} {};", c_field_type, field_identifier.lexeme))
        }
        self.un_indent();
        self.write_line(&format!("}} {};", identifier.lexeme));
        self.write_line("");
    }

    fn map_type(&mut self, type_expr: &TypeExpr) -> String {
        match type_expr {
            TypeExpr::Named { identifier, type_arguments, .. } => {
                let c_identifier = match identifier.lexeme.as_str() {
                    "Int" => "int".to_string(),
                    "Double" => "double".to_string(),
                    "String" => "const char*".to_string(),
                    "Bool" => "bool".to_string(),
                    "Void" => "void".to_string(),
                    "Array" if type_arguments.len() == 1 => {
                        let c_element = self.map_type(&type_arguments[0]);
                        let suffix = UserTypeInstantiation::suffix(&c_element);
                        self.array_instantiations.insert(c_element.clone());
                        format!("Array_{suffix}")
                    }
                    "Map" if type_arguments.len() == 2 => {
                        "Map".to_string()
                    }
                    "Range" | "InclusiveRange" if type_arguments.len() == 1 => {
                        let c_bound = self.map_type(&type_arguments[0]);
                        let suffix = UserTypeInstantiation::suffix(&c_bound);
                        self.range_instantiations.insert(c_bound.clone());
                        format!("Range_{suffix}")
                    }

                    _ => {
                        if type_arguments.is_empty() {
                            identifier.lexeme.clone()
                        } else {
                            let argument_c_types = type_arguments.iter().map(|t| self.map_type(t)).collect();
                            let instantiation = UserTypeInstantiation { identifier: identifier.lexeme.clone(), argument_c_types };
                            let c_identifier = instantiation.c_identifier();
                            self.user_type_instantiations.insert(instantiation);

                            c_identifier
                        }
                    }
                };

                c_identifier
            }
            TypeExpr::Function { .. } => "Fn".to_string(),
            TypeExpr::TypeVar { .. } => "void*".to_string()
        }
    }

    fn map_type_with_parameters(&mut self, type_expr: &TypeExpr, bindings: &[(String, String)]) -> String {
        match type_expr {
            TypeExpr::Named { identifier, type_arguments, is_optional, .. } => {
                if type_arguments.is_empty() {
                    if let Some((_, c_type)) = bindings.iter().find(|(parameter_identifier, _)| parameter_identifier == &identifier.lexeme) {
                        let base_c_type = c_type.clone();

                        if *is_optional {
                            let instantiation = UserTypeInstantiation {
                                identifier: "Optional".to_string(),
                                argument_c_types: vec![base_c_type.clone()],
                            };
                            self.user_type_instantiations.insert(instantiation);
                            
                            let suffix = UserTypeInstantiation::suffix(&base_c_type);
                            return format!("Optional_{suffix}");
                        }

                        return base_c_type;
                    }
                }
                self.map_type(type_expr)
            }
            _ => self.map_type(type_expr),
        }
    }

    fn new_temp_id(&mut self, prefix: &str) -> String {
        let id = self.temp_id_counter;
        self.temp_id_counter += 1;
        format!("__{prefix}{id}")
    }

    fn write_line(&mut self, line: &str) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
        self.output.push_str(line);
        self.output.push_str("\n");
    }

    fn write_indented_to(&mut self, out: &mut Vec<String>, indent: usize, text: String) {
        let mut string = String::new();
        for _ in 0..indent {
            string.push_str("    ");
        }
        string.push_str(&text);
        out.push(string);
    }
    
    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn un_indent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }
}