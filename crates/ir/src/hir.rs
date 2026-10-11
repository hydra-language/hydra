use errors::error::Span;

use crate::{Constant, types::TypeRef};
use crate::context::DefID;
use crate::intrinsic::IntrinsicKind;
use crate::types::Type;

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct HIRLocal {
    pub def_id: DefID,
    pub name: String,
    pub ty: Type,
    pub is_mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HIRFunctionRef {
    pub def_id: DefID,
    pub symbol: String,
    pub display_symbol: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HIRProgram {
    pub functions: Vec<HIRFunction>,
    pub structs: Vec<(String, Vec<(String, Type)>)>, 
    pub globals: Vec<(String, Type, HIRExpr)>,
}

#[derive(Debug, Clone)]
pub struct HIRStructInfo {
    pub type_ref: TypeRef,
    pub fields: Vec<Type>,
}

#[derive(Debug, Clone, Default)]
pub struct HIRTypeInfo {
    pub structs: HashMap<DefID, HIRStructInfo>,
    pub copy_types: HashSet<Type>,
    pub drop_functions: HashMap<Type, HIRFunctionRef>,
}

#[derive(Debug, Clone)]
pub struct MonomorphizedProgram {
    pub functions: Vec<HIRFunction>,
    pub type_info: HIRTypeInfo,
}

#[derive(Debug, Clone)]
pub struct HIRFunction {
    pub name: String,
    pub def_id: DefID,
    pub symbol: String,
    pub source_id: u32,
    pub display_name: Option<String>,
    pub params: Vec<HIRLocal>,
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
    ConstRef { def_id: DefID, value: Constant },

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
        callee: HIRFunctionRef,
        args: Vec<HIRExpr>,
        generic_args: Vec<Type>,
        trait_dispatch: Option<DefID>,
    },

    IntrinsicCall {
        callee: HIRFunctionRef,
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
        local: HIRLocal,
        init: Option<HIRExpr>,
        has_type_annotation: bool,
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
    Numeric,
    Pointer,
    PointerToInteger,
    IntegerToPointer,
    NoOp,
}
