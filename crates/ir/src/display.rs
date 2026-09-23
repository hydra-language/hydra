use std::fmt;

use crate::hir::{
    HIRFunction,
    HIRBlock,
    HIRStmt,
    HIRBinOp,
    HIRExpr,
    HIRExprKind,
    HIRUnaryOp,
};

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
            HIRStmt::VarDecl { def_id, init, .. } => {
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
            HIRExprKind::NONE => write!(f, "None"),
            HIRExprKind::OptionalPresent(value) => {
                write!(f, "present({})", value)
            }
            
            HIRExprKind::OrElse { optional, fallback } => {
                write!(f, "({} orelse {})", optional, fallback)
            }
            
            HIRExprKind::StructInit { def_id, values } => {
                write!(f, "{} {{ ", def_id)?;
                for (i, val) in values.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", val)?;
                }
                write!(f, " }}")
            }

            HIRExprKind::ArrayInit { elements } => {
                write!(f, "{{")?;
                for (i, val) in elements.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", val)?;
                }
                write!(f, "}}")
            }

            HIRExprKind::SliceInit { elements } => {
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

            HIRExprKind::IntrinsicCall { callee: _, kind, args, type_args } => {
                write!(f, "intrinsic {:?}", kind)?;

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
