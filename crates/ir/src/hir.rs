use errors::error::Span;
use crate::types::Type;
use crate::context::DefID;
use std::fmt;

#[derive(Debug, Clone)]
pub struct HIRProgram {
    pub functions: Vec<HIRFunction>,
    pub structs: Vec<(String, Vec<(String, Type)>)>, 
    pub globals: Vec<(String, Type, HIRExpr)>,
}

#[derive(Debug, Clone)]
pub struct HIRFunction {
    pub name: String, 
    pub def_id: DefID, 
    pub params: Vec<(DefID, Type)>, 
    pub return_type: Type,
    pub body: HIRBlock,
    pub is_extern: bool,
    pub is_intrinsic: bool,
    pub generic_params: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HIRExpr {
    pub kind: HIRExprKind,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HIRExprKind {
    // 1. Value References
    VarRef(DefID),

    // 2. Literals
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    CharLiteral(char),
    BoolLiteral(bool),

    // 3. Memory & Aggregates
    StructInit {
        def_id: DefID,
        values: Vec<HIRExpr>, 
    },

    ArrayInit {
        elements: Vec<HIRExpr>,
    },

    ArrayAccess {
        array: Box<HIRExpr>,
        index: Box<HIRExpr>,
    },

    FieldAccess {
        object: Box<HIRExpr>,
        field_index: usize,
    },

    // 4. Operations
    Call {
        callee: DefID,
        args: Vec<HIRExpr>,
        generic_args: Vec<Type>, 
    },

    BuiltinCall {
        name: String,
        args: Vec<HIRExpr>,
    },

    Binary {
        op: HIRBinOp,
        lhs: Box<HIRExpr>,
        rhs: Box<HIRExpr>,
    },

    Unary {
        op: HIRUnaryOp,
        operand: Box<HIRExpr>,
    },

    Cast {
        expr: Box<HIRExpr>,
        kind: CastKind, // Makes LLVM instruction selection extremely easy
    },

    Borrow { 
        is_mut: bool, 
        target: Box<HIRExpr> 
    },

    Dereference { 
        target: Box<HIRExpr> 
    },

    Assign {
        target: Box<HIRExpr>, 
        value: Box<HIRExpr>,
    },

    // 5. Control Flow Primitives
    If {
        cond: Box<HIRExpr>,
        then_block: Box<HIRBlock>,
        else_block: Option<Box<HIRBlock>>,
    },

    Loop(Box<HIRBlock>),
    Break,
    Continue,
    Return(Option<Box<HIRExpr>>),
    Block(HIRBlock),
}

#[derive(Debug, Clone)]
pub struct HIRBlock {
    pub stmts: Vec<HIRStmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HIRStmt {
    VarDecl {
        def_id: DefID,
        init: Option<HIRExpr>,
    },
    Expr(HIRExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HIRBinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HIRUnaryOp {
    Neg, Not, AddrOf, Deref,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    Numeric,  // Covers widening, truncating, int-to-float, etc.
    Pointer,  // Covers references to pointers, pointer decay
    NoOp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64, Type),
    Float(f64, Type),
    Bool(bool),
    Char(char),
    String(String),
}

impl fmt::Display for HIRFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fn {} (", self.name)?;
        for (i, (def_id, ty)) in self.params.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}: {}", def_id, ty)?;
        }
        write!(f, ") -> {} ", self.return_type)?;
        write!(f, "{}", self.body)
    }
}

impl fmt::Display for HIRBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{{")?;
        for stmt in &self.stmts {
            let stmt_str = format!("{}", stmt);
            for line in stmt_str.lines() {
                writeln!(f, "    {}", line)?;
            }
        }
        write!(f, "}}")
    }
}

impl fmt::Display for HIRStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HIRStmt::VarDecl { def_id, init } => {
                write!(f, "let {}", def_id)?;
                if let Some(expr) = init {
                    write!(f, " = {};", expr)?;
                } else {
                    write!(f, ";")?;
                }
                Ok(())
            }
            HIRStmt::Expr(expr) => write!(f, "{};", expr),
        }
    }
}

impl fmt::Display for HIRExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.ty)
    }
}

impl fmt::Display for HIRExprKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HIRExprKind::VarRef(id) => write!(f, "{}", id),
            HIRExprKind::IntLiteral(val) => write!(f, "{}", val),
            HIRExprKind::FloatLiteral(val) => write!(f, "{}", val),
            HIRExprKind::StringLiteral(val) => write!(f, "\"{}\"", val),
            HIRExprKind::CharLiteral(val) => write!(f, "'{}'", val),
            HIRExprKind::BoolLiteral(val) => write!(f, "{}", val),
            
            HIRExprKind::StructInit { def_id, values } => {
                write!(f, "{} {{ ", def_id)?;
                for (i, val) in values.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", val)?;
                }
                write!(f, " }}")
            }

            HIRExprKind::ArrayInit { elements } => {
                write!(f, "[")?;
                for (i, val) in elements.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", val)?;
                }
                write!(f, "]")
            }

            HIRExprKind::ArrayAccess { array, index } => write!(f, "{}[{}]", array, index),
            HIRExprKind::FieldAccess { object, field_index } => write!(f, "{}.{}", object, field_index),
            
            HIRExprKind::Call { callee, args, .. } => {
                write!(f, "call {}(", callee)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }

            HIRExprKind::BuiltinCall { name, args } => {
                write!(f, "builtin {}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            
            HIRExprKind::Binary { op, lhs, rhs } => write!(f, "({} {} {})", lhs, op, rhs),
            HIRExprKind::Unary { op, operand } => write!(f, "{}{}", op, operand),
            HIRExprKind::Cast { expr, kind } => write!(f, "({} as {:?})", expr, kind),
            HIRExprKind::Assign { target, value } => write!(f, "{} = {}", target, value),

            HIRExprKind::Borrow { is_mut, target } => {
                let mut_str = if *is_mut { "mut " } else { "" };
                write!(f, "&{}{}", mut_str, target)
            }

            HIRExprKind::Dereference { target } => {
                write!(f, "*{}", target)
            }
            
            HIRExprKind::If { cond, then_block, else_block } => {
                write!(f, "if {} {}", cond, then_block)?;
                if let Some(els) = else_block {
                    write!(f, " else {}", els)?;
                }
                Ok(())
            }
            HIRExprKind::Loop(block) => write!(f, "loop {}", block),
            HIRExprKind::Break => write!(f, "break"),
            HIRExprKind::Continue => write!(f, "continue"),
            HIRExprKind::Return(Some(expr)) => write!(f, "return {}", expr),
            HIRExprKind::Return(None) => write!(f, "return"),
            HIRExprKind::Block(block) => write!(f, "{}", block),
        }
    }
}

impl fmt::Display for HIRBinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        let op_str = match self {
            HIRBinOp::Add => "+", HIRBinOp::Sub => "-", HIRBinOp::Mul => "*",
            HIRBinOp::Div => "/", HIRBinOp::Mod => "%", HIRBinOp::Eq => "==",
            HIRBinOp::Ne => "!=", HIRBinOp::Lt => "<", HIRBinOp::Le => "<=",
            HIRBinOp::Gt => ">", HIRBinOp::Ge => ">=", HIRBinOp::And => "&&",
            HIRBinOp::Or => "||",
        };
        write!(f, "{}", op_str)
    }
}

impl fmt::Display for HIRUnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        let op_str = match self {
            HIRUnaryOp::Neg => "-", HIRUnaryOp::Not => "!",
            HIRUnaryOp::AddrOf => "&", HIRUnaryOp::Deref => "*",
        };
        write!(f, "{}", op_str)
    }
}
