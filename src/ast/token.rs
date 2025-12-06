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

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // One-character tokens
            Self::LeftParen => write!(f, "("),
            Self::RightParen => write!(f, ")"),
            Self::LeftBrace => write!(f, "{{"),
            Self::RightBrace => write!(f, "}}"),
            Self::LeftBracket => write!(f, "["),
            Self::RightBracket => write!(f, "]"),
            Self::Comma => write!(f, ","),
            Self::Colon => write!(f, ":"),
            Self::Semicolon => write!(f, ";"),
            Self::Underscore => write!(f, "_"),

            // Two-character tokens
            Self::AtEqual => write!(f, "@="),
            Self::AtMinus => write!(f, "@-"),
            Self::AmpersandAmpersand => write!(f, "&&"),
            Self::PipePipe => write!(f, "||"),
            
            // One or two character tokens
            Self::Exclamation => write!(f, "!"),
            Self::ExclamationEqual => write!(f, "!="),
            Self::Equal => write!(f, "="),
            Self::EqualEqual => write!(f, "=="),
            Self::EqualGreater => write!(f, ">="),
            Self::Greater => write!(f, ">"),
            Self::GreaterEqual => write!(f, ">="),
            Self::Less => write!(f, "<"),
            Self::LessEqual => write!(f, "<="),
            Self::Plus => write!(f, "+"),
            Self::PlusEqual => write!(f, "+="),
            Self::PlusPlus => write!(f, "++"),
            Self::Minus => write!(f, "-"),
            Self::MinusEqual => write!(f, "-="),
            Self::MinusGreater => write!(f, "->"),
            Self::MinusMinus => write!(f, "--"),
            Self::Star => write!(f, "*"),
            Self::StarEqual => write!(f, "*="),
            Self::Slash => write!(f, "/"),
            Self::SlashEqual => write!(f, "/="),
            Self::Percent => write!(f, "%"),
            Self::PercentEqual => write!(f, "%="),
            Self::Caret => write!(f, "^"),
            Self::CaretEqual => write!(f, "^="),
            Self::Question => write!(f, "?"),
            Self::QuestionQuestion => write!(f, "??"),

            // One, two, or three character tokens
            Self::Dot => write!(f, "."),
            Self::DotDot => write!(f, ".."),
            Self::DotDotEqual => write!(f, "..="),
            
            // Literals
            Self::Identifier => write!(f, "identifier"),
            Self::String => write!(f, "string"),
            Self::Number => write!(f, "number"),
            
            // Keywords
            Self::Var => write!(f, "var"),
            Self::Enum => write!(f, "enum"),
            Self::Struct => write!(f, "struct"),
            Self::Fn => write!(f, "fn"),
            Self::If => write!(f, "if"),
            Self::Else => write!(f, "else"),
            Self::Match => write!(f, "match"),
            Self::For => write!(f, "for"),
            Self::While => write!(f, "while"),
            Self::Break => write!(f, "break"),
            Self::Continue => write!(f, "continue"),
            Self::In => write!(f, "in"),
            
            // End of file
            Self::Eof => write!(f, "end of file"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(f64),
    String(String),
}

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub lexeme: String,
    pub literal: Option<Literal>,
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} {} {:?}", self.token_type, self.lexeme, self.literal)
    }
}
