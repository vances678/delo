use std::collections::HashMap;
use std::mem;

use crate::error::error;
use crate::token::{Literal, Token, TokenType};

pub struct Scanner {
    source: String,
    tokens: Vec<Token>,
    start: usize,
    current: usize,
    line: u32,
    keywords: HashMap<String, TokenType>,
}

impl Scanner {
    pub fn new(source: String) -> Scanner {
        let mut keywords = HashMap::new();
        keywords.insert("var".to_string(), TokenType::Var);
        keywords.insert("enum".to_string(), TokenType::Enum);
        keywords.insert("struct".to_string(), TokenType::Struct);
        keywords.insert("fn".to_string(), TokenType::Fn);
        keywords.insert("if".to_string(), TokenType::If);
        keywords.insert("else".to_string(), TokenType::Else);
        keywords.insert("match".to_string(), TokenType::Match);
        keywords.insert("for".to_string(), TokenType::For);
        keywords.insert("while".to_string(), TokenType::While);
        keywords.insert("break".to_string(), TokenType::Break);
        keywords.insert("continue".to_string(), TokenType::Continue);
        keywords.insert("in".to_string(), TokenType::In);

        Scanner {
            source,
            tokens: Vec::new(),
            start: 0,
            current: 0,
            line: 1,
            keywords,
        }
    }

    pub fn scan_tokens(&mut self) -> Vec<Token> {
        while !self.is_at_end() {
            self.start = self.current;
            self.scan_token();
        }

        self.tokens.push(Token {
            token_type: TokenType::Eof,
            lexeme: String::new(),
            literal: None,
            line: self.line,
        });

        mem::take(&mut self.tokens)
    }

    fn scan_token(&mut self) {
        let c = self.advance();
        match c {
            '(' => self.add_token(TokenType::LeftParen),
            ')' => self.add_token(TokenType::RightParen),
            '{' => self.add_token(TokenType::LeftBrace),
            '}' => self.add_token(TokenType::RightBrace),
            '[' => self.add_token(TokenType::LeftBracket),
            ']' => self.add_token(TokenType::RightBracket),
            ',' => self.add_token(TokenType::Comma),
            ':' => self.add_token(TokenType::Colon),
            ';' => self.add_token(TokenType::Semicolon),
            '_' => self.add_token(TokenType::Underscore),
            '@' => if self.match_char('=') { self.add_token(TokenType::AtEqual) } else if self.match_char('-') { self.add_token(TokenType::AtMinus) },
            '&' => if self.match_char('&') { self.add_token(TokenType::AmpersandAmpersand) },
            '|' => if self.match_char('|') { self.add_token(TokenType::PipePipe) },
            '!' => if self.match_char('=') { self.add_token(TokenType::ExclamationEqual) } else { self.add_token(TokenType::Exclamation) },
            '=' => {
                if self.match_char('=') { 
                    self.add_token(TokenType::EqualEqual) 
                } else if self.match_char('>') { 
                    self.add_token(TokenType::EqualGreater) 
                } else { 
                    self.add_token(TokenType::Equal) 
                }
            },
            '>' => if self.match_char('=') { self.add_token(TokenType::GreaterEqual) } else { self.add_token(TokenType::Greater) },
            '<' => if self.match_char('=') { self.add_token(TokenType::LessEqual) } else { self.add_token(TokenType::Less) },
            '+' => {
                if self.match_char('=') { 
                    self.add_token(TokenType::PlusEqual) 
                } else if self.match_char('+') { 
                    self.add_token(TokenType::PlusPlus) 
                } else { 
                    self.add_token(TokenType::Plus) 
                }
            },
            '-' => {
                if self.match_char('=') { 
                    self.add_token(TokenType::MinusEqual) 
                } else if self.match_char('>') { 
                    self.add_token(TokenType::MinusGreater) 
                } else if self.match_char('-') { 
                    self.add_token(TokenType::MinusMinus) 
                } else { 
                    self.add_token(TokenType::Minus) 
                }
            },
            '*' => if self.match_char('=') { self.add_token(TokenType::StarEqual) } else { self.add_token(TokenType::Star) },
            '/' => {
                if self.match_char('/') {
                    while self.peek() != '\n' && !self.is_at_end() {
                        self.advance();
                    }
                } else if self.match_char('*') {
                    while !self.is_at_end() {
                        if self.peek() == '\n' {
                            self.line += 1;
                            self.advance();
                        } else if self.peek() == '*' && self.peek_next() == '/' {
                            self.advance();
                            self.advance();
                            return;
                        } else {
                            self.advance();
                        }
                    }
                    error(self.line, "Unterminated block comment.");
                } else if self.match_char('=') {
                    self.add_token(TokenType::SlashEqual);
                } else {
                    self.add_token(TokenType::Slash);
                }
            },
            '%' => if self.match_char('=') { self.add_token(TokenType::PercentEqual) } else { self.add_token(TokenType::Percent) },
            '^' => if self.match_char('=') { self.add_token(TokenType::CaretEqual) } else { self.add_token(TokenType::Caret) },
            '?' => if self.match_char('?') { self.add_token(TokenType::QuestionQuestion) } else { self.add_token(TokenType::Question) },
            '.' => {
                if self.match_char('.') {
                    if self.match_char('=') {
                        self.add_token(TokenType::DotDotEqual);
                    } else {
                        self.add_token(TokenType::DotDot);
                    }
                } else {
                    self.add_token(TokenType::Dot);
                }
            },
            ' ' | '\r' | '\t' => {}
            '\n' => self.line += 1,
            '"' => self.string(),
            _ => {
                if Self::is_digit(c) {
                    self.number();
                } else if Self::is_alpha(c) {
                    self.identifier();
                } else {
                    error(self.line, "Unexpected character.");
                }
            }
        }
    }

