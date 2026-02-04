use crate::{CompilerError, Error, Hint};
use lexer::Token;

#[derive(Debug, Clone)]
pub struct TypeMismatch<'a> {
    code: &'a str,
    message: String,
    token: Token<'a>,
    expected: &'a str,
    found: Token<'a>,
}

impl<'a> CompilerError for TypeMismatch<'a> {
    fn report(&self, source: &str, filename: &str) {
        let error_to_report = Error {
            code: self.code,
            message: &self.message,
            token: self.token.clone(),
            help: None
        };

        error_to_report.report(source, filename);

        if let Some(hint) = self.hint() {
            eprintln!("     = {}", hint)
        }
    }
}

impl<'a> Hint for TypeMismatch<'a> {
    fn hint(&self) -> Option<String> {
        Some(format!("help: consider changing the type from '{}' to '{}'", self.found.lexeme, self.expected))
    }
}

pub fn type_mismatch<'a>(token: Token<'a>, expected: &'a str, found: Token<'a>) -> impl CompilerError + 'a {
    TypeMismatch {
        code: "E003",
        message: format!("type mismatch: expected '{}', found '{}'", expected, found.lexeme),
        token,
        expected,
        found
    }
}

