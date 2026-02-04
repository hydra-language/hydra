use std::fmt;
use crate::expr::Expr;
use crate::types::Type;

#[derive(Debug, Clone)]
pub enum Stmt {
    Var {
        name: String,
        ty: Type,
        init: Expr,
        is_mutable: bool,
    },

    Assign {
        name: String,
        value: Expr,
    },

    Expr(Expr),

    Return(Option<Expr>),

    If {
        cond: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },

    While {
        cond: Expr,
        body: Block,
    },

    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

impl Stmt {
    pub fn pretty_print(&self, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        
        match self {
            Stmt::Var { name, ty, init, .. } => {
                format!("{}let {}: {} = {};", prefix, name, ty, init.pretty_print(indent))
            },
            Stmt::Assign { name, value } => {
                format!("{}{} = {};", prefix, name, value.pretty_print(indent))
            },
            Stmt::Expr(expr) => {
                format!("{}{};", prefix, expr.pretty_print(indent))
            },
            Stmt::Return(Some(expr)) => {
                format!("{}return {};", prefix, expr.pretty_print(indent))
            },
            Stmt::Return(None) => format!("{}return;", prefix),
            
            // For blocks, we increment indent
            Stmt::If { cond, then_block, .. } => {
                format!("{}if {} {}", prefix, cond.pretty_print(indent), then_block.pretty_print(indent))
            },
            Stmt::While { cond, body } => {
                format!("{}while {} {}", prefix, cond.pretty_print(indent), body.pretty_print(indent))
            },
            Stmt::Break => format!("{}break;", prefix),
            Stmt::Continue => format!("{}continue;", prefix),
        }
    }
}

impl Block {
    pub fn pretty_print(&self, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        let mut s = String::from("{\n");
        
        for stmt in &self.stmts {
            // Statements inside blocks get +1 indentation
            s.push_str(&format!("{}\n", stmt.pretty_print(indent + 1)));
        }
        
        s.push_str(&format!("{}}}", prefix));
        s
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_print(0))
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_print(0))
    }
}
