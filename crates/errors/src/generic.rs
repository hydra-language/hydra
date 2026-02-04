use crate::{CompilerError, Error, Hint};
use lexer::Token;

#[derive(Debug, Clone)]
pub struct GenericError<'a> {
    pub code: &'a str,
    pub message: String,
    pub token: Token<'a>,
    pub help: Option<String>,
}

impl<'a> CompilerError for GenericError<'a> {
    fn report(&self, source: &str, filename: &str) {
        let error = Error {
            code: self.code,
            message: &self.message,
            token: self.token.clone(),
            help: self.help.clone(),
        };

        error.report(source, filename);
    }
}

impl<'a> Hint for GenericError<'a> {
    fn hint(&self) -> Option<String> {
        self.help.clone()
    }
}
