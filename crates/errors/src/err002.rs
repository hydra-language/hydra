use crate::{CompilerError, Error};
use lexer::Token;

pub struct FormattedError<'a> {
    code: &'a str,
    message: String,
    token: Token<'a>,
}

impl<'a> CompilerError for FormattedError<'a> {
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
    FormattedError {
        code: "E002",
        message: format!("expected {}, but found `{}`", expected, found.lexeme),
        token: found,
    }
}
