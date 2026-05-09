use std::any::{type_name};

use crate::ast::token::{Token, TokenType};
use crate::type_system::types::TypeExpr;

pub trait ErrorMessage {
    fn message(&self) -> String;
}

pub trait ErrorWithLocation: ErrorMessage {
    fn line(&self) -> Option<u32>;
    fn column(&self) -> Option<u32>;
}

#[derive(Debug, Clone)]
pub enum ScanError {
    UnexpectedCharacter { line: u32, column: u32, character: char },
    UnterminatedString { line: u32, column: u32 },
    UnterminatedBlockComment { line: u32, column: u32 },
}

impl ErrorMessage for ScanError {
    fn message(&self) -> String {
        match self {
            Self::UnexpectedCharacter { character, .. } => format!("Unexpected character '{character}'"),
            Self::UnterminatedString { .. } => format!("Unterminated string"),
            Self::UnterminatedBlockComment { .. } => format!("Unterminated block comment"),
        }
    }
}

impl ErrorWithLocation for ScanError {
    fn line(&self) -> Option<u32> {
        match self {
            Self::UnexpectedCharacter { line, .. } => Some(*line),
            Self::UnterminatedString { line, .. } => Some(*line),
            Self::UnterminatedBlockComment { line, .. } => Some(*line),
        }
    }
    fn column(&self) -> Option<u32> {
        match self {
            Self::UnexpectedCharacter { column, .. } => Some(*column),
            Self::UnterminatedString { column, .. } => Some(*column),
            Self::UnterminatedBlockComment { column, .. } => Some(*column),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParseError {
    ExpectedToken {
        line: u32,
        column: u32,
        expected: Vec<TokenType>,
        found: Token,
    },
    UnexpectedToken {
        line: u32,
        column: u32,
        found: Token,
    },
    InvalidKeyword {
        line: u32,
        column: u32,
        found: Token,
    },
    InvalidPattern {
        line: u32,
        column: u32,
        found: Token,
    },
    InvalidAssignmentTarget {
        line: u32,
        column: u32,
        found: Token,
    },
    MismatchedRangeBounds {
        line: u32,
        column: u32,
        start_bound: Token,
        end_bound: Token,
    },
    MissingRangeEnd {
        line: u32,
        column: u32,
        start_bound: Token,
    },
    DuplicateParameter {
        line: u32,
        column: u32,
        parameter: Token,
    },
    EmptyTuple {
        line: u32,
        column: u32,
    },
}

impl ErrorMessage for ParseError {
    fn message(&self) -> String {
        match self {
            Self::ExpectedToken { expected, found, .. } => {
                let expected_tokens_string = expected.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(" or ");

                let found_string = match found.token_type {
                    TokenType::Identifier | TokenType::Number | TokenType::String => {
                        format!("'{}' ({})", found.token_type, found.lexeme)
                    }
                    _ => {
                        format!("'{}'", found.token_type)
                    }
                };
                format!("Expected '{expected_tokens_string}', found {found_string}")
            },
            Self::UnexpectedToken { found, .. } => {
                format!("Unexpected token ({})", found.lexeme)
            },
            Self::InvalidKeyword { found, .. } => {
                format!("Invalid keyword ({})", found.lexeme)
            },
            Self::InvalidPattern { found, .. } => {
                format!("Invalid pattern ({})", found.lexeme)
            },
            Self::InvalidAssignmentTarget { found, .. } => {
                format!("Invalid assignment target ({})", found.lexeme)
            },
            Self::MismatchedRangeBounds { start_bound, end_bound, .. } => {
                format!("Mismatched range bound types ('{}' and '{}') - Range bounds must have the same literal type", start_bound.token_type, end_bound.token_type)
            },
            Self::MissingRangeEnd { start_bound, .. } => {
                format!("Missing end bound for range starting with '{}'", start_bound.lexeme)
            },
            Self::DuplicateParameter { parameter, .. } => {
                format!("Duplicate parameter '{}'", parameter.lexeme)
            },
            Self::EmptyTuple { .. } => {
                "Empty tuple () is not allowed; use Void for type-level no-value".to_string()
            },
        }
    }
}

impl ErrorWithLocation for ParseError {
    fn line(&self) -> Option<u32> {
        match self {
            Self::ExpectedToken { line, .. } => Some(*line),
            Self::UnexpectedToken { line, .. } => Some(*line),
            Self::InvalidKeyword { line, .. } => Some(*line),
            Self::InvalidPattern { line, .. } => Some(*line),
            Self::InvalidAssignmentTarget { line, .. } => Some(*line),
            Self::MismatchedRangeBounds { line, .. } => Some(*line),
            Self::MissingRangeEnd { line, .. } => Some(*line),
            Self::DuplicateParameter { line, .. } => Some(*line),
            Self::EmptyTuple { line, .. } => Some(*line),
        }
    }
    fn column(&self) -> Option<u32> {
        match self {
            Self::ExpectedToken { column, .. } => Some(*column),
            Self::UnexpectedToken { column, .. } => Some(*column),
            Self::InvalidKeyword { column, .. } => Some(*column),
            Self::InvalidPattern { column, .. } => Some(*column),
            Self::InvalidAssignmentTarget { column, .. } => Some(*column),
            Self::MismatchedRangeBounds { column, .. } => Some(*column),
            Self::MissingRangeEnd { column, .. } => Some(*column),
            Self::DuplicateParameter { column, .. } => Some(*column),
            Self::EmptyTuple { column, .. } => Some(*column),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TypeError {
    MismatchedTypes {
        line: u32,
        column: u32,
        expected: TypeExpr,
        found: TypeExpr,
    },
    InfiniteType {
        line: u32,
        column: u32,
        type_var_id: usize,
        type_expr: TypeExpr,
    },
    MissingTypeAnnotationAndInitializer {
        line: u32,
        column: u32,
        identifier: String,
    },
    UndefinedVariable {
        line: u32,
        column: u32,
        identifier: String,
    },
    ConditionNotBoolean {
        line: u32,
        column: u32,
        condition_type: TypeExpr,
    },
    InvalidUnaryOperand {
        line: u32,
        column: u32,
        operator: String,
        operand_type: TypeExpr,
    },
    InvalidBinaryOperands {
        line: u32,
        column: u32,
        operator: String,
        left_type: TypeExpr,
        right_type: TypeExpr,
    },
    InvalidIndexTarget {
        line: u32,
        column: u32,
        target_type: TypeExpr,
    },
    InvalidIndexType {
        line: u32,
        column: u32,
        target_type: TypeExpr,
        index_type: TypeExpr,
    },
    InvalidFieldAccessTarget {
        line: u32,
        column: u32,
        target_type: TypeExpr,
    },
    InvalidCallTarget {
        line: u32,
        column: u32,
        target_type: TypeExpr,
    },
    InvalidMapKeyType {
        line: u32,
        column: u32,
        key_type: TypeExpr,
    },
    InvalidForInTarget {
        line: u32,
        column: u32,
        target_type: TypeExpr,
    },
    UnknownField {
        line: u32,
        column: u32,
        target_type: TypeExpr,
        field_name: String,
    },
    BreakOutsideLoop {
        line: u32,
        column: u32,
    },
    ContinueOutsideLoop {
        line: u32,
        column: u32,
    },
    EmptyMatch {
        line: u32,
        column: u32,
    },
    UnknownLiteral {
        line: u32,
        column: u32,
        token: Token,
    },
    MissingElseBranch {
        line: u32,
        column: u32,
    },
    ArgumentCountMismatch {
        line: u32,
        column: u32,
        expected: usize,
        found: usize,
    },
    EnumVariantPayloadCountMismatch {
        line: u32,
        column: u32,
        variant_identifier: String,
        expected: usize,
        found: usize,
    },
    InvalidEnumVariant {
        line: u32,
        column: u32,
        enum_type: TypeExpr,
        variant_identifier: String
    },
    InvalidEnumPatternTarget {
        line: u32,
        column: u32,
        subject_type: TypeExpr,
    },
    DuplicateEnumVariant {
        line: u32,
        column: u32,
        variant_identifier: String,
    },
    TupleFieldOutOfRange {
        line: u32,
        column: u32,
        tuple_type: TypeExpr,
        index: usize,
    },
    TupleArityMismatch {
        line: u32,
        column: u32,
        expected: usize,
        found: usize,
    },
    DestructureRequiresTuple {
        line: u32,
        column: u32,
        target_type: TypeExpr,
    }
}

impl ErrorMessage for TypeError {
    fn message(&self) -> String {
        match self {
            Self::MismatchedTypes { expected, found, .. } => format!("Type mismatch: expected {expected}, found {found}"),
            Self::InfiniteType { type_var_id, type_expr, .. } => format!("Cannot construct infinite type: type variable {type_var_id} occurs in {type_expr}"),
            Self::MissingTypeAnnotationAndInitializer { identifier, .. } => format!("Variable {identifier} must have a type annotation or an initializer"),
            Self::UndefinedVariable { identifier, .. } => format!("Undefined variable ({identifier})"),
            Self::ConditionNotBoolean { condition_type, .. } => format!("Condition must be have type Bool, found {condition_type}"),
            Self::InvalidUnaryOperand { operator, operand_type, .. } => format!("Invalid operand type {operand_type} for operator '{operator}'"),
            Self::InvalidBinaryOperands { operator, left_type, right_type, .. } => format!("Invalid operand types {left_type} and {right_type} for operator '{operator}'"),
            Self::InvalidIndexTarget { target_type, .. } => format!("Target type {target_type} cannot be indexed"),
            Self::InvalidIndexType { target_type, index_type, .. } => format!("Invalid index type {index_type} for {target_type}"),
            Self::InvalidFieldAccessTarget { target_type, .. } => format!("Cannot access fields on value of non-struct type {target_type}"),
            Self::InvalidCallTarget { target_type, .. } => format!("Cannot call value of non-function type {target_type}"),
            Self::InvalidForInTarget { target_type, .. } => format!("For-in iterable must be a Range, InclusiveRange, Array, or Map, found {target_type}"),
            Self::InvalidMapKeyType { key_type, .. } => format!("Map keys must be Int, Double, Bool, or String, found {key_type}"),
            Self::UnknownField { target_type, field_name, .. } => format!("Type {target_type} has no field named '{field_name}'"),
            Self::BreakOutsideLoop { .. } => "'break' must be inside a loop".to_string(),
            Self::ContinueOutsideLoop { .. } => "'continue' must be inside a loop".to_string(),
            Self::EmptyMatch { .. } => "Must have at least one case in match statement".to_string(),
            Self::UnknownLiteral { token, .. } => format!("Unknown literal ({})", token.lexeme),
            Self::MissingElseBranch { .. } => "Must have an else branch in if expression".to_string(),
            Self::ArgumentCountMismatch { expected, found, .. } => format!("Expected {expected} arguments, found {found}"),
            Self::EnumVariantPayloadCountMismatch { variant_identifier, expected, found, .. } => format!("Enum variant '{variant_identifier}' expected {expected} arguments, found {found}"),
            Self::InvalidEnumVariant { enum_type, variant_identifier, .. } => format!("Enum type {enum_type} has no variant named '{variant_identifier}'"),
            Self::InvalidEnumPatternTarget { subject_type, .. } => format!("Cannot pattern match enum variant on non-enum type {subject_type}"),
            Self::DuplicateEnumVariant { variant_identifier, .. } => format!("Duplicate enum variant '{variant_identifier}'"),
            Self::TupleFieldOutOfRange { tuple_type, index, .. } => format!("Tuple field index {index} is out of range for type {tuple_type}"),
            Self::TupleArityMismatch { expected, found, .. } => format!("Tuple size mismatch: expected {expected} elements, found {found}"),
            Self::DestructureRequiresTuple { target_type, .. } => format!("Tuple destructuring requires a tuple type, found {target_type}"),
        }
    }
}

impl ErrorWithLocation for TypeError {
    fn line(&self) -> Option<u32> {
        match self {
            Self::MismatchedTypes { line, .. } => Some(*line),
            Self::InfiniteType { line, .. } => Some(*line),
            Self::MissingTypeAnnotationAndInitializer { line, .. } => Some(*line),
            Self::UndefinedVariable { line, .. } => Some(*line),
            Self::ConditionNotBoolean { line, .. } => Some(*line),
            Self::InvalidUnaryOperand { line, .. } => Some(*line),
            Self::InvalidBinaryOperands { line, .. } => Some(*line),
            Self::InvalidIndexTarget { line, .. } => Some(*line),
            Self::InvalidIndexType { line, .. } => Some(*line),
            Self::InvalidFieldAccessTarget { line, .. } => Some(*line),
            Self::InvalidCallTarget { line, .. } => Some(*line),
            Self::InvalidForInTarget { line, .. } => Some(*line),
            Self::InvalidMapKeyType { line, .. } => Some(*line),
            Self::UnknownField { line, .. } => Some(*line),
            Self::BreakOutsideLoop { line, .. } => Some(*line),
            Self::ContinueOutsideLoop { line, .. } => Some(*line),
            Self::EmptyMatch { line, .. } => Some(*line),
            Self::UnknownLiteral { line, .. } => Some(*line),
            Self::MissingElseBranch { line, .. } => Some(*line),
            Self::ArgumentCountMismatch { line, .. } => Some(*line),
            Self::EnumVariantPayloadCountMismatch { line, .. } => Some(*line),
            Self::InvalidEnumVariant { line, .. } => Some(*line),
            Self::InvalidEnumPatternTarget { line, .. } => Some(*line),
            Self::DuplicateEnumVariant { line, .. } => Some(*line),
            Self::TupleFieldOutOfRange { line, .. } => Some(*line),
            Self::TupleArityMismatch { line, .. } => Some(*line),
            Self::DestructureRequiresTuple { line, .. } => Some(*line),
        }
    }
    fn column(&self) -> Option<u32> {
        match self {
            Self::MismatchedTypes { column, .. } => Some(*column),
            Self::InfiniteType { column, .. } => Some(*column),
            Self::MissingTypeAnnotationAndInitializer { column, .. } => Some(*column),
            Self::UndefinedVariable { column, .. } => Some(*column),
            Self::ConditionNotBoolean { column, .. } => Some(*column),
            Self::InvalidUnaryOperand { column, .. } => Some(*column),
            Self::InvalidBinaryOperands { column, .. } => Some(*column),
            Self::InvalidIndexTarget { column, .. } => Some(*column),
            Self::InvalidIndexType { column, .. } => Some(*column),
            Self::InvalidFieldAccessTarget { column, .. } => Some(*column),
            Self::InvalidCallTarget { column, .. } => Some(*column),
            Self::InvalidForInTarget { column, .. } => Some(*column),
            Self::InvalidMapKeyType { column, .. } => Some(*column),
            Self::UnknownField { column, .. } => Some(*column),
            Self::BreakOutsideLoop { column, .. } => Some(*column),
            Self::ContinueOutsideLoop { column, .. } => Some(*column),
            Self::EmptyMatch { column, .. } => Some(*column),
            Self::UnknownLiteral { column, .. } => Some(*column),
            Self::MissingElseBranch { column, .. } => Some(*column),
            Self::ArgumentCountMismatch { column, .. } => Some(*column),
            Self::EnumVariantPayloadCountMismatch { column, .. } => Some(*column),
            Self::InvalidEnumVariant { column, .. } => Some(*column),
            Self::InvalidEnumPatternTarget { column, .. } => Some(*column),
            Self::DuplicateEnumVariant { column, .. } => Some(*column),
            Self::TupleFieldOutOfRange { column, .. } => Some(*column),
            Self::TupleArityMismatch { column, .. } => Some(*column),
            Self::DestructureRequiresTuple { column, .. } => Some(*column),
        }
    }
}

#[derive(Debug, Clone)]
pub enum UnifyError {
    MismatchedTypes {
        expected: TypeExpr,
        found: TypeExpr,
    },
    InfiniteType {
        type_var_id: usize,
        type_expr: TypeExpr,
    },
}

impl ErrorMessage for UnifyError {
    fn message(&self) -> String {
        match self {
            Self::MismatchedTypes { expected, found, .. } => format!("Type mismatch: expected {expected}, found {found}"),
            Self::InfiniteType { type_var_id, type_expr, .. } => format!("Cannot construct infinite type: type variable {type_var_id} occurs in {type_expr}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CodegenError {
    MissingType {
        line: u32,
        column: u32,
        identifier: String,
    },
    UnexpectedTypeInCodegen {
        line: u32,
        column: u32,
        expected: &'static str,
        found_type: TypeExpr,
    },
    InvalidEnumArgumentCount {
        line: u32,
        column: u32,
        variant_identifier: String,
        expected: usize,
        found: usize,
    },
    InvalidCallTarget {
        line: u32,
        column: u32,
        target_type: Option<TypeExpr>,
    },
    MissingElseInIfExpression {
        line: u32,
        column: u32,
    },
    BlockExpressionMissingEndExpression {
        line: u32,
        column: u32,
    },
    UnsupportedMatchPattern {
        line: u32,
        column: u32,
    },
    UnknownEnumVariantInPattern {
        line: u32,
        column: u32,
        enum_identifier: String,
    },
    InvalidEnumPatternArgumentCount {
        line: u32,
        column: u32,
        enum_identifier: String,
        variant_identifier: String,
        expected: usize,
        found: usize,
    },
}

impl ErrorMessage for CodegenError {
    fn message(&self) -> String {
        match self {
            Self::MissingType { identifier, .. } => format!("Could not determine type for '{}'", identifier),
            Self::UnexpectedTypeInCodegen { expected, found_type, .. } => format!("Internal codegen error: expected {expected}, but found {found_type}"),
            Self::InvalidEnumArgumentCount { variant_identifier, expected, found, .. } => format!("Enum variant '{variant_identifier}' expected {expected} arguments, found {found}"),
            Self::InvalidCallTarget { target_type, .. } => {
                let type_description = match target_type {
                    Some(type_expr) => format!("{type_expr}"),
                    None => "UNKNOWN".to_string(),
                };
                format!("Cannot call value of type {}", type_description)
            }
            Self::MissingElseInIfExpression { .. } => "Missing else branch in if expression".to_string(),
            Self::BlockExpressionMissingEndExpression { .. } => "Block expression must end with an expression".to_string(),
            Self::UnsupportedMatchPattern { .. } => "Unsupported match pattern".to_string(),
            Self::UnknownEnumVariantInPattern { enum_identifier, .. } => format!("Unknown enum variant in pattern for enum '{enum_identifier}'"),
            Self::InvalidEnumPatternArgumentCount { enum_identifier, variant_identifier, expected, found, .. } => format!("Invalid number of patterns for enum variant '{variant_identifier}' of enum '{enum_identifier}' - expected {expected}, but found {found}"),
        }
    }
}

impl ErrorWithLocation for CodegenError {
    fn line(&self) -> Option<u32> {
        match self {
            Self::MissingType { line, .. } => Some(*line),
            Self::UnexpectedTypeInCodegen { line, .. } => Some(*line),
            Self::InvalidEnumArgumentCount { line, .. } => Some(*line),
            Self::InvalidCallTarget { line, .. } => Some(*line),
            Self::MissingElseInIfExpression { line, .. } => Some(*line),
            Self::BlockExpressionMissingEndExpression { line, .. } => Some(*line),
            Self::UnsupportedMatchPattern { line, .. } => Some(*line),
            Self::UnknownEnumVariantInPattern { line, .. } => Some(*line),
            Self::InvalidEnumPatternArgumentCount { line, .. } => Some(*line),
        }
    }
    fn column(&self) -> Option<u32> {
        match self {
            Self::MissingType { column, .. } => Some(*column),
            Self::UnexpectedTypeInCodegen { column, .. } => Some(*column),
            Self::InvalidEnumArgumentCount { column, .. } => Some(*column),
            Self::InvalidCallTarget { column, .. } => Some(*column),
            Self::MissingElseInIfExpression { column, .. } => Some(*column),
            Self::BlockExpressionMissingEndExpression { column, .. } => Some(*column),
            Self::UnsupportedMatchPattern { column, .. } => Some(*column),
            Self::UnknownEnumVariantInPattern { column, .. } => Some(*column),
            Self::InvalidEnumPatternArgumentCount { column, .. } => Some(*column),
        }
    }
}

pub enum CompilerRunnerError {
    FailedToGetCurrentDirectory,
    NoCompilerFound {
        compilers_tried: Vec<String>,
    },
    CompilationFailed {
        compiler: String,
        status: i32,
        stderr: String,
    },
    IoError(std::io::Error),
}

impl ErrorMessage for CompilerRunnerError {
    fn message(&self) -> String {
        match self {
            Self::FailedToGetCurrentDirectory => "Failed to get current directory".to_string(),
            Self::NoCompilerFound { compilers_tried } => format!("No compiler found, tried: {}", compilers_tried.join(", ")),
            Self::CompilationFailed { compiler, status, stderr } => format!("Compilation failed using {compiler} with status {status}:\n{stderr}"),
            Self::IoError(error) => format!("IO error: {}", error),
        }
    }
}

impl ErrorWithLocation for CompilerRunnerError {
    fn line(&self) -> Option<u32> {
        None
    }
    fn column(&self) -> Option<u32> {
        None
    }
}

impl From<std::io::Error> for CompilerRunnerError {
    fn from(err: std::io::Error) -> Self {
        CompilerRunnerError::IoError(err)
    }
}

pub fn print_error<E: ErrorWithLocation>(error: &E, lines: &[&str], file_path: &str) {
    eprintln!("error: {}", error.message());

    if let Some(line_num) = error.line() {
        if let Some(column_num) = error.column() {
            eprintln!("@ {file_path}:{line_num}:{column_num}");
        } else {
            eprintln!("@ {file_path}:{line_num}:?");
        }

        if let Some(src_line) = lines.get((line_num - 1) as usize) {
            let num_digits = line_num.to_string().len();
            eprintln!("{} |", " ".repeat(num_digits));
            eprintln!("{line_num} | {src_line}");
            if let Some(column_num) = error.column() {
                eprintln!("{} | {}", " ".repeat(num_digits), " ".repeat((column_num - 1) as usize) + &"^".repeat(1)); // TODO: change to get actual length 
            } else {
                eprintln!("{} | {}", " ".repeat(num_digits), "^".repeat(src_line.len()));
            }
        }
    }

    eprintln!();
}

pub fn unwrap_or_exit<T, E: ErrorWithLocation>(result: Result<T, Vec<E>>, lines: &[&str], file_path: &str) -> T {
    match result {
        Ok(value) => value,
        Err(errors) => {
            let error_type = if type_name::<E>().contains("ParseError") {
                "parse"
            } else if type_name::<E>().contains("ScanError") {
                "scan"
            } else if type_name::<E>().contains("TypeError") {
                "type"
            } else if type_name::<E>().contains("CodegenError") {
                "codegen"
            } else if type_name::<E>().contains("CompilerRunnerError") {
                "compiler runner"
            } else {
                "unknown"
            };
            let error_message = format!(">>> Found {} {} error{} <<<", errors.len(), error_type, if errors.len() == 1 { "" } else { "s" });
            let border = "=".repeat(error_message.len());
            eprintln!("{border}");
            eprintln!("{error_message}");
            eprintln!("{border}");
            eprintln!();
            for error in &errors {
                print_error(error, lines, file_path);
            }
            std::process::exit(65);
        }
    }
}