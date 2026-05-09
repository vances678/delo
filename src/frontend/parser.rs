use std::rc::Rc;
use std::mem;

use crate::error::ParseError;
use crate::ast::token::{Literal, Token, TokenType};
use crate::ast::expr::{Expr, MatchCase, Pattern};
use crate::ast::stmt::{EnumVariant, FunctionStmt, Stmt, VariableBinding};
use crate::type_system::types::TypeExpr;

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
    errors: Vec<ParseError>
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            errors: Vec::new(),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Stmt>, Vec<ParseError>> {
        let mut statements = Vec::new();
        
        while !self.is_at_end() {
            match self.declaration() {
                Ok(Some(stmt)) => statements.push(stmt),
                Ok(None) => {},
                Err(error) => self.errors.push(error),
            }
        }

        if self.errors.is_empty() {
            Ok(statements)
        } else {
            Err(mem::take(&mut self.errors))
        }
    }

     fn declaration(&mut self) -> Result<Option<Stmt>, ParseError> {
        let result = if self.matches(&[TokenType::Var]) {
            self.variable_declaration()
        } else if self.matches(&[TokenType::Enum]) {
            self.enum_declaration()
        } else if self.matches(&[TokenType::Struct]) {
            self.struct_declaration()
        } else if self.matches(&[TokenType::Fn]) {
            self.function_declaration()
        } else {
            self.statement()
        };

        match result {
            Ok(stmt) => Ok(Some(stmt)),
            Err(error) => {
                self.synchronize();
                Err(error)
            }
        }
    }

    fn variable_declaration(&mut self) -> Result<Stmt, ParseError> {
        let binding = self.variable_binding()?;

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

        Ok(Stmt::Variable {
            binding,
            type_annotation,
            initializer,
        })
    }

    fn variable_binding(&mut self) -> Result<VariableBinding, ParseError> {
        if self.matches(&[TokenType::LeftParen]) {
            let left_paren = self.previous().clone();

            if self.check(&TokenType::RightParen) {
                let token = self.peek().clone();
                return Err(ParseError::EmptyTuple { line: token.line, column: token.column });
            }

            let mut elements = vec![self.variable_binding()?];
            let mut had_trailing_comma = false;

            while self.matches(&[TokenType::Comma]) {
                if self.check(&TokenType::RightParen) {
                    had_trailing_comma = true;
                    break;
                }
                elements.push(self.variable_binding()?);
            }

            self.consume(TokenType::RightParen)?;

            if elements.len() == 1 && !had_trailing_comma {
                return Ok(elements.into_iter().next().unwrap());
            }

            Ok(VariableBinding::Tuple { elements, left_paren })
        } else {
            let identifier = self.consume(TokenType::Identifier)?.clone();
            Ok(VariableBinding::Identifier(identifier))
        }
    }

    fn enum_declaration(&mut self) -> Result<Stmt, ParseError> {
        let identifier = self.consume(TokenType::Identifier)?.clone();

        let type_parameters = self.type_parameters()?;

        self.consume(TokenType::LeftBrace)?;
        let mut variants = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            let variant_identifier = self.consume(TokenType::Identifier)?.clone();

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
                self.consume(TokenType::RightParen)?;
            }

            variants.push(EnumVariant {
                identifier: variant_identifier,
                payload_types,
            });

            if !(self.matches(&[TokenType::Comma]) || self.check(&TokenType::RightBrace)) {
                let found = self.peek().clone();
                return Err(ParseError::ExpectedToken { line: 
                    found.line, 
                    column: found.column, 
                    expected: vec![TokenType::Comma, TokenType::RightBrace], 
                    found 
                });
            }
        }

        self.consume(TokenType::RightBrace)?;

        Ok(Stmt::Enum {
            identifier,
            type_parameters,
            variants,
        })
    }

    fn struct_declaration(&mut self) -> Result<Stmt, ParseError> {
        let identifier = self.consume(TokenType::Identifier)?.clone();

        let type_parameters = self.type_parameters()?;

        self.consume(TokenType::LeftBrace)?;
        let mut fields = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            let field_name = self.consume(TokenType::Identifier)?.clone();
            self.consume(TokenType::Colon)?;

            let field_type = self.type_expr()?;
            fields.push((field_name, field_type));
            
            if !(self.matches(&[TokenType::Comma]) || self.check(&TokenType::RightBrace)) {
                let found = self.peek().clone();
                return Err(ParseError::ExpectedToken { line: 
                    found.line, 
                    column: found.column, 
                    expected: vec![TokenType::Comma, TokenType::RightBrace], 
                    found 
                });
            }
        }

        self.consume(TokenType::RightBrace)?;

        Ok(Stmt::Struct {
            identifier,
            type_parameters,
            fields,
        })
    }

    fn function_declaration(&mut self) -> Result<Stmt, ParseError> {
        let identifier = self.consume(TokenType::Identifier)?.clone();

        let type_parameters = self.type_parameters()?;

        self.consume(TokenType::LeftParen)?;
        let mut parameters: Vec<(Token, TypeExpr)> = Vec::new();
        if !self.check(&TokenType::RightParen) {
            loop {
                let parameter_identifier = self.consume(TokenType::Identifier)?.clone();
                self.consume(TokenType::Colon)?;
                let parameter_type = self.type_expr()?;

                if parameters.iter().any(|(existing_identifier, _)| existing_identifier.lexeme == parameter_identifier.lexeme) {
                    return Err(ParseError::DuplicateParameter {
                        line:   parameter_identifier.line,
                        column: parameter_identifier.column,
                        parameter:  parameter_identifier.clone(),
                    });
                }

                parameters.push((parameter_identifier, parameter_type));
                if !self.matches(&[TokenType::Comma]) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen)?;

        let return_type = if self.matches(&[TokenType::MinusGreater]) {
            Some(self.type_expr()?)
        } else {
            None
        };

        self.consume(TokenType::LeftBrace)?;
        let body = self.block_statement()?;

        Ok(Stmt::Function(Rc::new(FunctionStmt { 
            identifier,
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
        let if_token = self.previous().clone();

        self.consume(TokenType::LeftParen)?;
        let condition = self.expression()?;
        self.consume(TokenType::RightParen)?;

        let then_branch = self.statement()?;

        let else_branch = if self.matches(&[TokenType::Else]) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };

        Ok(Stmt::If { 
            if_token,
            condition, 
            then_branch: Box::new(then_branch), 
            else_branch,
        })
    }

    fn for_statement(&mut self) -> Result<Stmt, ParseError> {
        let for_token = self.previous().clone();
        self.consume(TokenType::LeftParen)?;

        // for-in form: for (binding in iterable) { ... } where binding is ident or tuple
        if (self.check(&TokenType::Identifier) && self.peek_next().token_type == TokenType::In)
            || self.check(&TokenType::LeftParen)
        {
            let saved = self.current;
            let binding = self.variable_binding()?;
            if self.check(&TokenType::In) {
                self.advance();
                let iterable = self.expression()?;
                self.consume(TokenType::RightParen)?;
                let body = self.statement()?;

                return Ok(Stmt::ForIn {
                    binding,
                    iterable,
                    body: Box::new(body),
                });
            }
            self.current = saved;
        }

        // standard form: for (initializer; condition; increment) { ... } 
        let initializer = if self.check(&TokenType::Semicolon) {
            None
        } else if self.matches(&[TokenType::Var]) {
            Some(self.variable_declaration()?)
        } else {
            Some(self.expression_statement()?)
        };
        self.consume(TokenType::Semicolon)?;

        let condition = if !self.check(&TokenType::Semicolon) {
            Some(self.expression()?)
        } else {
            None
        };
        self.consume(TokenType::Semicolon)?;

        let increment = if !self.check(&TokenType::RightParen) {
            Some(self.expression()?)
        } else {
            None
        };
        self.consume(TokenType::RightParen)?;


        let mut body = self.statement()?;

        if let Some(incr) = increment {
            body = Stmt::Block(vec![body, Stmt::Expression(incr)]);
        }

        let current_token = &self.tokens[self.current];
        let condition = condition.unwrap_or(Expr::Variable(Token { 
            token_type: TokenType::Identifier, 
            lexeme: "True".to_string(),
            literal: None, 
            line: current_token.line, 
            column: current_token.column 
        }));

        let mut while_stmt = Stmt::While {
            while_token: for_token,
            condition,
            body: Box::new(body),
        };

        if let Some(init) = initializer {
            while_stmt = Stmt::Block(vec![init, while_stmt]);
        }

        Ok(while_stmt)
    }

    fn while_statement(&mut self) -> Result<Stmt, ParseError> {
        let while_token = self.previous().clone();
        
        self.consume(TokenType::LeftParen)?;
        let condition = self.expression()?;
        self.consume(TokenType::RightParen)?;
        let body = self.statement()?;

        Ok(Stmt::While {
            while_token, 
            condition, 
            body: Box::new(body) 
        })
    }

    fn block_statement(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut statements = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            match self.declaration()? {
                Some(stmt) => statements.push(stmt),
                None => {}
            }
        }

        self.consume(TokenType::RightBrace)?;
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
        let if_token = self.previous().clone();

        self.consume(TokenType::LeftParen)?;
        let condition = self.expression()?;
        self.consume(TokenType::RightParen)?;

        let then_branch = self.block_expression()?;
        self.consume(TokenType::Else)?;

        let else_branch = if self.matches(&[TokenType::If]) {
            self.if_expression()?
        } else {
            self.block_expression()?
        };

        Ok(Expr::If {
            if_token,
            condition: Box::new(condition), 
            then_branch: Box::new(then_branch), 
            else_branch: Some(Box::new(else_branch)) 
        })
    }

    fn block_expression(&mut self) -> Result<Expr, ParseError> {
        let left_brace = self.consume(TokenType::LeftBrace)?.clone();
        let mut statements = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            match self.declaration()? {
                Some(stmt) => statements.push(stmt),
                None => {}
            }
        }

        self.consume(TokenType::RightBrace)?;

        Ok(Expr::Block {
            stmts: statements,
            left_brace,
        })
    }

    fn match_expression(&mut self) -> Result<Expr, ParseError> {
        let match_token = self.previous().clone();
        let subject = self.expression()?;

        self.consume(TokenType::LeftBrace)?;

        let mut cases = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            let pattern = self.pattern()?;

            let (guard_token, guard) = if self.matches(&[TokenType::If]) {
                let if_token = self.previous().clone();
                let condition = self.expression()?;
                (Some(if_token), Some(condition))
            } else {
                (None, None)
            };

            self.consume(TokenType::EqualGreater)?;
            let body = self.expression()?;

            cases.push(MatchCase {
                pattern,
                guard_token,
                guard,
                body,
            });

            if !self.matches(&[TokenType::Comma]) && !self.check(&TokenType::RightBrace) {
                let found = self.peek().clone();
                return Err(ParseError::ExpectedToken { 
                    line: found.line, 
                    column: found.column, 
                    expected: vec![TokenType::Comma, TokenType::RightBrace], 
                    found 
                });
            }
        }

        self.consume(TokenType::RightBrace)?;

        Ok(Expr::Match {
            match_token,
            subject: Box::new(subject),
            cases,
        })
    }

    fn pattern(&mut self) -> Result<Pattern, ParseError> {
        if self.matches(&[TokenType::Underscore]) {
            return Ok(Pattern::Wildcard(self.previous().clone()));
        }

        if self.matches(&[TokenType::Number, TokenType::String]) {
            let start = self.previous().clone();
            let start_type = start.token_type.clone();

            if self.matches(&[TokenType::DotDot, TokenType::DotDotEqual]) {
                let is_inclusive = self.previous().token_type == TokenType::DotDotEqual;

                let end = self.peek().clone();
                match end.token_type {
                    TokenType::Number | TokenType::String => {
                        if end.token_type != start_type {
                            return Err(ParseError::MismatchedRangeBounds { 
                                line: end.line, 
                                column: end.column, 
                                start_bound: start, 
                                end_bound: end 
                            });
                        }
                        self.advance();

                        return Ok(Pattern::Range {
                            start,
                            end,
                            is_inclusive,
                        });
                    }
                    _ => {
                        return Err(ParseError::MissingRangeEnd { 
                            line: end.line, 
                            column: end.column, 
                            start_bound: start 
                        });
                    }
                }
            }

            return Ok(Pattern::Literal(start));
        }

        if self.matches(&[TokenType::Identifier]) {
            let identifier = self.previous().clone();

            if self.matches(&[TokenType::LeftParen]) {
                let mut arguments = Vec::new();
                if !self.check(&TokenType::RightParen) {
                    loop {
                        arguments.push(self.pattern()?);
                        if !self.matches(&[TokenType::Comma]) {
                            break;
                        }
                    }
                }
                self.consume(TokenType::RightParen)?;

                return Ok(Pattern::EnumVariant { identifier, arguments });
            }

            return Ok(Pattern::Variable(identifier)); // TODO: determine if 0-ary EnumVariant with enum variant table
        }

        if self.matches(&[TokenType::LeftParen]) {
            let left_paren = self.previous().clone();

            if self.check(&TokenType::RightParen) {
                let token = self.peek().clone();
                return Err(ParseError::EmptyTuple { line: token.line, column: token.column });
            }

            let mut elements = vec![self.pattern()?];
            let mut had_trailing_comma = false;

            while self.matches(&[TokenType::Comma]) {
                if self.check(&TokenType::RightParen) {
                    had_trailing_comma = true;
                    break;
                }
                elements.push(self.pattern()?);
            }

            self.consume(TokenType::RightParen)?;

            if elements.len() == 1 && !had_trailing_comma {
                return Ok(elements.into_iter().next().unwrap());
            }

            return Ok(Pattern::Tuple { elements, left_paren });
        }

        let found = self.peek().clone();
        Err(ParseError::InvalidPattern {
            line: found.line,
            column: found.column,
            found,
        })
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
                    column: operator_token.column,
                };
                
                Expr::Binary {
                    left: Box::new(expr.clone()),
                    operator: intermediate_operator,
                    right: Box::new(right_side),
                }
            };

            return match expr { 
                Expr::Variable(identifier) => Ok(Expr::Assign { 
                    identifier, 
                    value: Box::new(value) 
                }),
                Expr::IndexAccess { target, index, left_bracket } => Ok(Expr::AssignIndex { 
                    target, 
                    index, 
                    value: Box::new(value) ,
                    left_bracket,
                }),
                Expr::TimeTravelAbsolute { target, index, at_token } => Ok(Expr::AssignTimeTravelAbsolute { 
                    target, 
                    index, 
                    value: Box::new(value),
                    at_token
                }),
                Expr::TimeTravelRelative { target, offset, at_token } => Ok(Expr::AssignTimeTravelRelative { 
                    target, 
                    offset, 
                    value: Box::new(value),
                    at_token
                }),
                _ => Err(ParseError::InvalidAssignmentTarget { 
                    line: operator_token.line, 
                    column: operator_token.column, 
                    found: operator_token.clone() 
                })
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
                range_token: self.previous().clone(),
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

                let one_expr = Expr::Literal(Token {
                    token_type: TokenType::Number,
                    lexeme: "1".to_string(),
                    literal: Some(Literal::Number(1.0)),
                    line: operator_token.line,
                    column: operator_token.column,
                });

                let (intermediate_operator_type, intermediate_operator_lexeme) = match operator_token.token_type {
                    TokenType::PlusPlus => (TokenType::Plus, "+"),
                    TokenType::MinusMinus => (TokenType::Minus, "-"),
                    _ => unreachable!(),
                };

                let intermediate_operator = Token {
                    token_type: intermediate_operator_type,
                    lexeme: intermediate_operator_lexeme.to_string(),
                    literal: None,
                    line: operator_token.line,
                    column: operator_token.column,
                };

                let right_side = Expr::Binary {
                    left: Box::new(expr.clone()),
                    operator: intermediate_operator,
                    right: Box::new(one_expr),
                };

                expr = match expr {
                    Expr::Variable(identifier) => Expr::Assign {
                        identifier,
                        value: Box::new(right_side),
                    },
                    Expr::IndexAccess { target, index, left_bracket } => Expr::AssignIndex {
                        target,
                        index,
                        value: Box::new(right_side),
                        left_bracket,
                    },
                    Expr::TimeTravelAbsolute { target, index, at_token } => Expr::AssignTimeTravelAbsolute {
                        target,
                        index,
                        value: Box::new(right_side),
                        at_token,
                    },
                    Expr::TimeTravelRelative { target, offset, at_token } => Expr::AssignTimeTravelRelative {
                        target,
                        offset,
                        value: Box::new(right_side),
                        at_token,
                    },
                    _ => {
                        return Err(ParseError::InvalidAssignmentTarget {
                            line: operator_token.line,
                            column: operator_token.column,
                            found: operator_token.clone(),
                        })
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

        let paren = self.consume(TokenType::RightParen)?.clone();

        Ok(Expr::Call {
            callee: Box::new(callee),
            paren,
            arguments,
        })
    }

    fn index_access(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let left_bracket = self.previous().clone();
        let index = self.expression()?;
        self.consume(TokenType::RightBracket)?;

        Ok(Expr::IndexAccess { 
            target: Box::new(target), 
            index: Box::new(index),
            left_bracket,
        })
    }

    fn field_access(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let field = if self.matches(&[TokenType::Number]) {
            self.previous().clone()
        } else {
            self.consume(TokenType::Identifier)?.clone()
        };

        if field.token_type == TokenType::Number && field.lexeme.contains('.') {
            let mut current = target;
            for part in field.lexeme.split('.') {
                let part_token = Token {
                    token_type: TokenType::Number,
                    lexeme: part.to_string(),
                    literal: None,
                    line: field.line,
                    column: field.column,
                };
                current = Expr::FieldAccess { target: Box::new(current), field: part_token };
            }
            return Ok(current);
        }

        Ok(Expr::FieldAccess {
            target: Box::new(target),
            field,
        })
    }

    fn time_travel_absolute(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let at_token = self.previous().clone();
        let index = self.consume(TokenType::Number)?.clone();

        Ok(Expr::TimeTravelAbsolute { 
            target: Box::new(target), 
            index: Box::new(Expr::Literal(index)),
            at_token,
        })
    }

    fn time_travel_relative(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let at_token = self.previous().clone();
        let offset = self.consume(TokenType::Number)?.clone();

        Ok(Expr::TimeTravelRelative { 
            target: Box::new(target), 
            offset: Box::new(Expr::Literal(offset)), 
            at_token,
        })
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let previous_token = self.previous().clone();
        if self.matches(&[TokenType::Number, TokenType::String]) {
            Ok(Expr::Literal(self.previous().clone()))

        } else if self.matches(&[TokenType::Identifier]) {
            if self.check(&TokenType::LeftBrace) && previous_token.token_type != TokenType::Match {
                let identifier = self.previous().clone();
                self.consume(TokenType::LeftBrace)?;
                let mut fields = Vec::new();

                if !self.check(&TokenType::RightBrace) {
                    loop {
                        let field_identifier = self.consume(TokenType::Identifier)?.clone();
                        self.consume(TokenType::Colon)?;
                        let field_value = self.expression()?;
                        fields.push((field_identifier, field_value));

                        if !self.matches(&[TokenType::Comma]) {
                            break;
                        }
                    }
                }

                self.consume(TokenType::RightBrace)?;

                Ok(Expr::StructInstantiation {
                    identifier,
                    fields,
                })
            } else {
                Ok(Expr::Variable(self.previous().clone()))
            }

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

            self.consume(TokenType::RightBracket)?;
            
            Ok(Expr::ArrayLiteral { elements, left_bracket })

        } else if self.matches(&[TokenType::LeftBrace]) {
            let left_brace = self.previous().clone();
            let mut elements = Vec::new();

            if !self.check(&TokenType::RightBrace) {
                loop {
                    let key = self.expression()?;
                    self.consume(TokenType::Colon)?;

                    let value = self.expression()?;
                    elements.push((key, value));

                    if !self.matches(&[TokenType::Comma]) {
                        break;
                    }
                }
            }

            self.consume(TokenType::RightBrace)?;

            Ok(Expr::MapLiteral {
                elements,
                left_brace,
            })

        } else if self.matches(&[TokenType::LeftParen]) {
            let left_paren = self.previous().clone();

            if self.check(&TokenType::Identifier) && self.peek_next().token_type == TokenType::Colon {

                let mut parameters = Vec::new();

                loop {
                    let parameter_name = self.consume(TokenType::Identifier)?.clone();
                    self.consume(TokenType::Colon)?;
                    let parameter_type = self.type_expr()?;
                    parameters.push((parameter_name, parameter_type));

                    if !self.matches(&[TokenType::Comma]) {
                        break;
                    }
                }

                self.consume(TokenType::RightParen)?;

                let return_type = if self.matches(&[TokenType::MinusGreater]) {
                    Some(self.type_expr()?)
                } else {
                    None
                };

                self.consume(TokenType::LeftBrace)?;
                let body = self.block_statement()?;

                Ok(Expr::Lambda {
                    parameters,
                    return_type,
                    body,
                })
            } else if self.check(&TokenType::RightParen) {
                self.advance();
                Ok(Expr::TupleLiteral { elements: Vec::new(), left_paren })
            } else {
                let first = self.expression()?;
                if self.matches(&[TokenType::Comma]) {
                    let mut elements = vec![first];

                    while !self.check(&TokenType::RightParen) {
                        elements.push(self.expression()?);
                        if !self.matches(&[TokenType::Comma]) {
                            break;
                        }
                    }

                    self.consume(TokenType::RightParen)?;
                    Ok(Expr::TupleLiteral { elements, left_paren })
                } else {
                    self.consume(TokenType::RightParen)?;
                    Ok(Expr::Grouping(Box::new(first)))
                }
            }
        } else {
            let found = self.peek().clone();
            Err(ParseError::UnexpectedToken {
                line: found.line,
                column: found.column,
                found,
            })
        }
    }

    fn type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        if self.matches(&[TokenType::LeftParen]) {
            let left_paren = self.previous().clone();

            if self.check(&TokenType::RightParen) {
                self.advance();
                if self.matches(&[TokenType::MinusGreater]) {
                    let return_type = self.type_expr()?;
                    return Ok(TypeExpr::Function {
                        parameter_types: Vec::new(),
                        return_type: Box::new(return_type),
                    });
                }
                return Ok(TypeExpr::Named {
                    identifier: Token {
                        token_type: TokenType::Identifier,
                        lexeme: "Void".to_string(),
                        literal: None,
                        line: left_paren.line,
                        column: left_paren.column,
                    },
                    type_parameters: None,
                    type_arguments: Vec::new(),
                    enum_variants: None,
                    struct_fields: None,
                });
            }

            let mut element_types = vec![self.type_expr()?];
            let mut saw_comma = false;
            let mut had_trailing_comma = false;

            while self.matches(&[TokenType::Comma]) {
                saw_comma = true;
                if self.check(&TokenType::RightParen) {
                    had_trailing_comma = true;
                    break;
                }
                element_types.push(self.type_expr()?);
            }

            self.consume(TokenType::RightParen)?;

            if self.matches(&[TokenType::MinusGreater]) {
                let return_type = self.type_expr()?;
                return Ok(TypeExpr::Function {
                    parameter_types: element_types,
                    return_type: Box::new(return_type),
                });
            }

            if saw_comma {
                if element_types.len() == 1 && !had_trailing_comma {
                    return Ok(element_types.into_iter().next().unwrap());
                }
                return Ok(TypeExpr::Tuple { element_types });
            }

            return Ok(element_types.into_iter().next().unwrap());
        }

        let identifier = self.consume(TokenType::Identifier)?.clone();

        let mut type_arguments = Vec::new();
        if self.matches(&[TokenType::Less]) {
            loop {
                type_arguments.push(self.type_expr()?);
                if !self.matches(&[TokenType::Comma]) {
                    break;
                }
            }
            self.consume(TokenType::Greater)?;
        }

        let inner = TypeExpr::Named {
            identifier,
            type_parameters: None,
            type_arguments,
            enum_variants: None,
            struct_fields: None,
        };

        if self.matches(&[TokenType::Question]) {
            let question_token = self.previous().clone();
            let optional_identifier = Token {
                token_type: TokenType::Identifier,
                lexeme: "Optional".to_string(),
                literal: None,
                line: question_token.line,
                column: question_token.column,
            };
            return Ok(TypeExpr::Named {
                identifier: optional_identifier,
                type_parameters: None,
                type_arguments: vec![inner],
                enum_variants: None,
                struct_fields: None,
            });
        }

        Ok(inner)
    }

    fn type_parameters(&mut self) -> Result<Vec<Token>, ParseError> {
        let mut type_parameters = Vec::new();

        if self.matches(&[TokenType::Less]) {
            loop {
                type_parameters.push(self.consume(TokenType::Identifier)?.clone());
                if !self.matches(&[TokenType::Comma]) {
                    break;
                }
            }
            self.consume(TokenType::Greater)?;
        }

        Ok(type_parameters)
    }

    fn consume(&mut self, token_type: TokenType) -> Result<&Token, ParseError> {
        if self.check(&token_type) {
            Ok(self.advance())
        } else {
            let next = self.peek();
            Err(ParseError::ExpectedToken {
                line: next.line,
                column: next.column,
                expected: vec![token_type],
                found: next.clone(),
            })
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
        if self.current == 0 {
            &self.tokens[0]
        } else {
            &self.tokens[self.current - 1]
        }
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