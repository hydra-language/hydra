use std::fmt;

use crate::{FunctionRef, MIRFunction, Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator};

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

impl fmt::Display for FunctionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol)
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

            Rvalue::SliceRef { is_mut, place, len, .. } => {
                if *is_mut {
                    write!(f, "&mut slice({}, len={})", place, len)
                } else {
                    write!(f, "&slice({}, len={})", place, len)
                }
            }

            Rvalue::BinaryOp(op, lhs, rhs) => write!(f, "{} {} {}", lhs, op, rhs),
            Rvalue::UnaryOp(op, operand) => write!(f, "{}{}", op, operand),
            Rvalue::Cast(_kind, operand, ty) => write!(f, "{} as {}", operand, ty),
            Rvalue::Aggregate(_, _) => write!(f, "aggregate(...)"),
            Rvalue::Intrinsic { callee, type_args, args, .. } => {
                write!(f, "{}", callee)?;

                if !type_args.is_empty() {
                    write!(f, "<")?;

                    for (i, ty) in type_args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }

                        write!(f, "{}", ty)?;
                    }

                    write!(f, ">")?;
                }

                write!(f, "(")?;

                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{}", arg)?;
                }

                write!(f, ")")
            }
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
                write!(f, "{} = call {}(", destination, callee.symbol)?;
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
