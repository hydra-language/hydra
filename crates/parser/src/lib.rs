pub mod ast;
pub mod parser;
pub mod semantic;
pub mod type_check;
pub mod symbol;

pub use ast::*;
use errors::CompilerError;
use lexer::Token;

#[derive(Debug)]
pub enum ParserError<'a> {
    NoMainFunction,

    ExpectedToken {
        expected: String,
        found: Token<'a>,
    },

    TypeMismatch {
        token: Token<'a>,
        expected: String,
        found: Token<'a>
    },

    Generic{
        message: String, 
        token: Token<'a>,
        help: Option<String>
    }
}

impl<'a> CompilerError for ParserError<'a> {
    fn report(&self, source: &str, filename: &str) {
        match self {
            ParserError::NoMainFunction => {
                errors::err001::no_main().report(source, filename);
            },

            ParserError::ExpectedToken { expected, found } => {
                errors::err002::expected_found(expected, found.clone()).report(source, filename);
            },

            ParserError::Generic { message, token, help } => {
                let error = errors::Error {
                    code: "E003",
                    message,
                    token: token.clone(),
                    help: help.clone()
                };

                error.report(source, filename);
            },
            
            ParserError::TypeMismatch { token, expected, found } => {
                errors::err003::type_mismatch(token.clone(), &expected, found.clone()).report(source, filename);
            }
        }
    }
}
