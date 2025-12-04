use std::rc::Rc;

use crate::error::{error_token, ParseError};
use crate::token::{Literal, Token, TokenType};
use crate::expr::{Expr, MatchCase, Pattern};
use crate::stmt::{EnumVariant, FunctionStmt, Stmt};
use crate::type_checker::TypeExpr;

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
        }
    }

    pub fn parse(&mut self) -> Vec<Stmt> {
        let mut statements = Vec::new();
        
        while !self.is_at_end() {
            if let Some(stmt) = self.declaration() {
                statements.push(stmt);
            }
        }

        statements
    }

     fn declaration(&mut self) -> Option<Stmt> {
        let result = if self.matches(&[TokenType::Var]) {
            self.var_declaration()
        } else if self.matches(&[TokenType::Enum]) {
            self.enum_declaration()
        } else if self.matches(&[TokenType::Struct]) {
            self.struct_declaration()
        } else if self.matches(&[TokenType::Fn]) {
            self.function_declaration("function")
        } else {
            self.statement()
        };

        match result {
            Ok(stmt) => Some(stmt),
            Err(_) => {
                self.synchronize();
                None
            }
        }
    }

    fn var_declaration(&mut self) -> Result<Stmt, ParseError> {
        let name = self.consume(TokenType::Identifier, "Expect variable name.")?.clone();

        let type_annotation = if self.matches(&[TokenType::Colon]) {
            Some(self.type_expr()?)
        } else {
            None
        };

        let initializer = if self.matches(&[TokenType::Equal]) {
            Some(self.expression()?)
        } else {
            None
        };

        Ok(Stmt::Var { 
            name,
            type_annotation, 
            initializer,
        })
    }

    fn enum_declaration(&mut self) -> Result<Stmt, ParseError> {
        let name = self.consume(TokenType::Identifier, "Expect enum name.")?.clone();

        let type_parameters = self.type_parameters()?;

        self.consume(TokenType::LeftBrace, "Expect '{' before enum variants.")?;
        let mut variants = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            let variant_name = self.consume(TokenType::Identifier, "Expect enum variant name.")?.clone();

            let mut payload_types = Vec::new();
            if self.matches(&[TokenType::LeftParen]) {
                if !self.check(&TokenType::RightParen) {
                    loop {
                        payload_types.push(self.type_expr()?);
                        if !self.matches(&[TokenType::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(TokenType::RightParen, "Expect ')' after variant payload types.")?;
            }

            variants.push(EnumVariant {
                name: variant_name,
                payload_types,
            });

            if !(self.matches(&[TokenType::Comma]) || self.check(&TokenType::RightBrace)) {
                return Err(self.error(self.peek(), "Expect ',' or '}' after enum variant."));
            }
        }

        self.consume(TokenType::RightBrace, "Expect '}' after enum variants.")?;

        Ok(Stmt::Enum {
            name,
            type_parameters,
            variants,
        })
    }

    fn struct_declaration(&mut self) -> Result<Stmt, ParseError> {
        let name = self.consume(TokenType::Identifier, "Expect struct name.")?.clone();

        let type_parameters = self.type_parameters()?;

        self.consume(TokenType::LeftBrace, "Expect '{' before struct fields.")?;
        let mut fields = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            let field_name = self.consume(TokenType::Identifier, "Expect field name.")?.clone();
            self.consume(TokenType::Colon, "Expect ':' after field name.")?;

            let field_type = self.type_expr()?;
            fields.push((field_name, field_type));
            
            if !(self.matches(&[TokenType::Comma]) || self.check(&TokenType::RightBrace)) {
                return Err(self.error(self.peek(), "Expect ',' or '}' after struct field."));
            }
        }

        self.consume(TokenType::RightBrace, "Expect '}' after struct fields.")?;

        Ok(Stmt::Struct {
            name,
            type_parameters,
            fields,
        })
    }

    fn function_declaration(&mut self, kind: &str) -> Result<Stmt, ParseError> {
        let name = self.consume(TokenType::Identifier, &format!("Expect {} name.", kind))?.clone();

        let type_parameters = self.type_parameters()?;

        self.consume(TokenType::LeftParen, &format!("Expect '(' after {} name.", kind))?;
        let mut parameters = Vec::new();
        if !self.check(&TokenType::RightParen) {
            loop {
                let parameter_name = self.consume(TokenType::Identifier, "Expect parameter name.")?.clone();
                self.consume(TokenType::Colon, "Expect ':' after parameter name.")?;
                let parameter_type = self.type_expr()?;
                parameters.push((parameter_name, parameter_type));
                if !self.matches(&[TokenType::Comma]) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, &format!("Expect ')' after parameters."))?;

        let return_type = if self.matches(&[TokenType::MinusGreater]) {
            Some(self.type_expr()?)
        } else {
            None
        };

        self.consume(TokenType::LeftBrace, &format!("Expect '{{' before {} body.", kind))?;
        let body = self.block_statement()?;

        Ok(Stmt::Function(Rc::new(FunctionStmt { 
            name,
            type_parameters,
            parameters, 
            return_type,
            body 
        })))
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        if self.matches(&[TokenType::If]) {
            return self.if_statement();
        } else if self.matches(&[TokenType::For]) {
            return self.for_statement();
        } else if self.matches(&[TokenType::While]) {
            return self.while_statement();
        } else if self.matches(&[TokenType::Break]) {
            return Ok(Stmt::Break(self.previous().clone()));
        } else if self.matches(&[TokenType::Continue]) {
            return Ok(Stmt::Continue(self.previous().clone()));
        } else if self.matches(&[TokenType::LeftBrace]) {
            return Ok(Stmt::Block(self.block_statement()?));
        }

        self.expression_statement()
    }
    
    fn if_statement(&mut self) -> Result<Stmt, ParseError> {
        self.consume(TokenType::LeftParen, "Expect '(' after 'if'.")?;
        let condition = self.expression()?;
        self.consume(TokenType::RightParen, "Expect ')' after if condition.")?;

        let then_branch = self.statement()?;

        let else_branch = if self.matches(&[TokenType::Else]) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };

        Ok(Stmt::If { 
            condition, 
            then_branch: Box::new(then_branch), 
            else_branch,
        })
    }

    fn for_statement(&mut self) -> Result<Stmt, ParseError> {
        self.consume(TokenType::LeftParen, "Expect '(' after 'for'.")?;

        // range form: for (name in range) { ... }
        if self.check(&TokenType::Identifier) && self.peek_next().token_type == TokenType::In {
            let name = self.advance().clone();
            self.consume(TokenType::In, "Expect 'in' after loop variable name.")?;
            let range = self.expression()?;
            self.consume(TokenType::RightParen, "Expect ')' after range expression.")?;
            let body = self.statement()?;

            return Ok(Stmt::ForRange {
                name,
                range,
                body: Box::new(body),
            });
        }

        // standard form: for (initializer; condition; increment) { ... } 
        let initializer = if self.check(&TokenType::Semicolon) {
            None
        } else if self.matches(&[TokenType::Var]) {
            Some(self.var_declaration()?)
        } else {
            Some(self.expression_statement()?)
        };
        self.consume(TokenType::Semicolon, "Expect ';' after loop initializer.")?;

        let condition = if !self.check(&TokenType::Semicolon) {
            Some(self.expression()?)
        } else {
            None
        };
        self.consume(TokenType::Semicolon, "Expect ';' after loop condition.")?;

        let increment = if !self.check(&TokenType::RightParen) {
            Some(self.expression()?)
        } else {
            None
        };
        self.consume(TokenType::RightParen, "Expect ')' after for clauses.")?;


        let mut body = self.statement()?;

        if let Some(incr) = increment {
            body = Stmt::Block(vec![body, Stmt::Expression(incr)]);
        }

        let condition = condition.unwrap_or(Expr::Literal(Literal::Number(1.0)));
        let mut while_stmt = Stmt::While {
            condition,
            body: Box::new(body),
        };

        if let Some(init) = initializer {
            while_stmt = Stmt::Block(vec![init, while_stmt]);
        }

        Ok(while_stmt)
    }

    fn while_statement(&mut self) -> Result<Stmt, ParseError> {
        self.consume(TokenType::LeftParen, "Expect '(' after 'while'.")?;
        let condition = self.expression()?;
        self.consume(TokenType::RightParen, "Expect ')' after 'while'.")?;
        let body = self.statement()?;

        Ok(Stmt::While { 
            condition, 
            body: Box::new(body) 
        })
    }

    fn block_statement(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut statements = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if let Some(stmt) = self.declaration() {
                statements.push(stmt);
            }
        }

        self.consume(TokenType::RightBrace, "Expect '}' after block.")?;
        Ok(statements)
    }

    fn expression_statement(&mut self) -> Result<Stmt, ParseError> {
        let value = self.expression()?;
        Ok(Stmt::Expression(value))
    }
    
    fn expression(&mut self) -> Result<Expr, ParseError> {
        if self.matches(&[TokenType::If]) {
            return self.if_expression();
        } else if self.matches(&[TokenType::Match]) {
            return self.match_expression();
        } 
        
        self.assignment()
    }

    fn if_expression(&mut self) -> Result<Expr, ParseError> {
        self.consume(TokenType::LeftParen, "Expect '(' after 'if'.")?;
        let condition = self.expression()?;
        self.consume(TokenType::RightParen, "Expect ')' after if condition.")?;

        let then_branch = self.block_expression()?;
        self.consume(TokenType::Else, "Expect 'else' after if branch.")?;

        let else_branch = if self.matches(&[TokenType::If]) {
            self.if_expression()?
        } else {
            self.block_expression()?
        };

        Ok(Expr::If { 
            condition: Box::new(condition), 
            then_branch: Box::new(then_branch), 
            else_branch: Some(Box::new(else_branch)) 
        })
    }

    fn block_expression(&mut self) -> Result<Expr, ParseError> {
        self.consume(TokenType::LeftBrace, "Expect '{' to start block expression.")?;
        let mut statements = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if let Some(stmt) = self.declaration() {
                statements.push(stmt);
            }
        }

        self.consume(TokenType::RightBrace, "Expect '}' after block expression.")?;

        Ok(Expr::Block(statements))
    }

    fn match_expression(&mut self) -> Result<Expr, ParseError> {
        let match_subject = self.expression()?;

        self.consume(TokenType::LeftBrace, "Expect '{' after match subject.")?;
        let mut cases = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            let pattern = self.pattern()?;

            let guard = if self.matches(&[TokenType::If]) {
                Some(self.expression()?)
            } else {
                None
            };

            self.consume(TokenType::EqualGreater, "Expect '=>' after match pattern.")?;
            let body = self.expression()?;

            cases.push(MatchCase {
                pattern,
                guard,
                body,
            });

            if !self.matches(&[TokenType::Comma]) && !self.check(&TokenType::RightBrace) {
                return Err(self.error(self.peek(), "Expect ',' or '}' after match case."));
            }
        }

        self.consume(TokenType::RightBrace, "Expect '}' after match cases.")?;

        Ok(Expr::Match {
            expr: Box::new(match_subject),
            cases,
        })
    }

    fn pattern(&mut self) -> Result<Pattern, ParseError> {
        if self.matches(&[TokenType::Underscore]) {
            return Ok(Pattern::Wildcard(self.previous().clone()));
        }

        if self.matches(&[TokenType::Number, TokenType::String]) {
            let start_token = self.previous().clone();
            let start_literal = start_token.literal.clone().unwrap();
            let start_type = start_token.token_type.clone();

            if self.matches(&[TokenType::DotDot, TokenType::DotDotEqual]) {
                let is_inclusive = self.previous().token_type == TokenType::DotDotEqual;

                let end_token = self.peek().clone();
                match end_token.token_type {
                    TokenType::Number | TokenType::String => {
                        if end_token.token_type != start_type {
                            return Err(self.error(&end_token, "Range pattern bounds must have the same literal type."));
                        }
                        let end_token = self.advance().clone();
                        let end_literal = end_token.literal.clone().unwrap();

                        return Ok(Pattern::Range {
                            start: start_literal,
                            end: end_literal,
                            is_inclusive,
                        });
                    }
                    _ => {
                        return Err(self.error(&end_token, "Expect literal after '..' in range pattern."));
                    }
                }
            }

            return Ok(Pattern::Literal(start_literal));
        }

        if self.matches(&[TokenType::Identifier]) {
            let name = self.previous().clone();

            if self.matches(&[TokenType::LeftParen]) {
                let mut args = Vec::new();
                if !self.check(&TokenType::RightParen) {
                    loop {
                        args.push(self.pattern()?);
                        if !self.matches(&[TokenType::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(TokenType::RightParen, "Expect ')' after pattern arguments.")?;

                return Ok(Pattern::EnumVariant { name, args });
            }

            return Ok(Pattern::Variable(name)); // TODO: determine if 0-ary EnumVariant with enum variant table
        }

        Err(self.error(self.peek(), "Invalid pattern in match case."))
    }

    fn assignment(&mut self) -> Result<Expr, ParseError> {
        let expr = self.coalesce()?;

        if self.matches(&[
            TokenType::Equal,
            TokenType::PlusEqual,
            TokenType::MinusEqual,
            TokenType::StarEqual,
            TokenType::SlashEqual,
            TokenType::PercentEqual,
            TokenType::CaretEqual,
        ]) {
            let operator_token = self.previous().clone();
            let right_side = self.assignment()?;
            
            let value = if operator_token.token_type == TokenType::Equal {
                right_side
            } else {
                let intermediate_operator_type = match operator_token.token_type {
                    TokenType::PlusEqual => TokenType::Plus,
                    TokenType::MinusEqual => TokenType::Minus,
                    TokenType::StarEqual => TokenType::Star,
                    TokenType::SlashEqual => TokenType::Slash,
                    TokenType::PercentEqual => TokenType::Percent,
                    TokenType::CaretEqual => TokenType::Caret,
                    _ => unreachable!(),
                }; 

                let intermediate_operator = Token {
                    token_type: intermediate_operator_type,
                    lexeme: operator_token.lexeme.clone(),
                    literal: None,
                    line: operator_token.line,
                };
                
                Expr::Binary {
                    left: Box::new(expr.clone()),
                    operator: intermediate_operator,
                    right: Box::new(right_side),
                }
            };

            return match expr { 
                Expr::Variable(name) => Ok(Expr::Assign { 
                    name, 
                    value: Box::new(value) 
                }),
                Expr::IndexAccess { target, index } => Ok(Expr::AssignIndex { 
                    target, 
                    index, 
                    value: Box::new(value) 
                }),
                Expr::TimeTravelAbsolute { target, index } => Ok(Expr::AssignTimeTravelAbsolute { 
                    target, 
                    index, 
                    value: Box::new(value) 
                }),
                Expr::TimeTravelRelative { target, offset } => Ok(Expr::AssignTimeTravelRelative { 
                    target, 
                    offset, 
                    value: Box::new(value) 
                }),
                _ => Err(self.error(&operator_token, "Invalid assignment target."))
            };
        }

        Ok(expr)
    }

    fn coalesce(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.or()?;

        while self.matches(&[TokenType::QuestionQuestion]) {
            let operator = self.previous().clone();
            let right = self.or()?;
            
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.and()?;

        while self.matches(&[TokenType::PipePipe]) {
            let operator = self.previous().clone();
            let right = self.and()?;
            expr = Expr::Logical {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.equality()?;

        while self.matches(&[TokenType::AmpersandAmpersand]) {
            let operator = self.previous().clone();
            let right = self.equality()?;
            expr = Expr::Logical {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.comparison()?;

        while self.matches(&[TokenType::ExclamationEqual, TokenType::EqualEqual]) {
            let operator = self.previous().clone();
            let right = self.comparison()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.range()?;

        while self.matches(&[TokenType::Greater, TokenType::GreaterEqual, TokenType::Less, TokenType::LessEqual]) {
            let operator = self.previous().clone();
            let right = self.range()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn range(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.term()?;  

        if self.matches(&[TokenType::DotDot, TokenType::DotDotEqual]) {
            let is_inclusive = self.previous().token_type == TokenType::DotDotEqual;
            let end = self.term()?;

            expr = Expr::RangeLiteral {
                start: Box::new(expr),
                end: Box::new(end),
                is_inclusive,
            };
        }

        Ok(expr)
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.factor()?;

        while self.matches(&[TokenType::Minus, TokenType::Plus]) {
            let operator = self.previous().clone();
            let right = self.factor()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn factor(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.power()?;

        while self.matches(&[TokenType::Slash, TokenType::Star, TokenType::Percent]) {
            let operator = self.previous().clone();
            let right = self.power()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn power(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.unary()?;

        if self.matches(&[TokenType::Caret]) {
            let operator = self.previous().clone();
            let right = self.power()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.matches(&[TokenType::Exclamation, TokenType::Minus]) {
            let operator = self.previous().clone();
            let right = self.unary()?;
            Ok(Expr::Unary {
                operator,
                right: Box::new(right),
            })
        } else {
            self.postfix()
        }
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.primary()?;

        loop {
            if self.matches(&[TokenType::LeftParen]) {
                expr = self.call(expr)?;
            } else if self.matches(&[TokenType::LeftBracket]) {
                expr = self.index_access(expr)?;
            } else if self.matches(&[TokenType::Dot]) {
                expr = self.field_access(expr)?;
            } else if self.matches(&[TokenType::AtEqual]) {
                expr = self.time_travel_absolute(expr)?;
            } else if self.matches(&[TokenType::AtMinus]) {
                expr = self.time_travel_relative(expr)?;
            } else if self.matches(&[TokenType::PlusPlus, TokenType::MinusMinus]) {
                let operator_token = self.previous().clone();

                let one_expr = Expr::Literal(Literal::Number(1.0));

                let intermediate_operator_type = match operator_token.token_type {
                    TokenType::PlusPlus => TokenType::Plus,
                    TokenType::MinusMinus => TokenType::Minus,
                    _ => unreachable!(),
                };

                let intermediate_operator = Token {
                    token_type: intermediate_operator_type,
                    lexeme: operator_token.lexeme.clone(),
                    literal: None,
                    line: operator_token.line,
                };

                let right_side = Expr::Binary {
                    left: Box::new(expr.clone()),
                    operator: intermediate_operator,
                    right: Box::new(one_expr),
                };

                expr = match expr {
                    Expr::Variable(name) => Expr::Assign {
                        name,
                        value: Box::new(right_side),
                    },
                    Expr::IndexAccess { target, index } => Expr::AssignIndex {
                        target,
                        index,
                        value: Box::new(right_side),
                    },
                    Expr::TimeTravelAbsolute { target, index } => Expr::AssignTimeTravelAbsolute {
                        target,
                        index,
                        value: Box::new(right_side),
                    },
                    Expr::TimeTravelRelative { target, offset } => Expr::AssignTimeTravelRelative {
                        target,
                        offset,
                        value: Box::new(right_side),
                    },
                    _ => {
                        return Err(self.error(&operator_token, "Invalid target for '++' or '--'."))
                    }
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn call(&mut self, callee: Expr) -> Result<Expr, ParseError> {
        let mut arguments = Vec::new();

        if !self.check(&TokenType::RightParen) {
            loop {
                arguments.push(self.expression()?);
                if !self.matches(&[TokenType::Comma]) {
                    break;
                }
            }
        }

        let paren = self.consume(TokenType::RightParen, "Expect ')' after arguments.")?.clone();

        Ok(Expr::Call {
            callee: Box::new(callee),
            paren,
            arguments,
        })
    }

    fn index_access(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let index = self.expression()?;
        self.consume(TokenType::RightBracket, "Expect ']' after index.")?;

        Ok(Expr::IndexAccess { 
            target: Box::new(target), 
            index: Box::new(index),
        })
    }

    fn field_access(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let field = self.consume(TokenType::Identifier, "Expect field name after '.'")?.clone();

        Ok(Expr::FieldAccess { 
            target: Box::new(target), 
            field,
        })
    }

    fn time_travel_absolute(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let index_token = self.consume(TokenType::Number, "Expect number after '@=' in time travel expression.")?;
        let index_literal = index_token.literal.clone().unwrap();

        Ok(Expr::TimeTravelAbsolute { 
            target: Box::new(target), 
            index: Box::new(Expr::Literal(index_literal)), 
        })
    }

    fn time_travel_relative(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let offset_token = self.consume(TokenType::Number, "Expect number after '@-' in time travel expression.")?;
        let offset_literal = offset_token.literal.clone().unwrap();

        Ok(Expr::TimeTravelRelative { 
            target: Box::new(target), 
            offset: Box::new(Expr::Literal(offset_literal)), 
        })
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        if self.matches(&[TokenType::Number, TokenType::String]) {
            Ok(Expr::Literal(self.previous().literal.clone().unwrap()))

        } else if self.matches(&[TokenType::Identifier]) {
            Ok(Expr::Variable(self.previous().clone()))

        } else if self.matches(&[TokenType::LeftBracket]) {
            let left_bracket = self.previous().clone();
            let mut elements = Vec::new();

            if !self.check(&TokenType::RightBracket) {
                loop {
                    elements.push(self.expression()?);
                    if !self.matches(&[TokenType::Comma]) {
                        break;
                    }
                }
            }

            self.consume(TokenType::RightBracket, "Expect ']' after array literal")?;
            
            Ok(Expr::ArrayLiteral { elements, left_bracket })

        } else if self.matches(&[TokenType::LeftBrace]) {
            let left_brace = self.previous().clone();
            let mut elements = Vec::new();

            if !self.check(&TokenType::RightBrace) {
                loop {
                    let key = self.expression()?;
                    self.consume(TokenType::Colon, "Expect ':' after map key.")?;

                    let value = self.expression()?;
                    elements.push((key, value));

                    if !self.matches(&[TokenType::Comma]) {
                        break;
                    }
                }
            }

            self.consume(TokenType::RightBrace, "Expect '}' after map literal.")?;

            Ok(Expr::MapLiteral {
                elements,
                left_brace,
            })

        } else if self.matches(&[TokenType::LeftParen]) {
            if self.check(&TokenType::Identifier) && self.peek_next().token_type == TokenType::Colon {

                let mut parameters = Vec::new();

                loop {
                    let parameter_name = self.consume(TokenType::Identifier, "Expect parameter name in lambda.")?.clone();
                    self.consume(TokenType::Colon, "Expect ':' after parameter name in lambda.")?;
                    let parameter_type = self.type_expr()?;
                    parameters.push((parameter_name, parameter_type));

                    if !self.matches(&[TokenType::Comma]) {
                        break;
                    }
                }

                self.consume(TokenType::RightParen, "Expect ')' after lambda parameters.")?;

                let return_type = if self.matches(&[TokenType::MinusGreater]) {
                    Some(self.type_expr()?)
                } else {
                    None
                };

                self.consume(TokenType::LeftBrace, "Expect '{' before lambda body.")?;
                let body = self.block_statement()?;

                Ok(Expr::Lambda {
                    parameters,
                    return_type,
                    body,
                })
            } else {
                let expr = self.expression()?;
                self.consume(TokenType::RightParen, "Expect ')' after expression.")?;
                Ok(Expr::Grouping(Box::new(expr)))
            }
        } else {
            Err(self.error(self.peek(), "Expect expression."))
        }
    }

    fn type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        if self.matches(&[TokenType::LeftParen]) {
            let mut parameter_types = Vec::new();

            if !self.check(&TokenType::RightParen) {
                loop {
                    parameter_types.push(self.type_expr()?);
                    if !self.matches(&[TokenType::Comma]) {
                        break;
                    }
                }
            }

            self.consume(TokenType::RightParen, "Expect ')' after function parameter types.")?;
            self.consume(TokenType::MinusGreater, "Expect '->' after function parameter types.")?;
            let return_type = self.type_expr()?;

            return Ok(TypeExpr::Function {
                parameter_types,
                return_type: Box::new(return_type),
            });
        }

        let name = self.consume(TokenType::Identifier, "Expect type name.")?.clone();

        let mut type_args = Vec::new();
        if self.matches(&[TokenType::Less]) {
            loop {
                type_args.push(self.type_expr()?);
                if !self.matches(&[TokenType::Comma]) {
                    break;
                }
            }
            self.consume(TokenType::Greater, "Expect '>' after type arguments.")?;
        }

        let is_optional = self.matches(&[TokenType::Question]);

        Ok(TypeExpr::Named {
            name,
            type_args,
            is_optional,
        })
    }

    fn type_parameters(&mut self) -> Result<Vec<Token>, ParseError> {
        let mut type_parameters = Vec::new();

        if self.matches(&[TokenType::Less]) {
            loop {
                type_parameters.push(self.consume(TokenType::Identifier, "Expect type parameter name.")?.clone());
                if !self.matches(&[TokenType::Comma]) {
                    break;
                }
            }
            self.consume(TokenType::Greater, "Expect '>' after type parameters.")?;
        }

        Ok(type_parameters)
    }

    fn consume(&mut self, token_type: TokenType, message: &str) -> Result<&Token, ParseError> {
        if self.check(&token_type) {
            Ok(self.advance())
        } else {
            Err(self.error(self.peek(), message))
        }
    }

    fn matches(&mut self, types: &[TokenType]) -> bool {
        for t in types {
            if self.check(t) {
                self.advance();
                return true;
            }
        }

        false
    }

    fn check(&self, token_type: &TokenType) -> bool {
        if self.is_at_end() {
            return false;
        }

        &self.peek().token_type == token_type
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }

        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.peek().token_type == TokenType::Eof
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn peek_next(&self) -> &Token {
        if self.current + 1 >= self.tokens.len() {
            &self.tokens[self.current]
        } else {
            &self.tokens[self.current + 1]
        }
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn error(&self, token: &Token, message: &str) -> ParseError {
        error_token(token, message);
        ParseError
    }

    fn synchronize(&mut self) {
        self.advance();

        while !self.is_at_end() {
            if self.previous().token_type == TokenType::Semicolon {
                return;
            }

            match self.peek().token_type {
                TokenType::Var
                | TokenType::Enum
                | TokenType::Struct
                | TokenType::Fn
                | TokenType::If
                | TokenType::For
                | TokenType::While
                | TokenType::Match
                | TokenType::Break
                | TokenType::Continue => return,
                _ => {}
            }

            self.advance();
        }
    }
}