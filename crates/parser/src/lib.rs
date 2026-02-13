pub mod ast;
pub mod parser;
pub mod loader;

pub use ast::*;
pub use errors::HydraError as ParserError;

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub enum StructSection {
    NONE = 0,
    CONSTANTS = 1,
    FIELDS = 2,
    METHODS = 3,
}
