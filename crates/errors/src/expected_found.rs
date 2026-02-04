use crate::{CompilerError, Error};
use lexer::Token;

#[derive(Debug, Clone)]
pub struct ExpectedFoundError<'a> {
    pub code: &'a str,
    pub message: String,
    pub token: Token<'a>,
}

impl<'a> CompilerError for ExpectedFoundError<'a> {
    fn report(&self, source: &str, filename: &str) {
        let error_to_report = Error {
            code: self.code,
            message: &self.message,
            token: self.token.clone(),
            help: None
        };

        error_to_report.report(source, filename);
    }
}

pub fn expected_found<'a>(expected: &str, found: Token<'a>) -> impl CompilerError + 'a {
    ExpectedFoundError {
        code: "E002",
        message: format!("expected {}, but found `{}`", expected, found.lexeme),
        token: found,
    }
}
