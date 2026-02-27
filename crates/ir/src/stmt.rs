use std::fmt;
use crate::expr::Expr;
use crate::types::Type;

#[derive(Debug, Clone)]
pub enum Stmt {
    Block(Block),

    Var {
        name: String,
        ty: Type,
        init: Expr,
        is_mutable: bool,
    },

    Assign {
        target: AssignmentTarget,
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
        kind: LoopKind,
    },

    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub enum AssignmentTarget {
    Variable(String),
    
    ArrayAccess {
        array: Expr,
        index: Expr,
    },

    MemberAccess { 
        object: Box<Expr>, 
        member: String,
        index: u32,
    },

    PointerDeref(Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum LoopKind {
    While,
    For,
    ForEach,
}

impl Stmt {

    pub fn pretty_print(&self, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        
        match self {
            Stmt::Block(block) => block.pretty_print(indent),

            Stmt::Var { name, ty, init, is_mutable } => {
                let keyword = if *is_mutable { "let" } else { "const" };
                format!("{}{} {}: {} = {};", prefix, keyword, name, ty, init.pretty_print(indent))
            },

            Stmt::Assign { target, value } => {
                format!("{}{} = {};", prefix, target, value.pretty_print(indent))
            },

            Stmt::Expr(expr) => {
                format!("{}{};", prefix, expr.pretty_print(indent))
            },

            Stmt::Return(Some(expr)) => {
                format!("{}return {};", prefix, expr.pretty_print(indent))
            },

            Stmt::Return(None) => format!("{}return;", prefix),
            
            // For blocks, we increment indent
            Stmt::If { cond, then_block, else_block } => {
                let mut output = format!("{}if {} {}", prefix, cond.pretty_print(indent), then_block.pretty_print(indent));

                if let Some(else_b) = else_block {
                    output.push_str(&format!(" else {}", else_b.pretty_print(indent)));
                }

                output
            },

            Stmt::While { cond, body, kind } => {
                let kind_str = match kind {
                    LoopKind::While => "while",
                    LoopKind::For => "while [for]",
                    LoopKind::ForEach => "while [foreach]",
                };

                format!("{}{} {} {}", prefix, kind_str, cond.pretty_print(indent), body.pretty_print(indent))
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

impl AssignmentTarget {

    pub fn pretty_print(&self, indent: usize) -> String {
        match self {
            AssignmentTarget::Variable(name) => name.clone(),

            AssignmentTarget::ArrayAccess { array, index } => {
                format!("{}[{}]", array.pretty_print(indent), index.pretty_print(indent))
            },

            AssignmentTarget::MemberAccess { object, member, .. } => {
                format!("{}.{}", object.pretty_print(indent), member)
            },

            AssignmentTarget::PointerDeref(expr) => {
                format!("*{}", expr.pretty_print(indent))
            },
        }
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

impl fmt::Display for AssignmentTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_print(0))
    }
}
