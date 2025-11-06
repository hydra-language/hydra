use crate::{CompilerError, Error, Hint};
use lexer::Token;

struct NoMainError<'a> {
    error: Error<'a>
}

impl<'a> CompilerError for NoMainError<'a> {
    fn report(&self, source: &str, filename: &str) {
        self.error.report(source, filename);

        if let Some(hint) = self.hint() {
            eprintln!("     = {}", hint)
        }
    }
}

impl<'a> Hint for NoMainError<'a> {
    fn hint(&self) -> Option<String> {
        Some("help: consider adding `fn main() -> void {}` to your code".to_string())
    }
}

pub fn no_main<'a>() -> impl CompilerError + Hint + 'a {
    NoMainError {
        error: Error {
            code: "E001",
            message: "no `main` function found",
            token: Token {
                token_type: lexer::TokenType::EOF,
                lexeme: "",
                line: 1,
                column: 1
            },
            help: None,
        },
    }
}
