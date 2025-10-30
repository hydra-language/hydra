use crate::{CompilerError, Error, Hint};
use lexer::Token;

pub struct TypeMismatchError<'a> {
    code: &'a str,
    message: String,
    token: Token<'a>,
    expected: &'a str,
    found: Token<'a>,
}

impl<'a> CompilerError for TypeMismatchError<'a> {
    fn report(&self, source: &str, filename: &str) {
        let error_to_report = Error {
            code: self.code,
            message: &self.message,
            token: self.token.clone(),
        };

        error_to_report.report(source, filename);
    }
}

impl<'a> Hint for TypeMismatchError<'a> {
    fn hint(&self) -> Option<String> {
        // let found = self.found.lexeme;
        
        Some("todo".to_string())
    }
}

pub fn type_mismatch<'a>(token: Token<'a>, expected: &'a str, found: Token<'a>) -> impl CompilerError + 'a {
    TypeMismatchError {
        code: "E003",
        message: format!("type mismatch: expected '{}', found '{}'", expected, found.lexeme),
        token,
        expected,
        found
    }
}

