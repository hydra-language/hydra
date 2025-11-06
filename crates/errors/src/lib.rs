pub mod err001;
pub mod err002;
pub mod err003;
pub mod err004;

use lexer::Token;

pub trait Hint {
    fn hint(&self) -> Option<String>;
}

pub trait CompilerError {
    fn report(&self, source: &str, filename: &str);
}

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
