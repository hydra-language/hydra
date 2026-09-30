pub mod types;
pub mod expr;
pub mod stmt;
pub mod context;
pub mod intrinsic;
pub mod hir;
pub mod instance;
pub mod display;

use types::Type;
use stmt::Block;

use crate::expr::Expr;

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Block,
    pub is_extern: bool,
    pub is_intrinsic: bool,
    pub generic_params: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Vec<Function>,
    pub structs: Vec<(String, Vec<(String, Type)>)>,
    pub globals: Vec<(String, Type, Expr)>
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64, Type),
    Float(f64, Type),
    Bool(bool),
    Char(char),
    String(String),
}
