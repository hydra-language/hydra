pub mod builder;
pub mod optimizer;

use std::fmt;

use errors::error;
use ir::types::Type;
use ir::context::DefID;
use ir::hir::{HIRBinOp, HIRUnaryOp, CastKind};
use ir::Constant;

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
    Use(Operand), // Just reading a value
    Ref(bool, Place),   // &place
    BinaryOp(HIRBinOp, Operand, Operand),
    UnaryOp(HIRUnaryOp, Operand),
    Cast(CastKind, Operand, Type),
    
    // Arrays and Structs
    Aggregate(AggregateKind, Vec<Operand>),
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
        callee: String,
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

impl fmt::Display for MIRFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "fn {}() -> {} {{", self.name, self.return_type)?;

        // Print locals (Variables & Temporaries)
        for (i, local) in self.locals.iter().enumerate() {
            let mut_str = if local.is_mutable { "mut " } else { "" };
            let debug_str = if let Some(def) = local.debug_def_id {
                format!(" // {}", def)
            } else {
                String::new()
            };
            writeln!(f, "    let {}_{}: {};{}", mut_str, i, local.ty, debug_str)?;
        }
        writeln!(f)?;

        // Print Basic Blocks
        for (i, block) in self.basic_blocks.iter().enumerate() {
            writeln!(f, "    bb{}: {{", i)?;
            
            for stmt in &block.statements {
                writeln!(f, "        {};", stmt)?;
            }
            
            writeln!(f, "        {}", block.terminator)?;
            writeln!(f, "    }}\n")?;
        }

        writeln!(f, "}}")
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            StatementKind::Assign(place, rval) => write!(f, "{} = {}", place, rval),
            StatementKind::Drop(place) => write!(f, "drop({})", place),
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for proj in &self.projection {
            if let ProjectionElem::Deref = proj { write!(f, "*")?; }
        }

        write!(f, "_{}", self.local.0)?;

        for proj in &self.projection {
            match proj {
                ProjectionElem::Deref => {}
                ProjectionElem::Field(idx) => write!(f, ".{}", idx)?,
                ProjectionElem::Index(local) => write!(f, "[_{}]", local.0)?,
            }
        }

        Ok(())
    }
}

impl fmt::Display for ProjectionElem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectionElem::Deref => write!(f, "*"),
            ProjectionElem::Field(idx) => write!(f, ".{}", idx),
            ProjectionElem::Index(local) => write!(f, "[_{}]", local.0),
        }
    }
}

impl fmt::Display for Rvalue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rvalue::Use(op) => write!(f, "{}", op),
            Rvalue::Ref(is_mut, place) => {
                if *is_mut { write!(f, "&mut {}", place) }
                else { write!(f, "&{}", place) }
            },
            Rvalue::BinaryOp(op, lhs, rhs) => write!(f, "{} {} {}", lhs, op, rhs),
            Rvalue::UnaryOp(op, operand) => write!(f, "{}{}", op, operand),
            Rvalue::Cast(_kind, operand, ty) => write!(f, "{} as {}", operand, ty),
            Rvalue::Aggregate(_, _) => write!(f, "aggregate(...)"),
        }
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Copy(place) => write!(f, "{}", place),
            Operand::Move(place) => write!(f, "move {}", place),
            Operand::Const(c) => write!(f, "const {}", c),
        }
    }
}

impl fmt::Display for Terminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminator::Goto { target } => write!(f, "goto -> bb{}", target.0),
            Terminator::SwitchInt { discriminant, true_target, false_target } => {
                write!(f, "switch_int({}) -> [true: bb{}, false: bb{}]", discriminant, true_target.0, false_target.0)
            }
            Terminator::Call { callee, args, destination, target } => {
                write!(f, "{} = call {}(", destination, callee)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", arg)?;
                }
                write!(f, ") -> [return: bb{}]", target.0)
            }
            Terminator::BuiltinCall { name, args, target } => {
                write!(f, "builtin {}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", arg)?;
                }
                write!(f, ") -> [return: bb{}]", target.0)
            }
            Terminator::Return => write!(f, "return"),
            Terminator::Unreachable => write!(f, "unreachable"),
        }
    }
}
