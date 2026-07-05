pub mod types;
pub mod expr;
pub mod stmt;
pub mod context;
pub mod hir;

use std::fmt;
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

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params_str = self.params.iter()
            .map(|(n, t)| format!("{}: {}", n, t))
            .collect::<Vec<_>>()
            .join(", ");
            
        write!(f, "fn {}({}) -> {} {}", self.name, params_str, self.return_type, self.body)
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for func in &self.functions {
            writeln!(f, "{}", func)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Constant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Constant::Int(v, _) => write!(f, "{}", v),
            Constant::Float(v, _) => write!(f, "{}", v),
            Constant::Bool(v) => write!(f, "{}", v),
            Constant::Char(v) => write!(f, "'{}'", v),
            Constant::String(v) => write!(f, "\"{}\"", v),
        }
    }
}
