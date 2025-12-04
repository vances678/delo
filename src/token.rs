use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // One-character tokens
    LeftParen, RightParen, LeftBrace, RightBrace, LeftBracket, RightBracket,
    Comma, Colon, Semicolon, Underscore,

    // Two-character tokens
    AtEqual, AtMinus, AmpersandAmpersand, PipePipe,
    
    // One or two character tokens
    Exclamation, ExclamationEqual,
    Equal, EqualEqual, EqualGreater,
    Greater, GreaterEqual,
    Less, LessEqual,
    Plus, PlusEqual, PlusPlus,
    Minus, MinusEqual, MinusGreater, MinusMinus,
    Star, StarEqual,
    Slash, SlashEqual,
    Percent, PercentEqual,
    Caret, CaretEqual,
    Question, QuestionQuestion,

    // One, two, or three character tokens
    Dot, DotDot, DotDotEqual,
    
    // Literals
    Identifier, String, Number,
    
    // Keywords
    Var, Enum, Struct, Fn, If, Else, Match, For, While, Break, Continue, In,
    
    // End of file
    Eof,
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(f64),
    String(String),
}

#[derive(Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub lexeme: String,
    pub literal: Option<Literal>,
    pub line: u32,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} {} {:?}", self.token_type, self.lexeme, self.literal)
    }
}
