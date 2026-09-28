use errors::error::Span;
use crate::intrinsic::IntrinsicKind;
use crate::types::Type;
use crate::context::DefID;

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
    pub is_inline: bool,
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
    NONE,
    OptionalPresent(Box<HIRExpr>),

    // 3. Memory & Aggregates
    StructInit {
        def_id: DefID,
        values: Vec<HIRExpr>, 
    },

    ArrayInit {
        elements: Vec<HIRExpr>,
    },

    SliceInit {
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
        trait_dispatch: Option<DefID>,
    },

    IntrinsicCall {
        callee: DefID,
        kind: IntrinsicKind,
        args: Vec<HIRExpr>,
        type_args: Vec<Type>,
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

    OrElse {
        optional: Box<HIRExpr>,
        fallback: Box<HIRExpr>,
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
        ty: Type,
        init: Option<HIRExpr>,
        has_type_annotation: bool,
        span: Span,
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

