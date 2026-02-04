pub mod no_main;
pub mod expected_found;
pub mod type_mismatch;
pub mod generic;

use lexer::Token;

pub trait Hint {
    fn hint(&self) -> Option<String>;
}

pub trait CompilerError {
    fn report(&self, source: &str, filename: &str);
}

#[derive(Debug, Clone)]
pub enum HydraError<'a> {
    NO_MAIN(Box<no_main::NoMainError<'a>>),
    EXPECTED_FOUND(Box<expected_found::ExpectedFoundError<'a>>),
    TYPE_MISMATCH(Box<type_mismatch::TypeMismatch<'a>>),
    GENERIC(Box<generic::GenericError<'a>>),
}

impl<'a> HydraError<'a> {
    pub fn report(&self, source: &str, filename: &str) {
        match self {
            HydraError::NO_MAIN(e) => e.report(source, filename),
            HydraError::EXPECTED_FOUND(e) => e.report(source, filename),
            HydraError::TYPE_MISMATCH(e) => e.report(source, filename),
            HydraError::GENERIC(e) => e.report(source, filename),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Error<'a> {
    pub code: &'a str,
    pub message: &'a str,
    pub token: Token<'a>,
    pub help: Option<String>
}

impl<'a> CompilerError for Error<'a> {
    fn report(&self, source: &str, filename: &str) {
        let line_str = source.lines().nth(self.token.line - 1).unwrap_or("");

        eprintln!("\nerror[{}]\n{}", self.code, self.message);
        eprintln!("  --> {}:{}:{}", filename, self.token.line, self.token.column);
        eprintln!("   |");
        eprintln!("{:>3} | {}", self.token.line, line_str);
        eprintln!("   | {}{}", " ".repeat(self.token.column), "^".repeat(self.token.lexeme.len()));
        eprintln!("   |");

        if let Some(help_msg) = &self.help {
            eprintln!("     = help: {}", help_msg);
        }
    }
}
