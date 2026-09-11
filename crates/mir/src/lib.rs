pub mod builder;
pub mod display;
pub mod optimizer;


use errors::error;
use ir::types::Type;
use ir::context::DefID;
use ir::hir::{HIRBinOp, HIRUnaryOp, CastKind};
use ir::Constant;
use ir::intrinsic::IntrinsicKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BasicBlockID(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalID(pub usize); // _0 is return value, _1.._n are args/vars/temporaries

#[derive(Debug, Clone)]
pub struct MIRProgram {
    pub functions: Vec<MIRFunction>,
    // Structs and Globals pass through largely unchanged from HIR
}

#[derive(Debug, Clone)]
pub struct MIRFunction {
    pub name: String,
    pub def_id: DefID,
    pub return_type: Type,
    pub arg_count: usize,
    pub locals: Vec<LocalDecl>,
    pub basic_blocks: Vec<BasicBlock>,
    pub is_inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionRef {
    pub def_id: DefID,
    pub symbol: String,
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub ty: Type,
    pub is_mutable: bool,
    pub debug_def_id: Option<DefID>, // Optional: Keep track of which DefID this maps to for debugging/diagnostics
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

// A Place represents a location in memory (an l-value)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub local: LocalID,
    pub projection: Vec<ProjectionElem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionElem {
    Deref,
    Field(usize),
    Index(LocalID), // e.g., arr[i] where `i` is a local
}

// Statements execute sequentially and alter memory/locals
#[derive(Debug, Clone)]
pub enum StatementKind {
    Assign(Place, Rvalue),
    Drop(Place),
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: error::Span,
}

// Rvalues (Right-values) are operations that compute a value
#[derive(Debug, Clone)]
pub enum Rvalue {
    Use(Operand), // just reading a value
    Ref(bool, Place),   // &place
    
    /// constructs a fat slice reference from backing storage.
    ///
    /// Runtime representation is conceptually:
    ///
    ///     { ptr: *T, len: usize }
    ///
    SliceRef {
        is_mut: bool,
        place: Place,
        len: usize,
        element_ty: Type,
    },

    BinaryOp(HIRBinOp, Operand, Operand),
    UnaryOp(HIRUnaryOp, Operand),
    Cast(CastKind, Operand, Type),
    
    // Arrays and Structs
    Aggregate(AggregateKind, Vec<Operand>),
    Intrinsic {
        callee: FunctionRef,
        kind: IntrinsicKind,
        type_args: Vec<Type>,
        args: Vec<Operand>,
    },
}

#[derive(Debug, Clone)]
pub enum AggregateKind {
    Array(Type),
    Struct(DefID),
}

#[derive(Debug, Clone)]
pub enum Operand {
    Copy(Place),
    Move(Place),
    Const(Constant), // Use the fully qualified name to be safe
}

// Terminators define how control flow leaves a basic block
#[derive(Debug, Clone)]
pub enum Terminator {
    Goto { target: BasicBlockID },
    
    // A conditional branch. (Rustc calls this SwitchInt because it switches on a boolean/int)
    SwitchInt {
        discriminant: Operand,
        true_target: BasicBlockID,
        false_target: BasicBlockID,
    },
    
    // Function calls are terminators in MIR! 
    // This allows for explicit unwinding/panic handling later.
    Call {
        callee: FunctionRef,
        args: Vec<Operand>,
        destination: Place,     // Where the return value gets stored
        target: BasicBlockID,   // Where to go after the call finishes
    },

    // Builtin calls (print, println)
    BuiltinCall {
        name: String,
        args: Vec<Operand>,
        target: BasicBlockID,
    },
    
    Return,
    Unreachable, // Used after infinite loops or panics
}