    fn string(&mut self) {
        while self.peek() != '"' && !self.is_at_end() {
            if self.peek() == '\n' {
                self.line += 1;
            }
            self.advance();
        }

        if self.is_at_end() {
            error(self.line, "Unterminated string.");
            return;
        }

        self.advance();

        let value = self.source[(self.start + 1)..(self.current - 1)].to_string();
        self.add_token_with_literal(TokenType::String, Some(Literal::String(value)));
    }

    fn number(&mut self) {
        while Self::is_digit(self.peek()) {
            self.advance();
        }

        if self.peek() == '.' && Self::is_digit(self.peek_next()) {
            self.advance();
            while Self::is_digit(self.peek()) {
                self.advance();
            }
        }

        let value = self.source[(self.start)..(self.current)].to_string();
        self.add_token_with_literal(TokenType::Number, Some(Literal::Number(value.parse().unwrap())));
    }

    fn identifier(&mut self) {
        while Self::is_alpha_numeric(self.peek()) {
            self.advance();
        }
        let text = self.source[(self.start)..(self.current)].to_string();
        let token_type = self.keywords.get(&text).cloned().unwrap_or(TokenType::Identifier);
        self.add_token(token_type);
    }

    fn add_token(&mut self, token_type: TokenType) {
        self.add_token_with_literal(token_type, None);
    }

    fn add_token_with_literal(&mut self, token_type: TokenType, literal: Option<Literal>) {
        let text = self.source[self.start..self.current].to_string();
        self.tokens.push(Token {
            token_type,
            lexeme: text,
            literal,
            line: self.line,
        });
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() {
            return false;
        }

        let next_char = self.source[self.current..].chars().next().unwrap();
        if next_char != expected {
            return false;
        }

        self.current += next_char.len_utf8();
        true
    }

    fn advance(&mut self) -> char {
        let c = self.source[self.current..].chars().next().unwrap();
        self.current += c.len_utf8();
        c
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            return '\0';
        }

        self.source[self.current..].chars().next().unwrap()
    }

    fn peek_next(&self) -> char {
        if self.current + 1 >= self.source.len() {
            return '\0';
        }

        self.source[(self.current + 1)..].chars().next().unwrap()
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    fn is_alpha_numeric(c: char) -> bool {
        Self::is_alpha(c) || Self::is_digit(c)
    }

    fn is_alpha(c: char) -> bool {
        (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_'
    }

    fn is_digit(c: char) -> bool {
        c >= '0' && c <= '9'
    }
}