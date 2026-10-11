use std::fmt;

use crate::{
    Constant, Function, Program,
    context::DefID,
    hir::{
        HIRBinOp,
        HIRBlock,
        HIRExpr,
        HIRExprKind,
        HIRFunction,
        HIRProgram,
        HIRStmt,
        HIRUnaryOp,
    },
    types::Type,
};

pub struct HIRProgramDisplay<'a> {
    program: &'a HIRProgram,
}

pub struct HIRFunctionDisplay<'a> {
    function: &'a HIRFunction,
}

impl HIRProgram {

    pub fn display(&self) -> HIRProgramDisplay<'_> {
        HIRProgramDisplay {
            program: self,
        }
    }
}

impl HIRFunction {

    pub fn display(&self) -> HIRFunctionDisplay<'_> {
        HIRFunctionDisplay {
            function: self,
        }
    }
}

struct HIRPrinter;

impl HIRPrinter {

    fn function_name<'a>(&self, function: &'a HIRFunction) -> &'a str {
        function.display_name.as_deref().unwrap_or(&function.name)
    }

    fn local_name(&self, def_id: DefID) -> String {
        def_id.to_string()
    }

    fn write_indent(&self, out: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        for _ in 0..indent {
            write!(out, "    ")?;
        }

        Ok(())
    }

    fn fmt_function(&self, out: &mut fmt::Formatter<'_>, function: &HIRFunction) -> fmt::Result {
        write!(out, "fn {}(", self.function_name(function))?;

        for (index, param) in function.params.iter().enumerate() {
            if index > 0 {
                write!(out, ", ")?;
            }

            write!(out, "{}: {}", param.name, param.ty)?;
        }

        write!(out, ") -> {} ", function.return_type)?;
        self.fmt_block(out, &function.body, 0)
    }

    fn fmt_block(&self, out: &mut fmt::Formatter<'_>, block: &HIRBlock, indent: usize) -> fmt::Result {
        writeln!(out, "{{")?;

        for stmt in &block.stmts {
            self.write_indent(out, indent + 1)?;
            self.fmt_stmt(out, stmt, indent + 1)?;
            writeln!(out)?;
        }

        self.write_indent(out, indent)?;
        write!(out, "}}")
    }

    fn fmt_stmt(&self, out: &mut fmt::Formatter<'_>, stmt: &HIRStmt, indent: usize) -> fmt::Result {
        match stmt {
            HIRStmt::VarDecl { local, init, .. } => {
                write!(out, "let {}: {}", local.name, local.ty)?;

                if let Some(init) = init {
                    write!(out, " = ")?;
                    self.fmt_expr(out, init, indent, false)?;
                }

                write!(out, ";")
            }

            HIRStmt::Expr(expr) => {
                self.fmt_expr(out, expr, indent, false)?;
                write!(out, ";")
            }
        }
    }

    fn fmt_expr(&self, out: &mut fmt::Formatter<'_>, expr: &HIRExpr, indent: usize, include_type: bool) -> fmt::Result {
        self.fmt_expr_kind(out, &expr.kind, Some(&expr.ty), indent)?;

        if include_type && !matches!(&expr.ty, Type::VOID) && !matches!(&expr.kind, HIRExprKind::Cast { .. }) {
            write!(out, ": {}", expr.ty)?;
        }

        Ok(())
    }

    fn fmt_expr_kind(&self, out: &mut fmt::Formatter<'_>, kind: &HIRExprKind, parent_ty: Option<&Type>, indent: usize) -> fmt::Result {
        match kind {
            HIRExprKind::VarRef(def_id) => {
                write!(out, "{}", self.local_name(*def_id))
            }

            HIRExprKind::ConstRef { def_id, .. } => {
                write!(out, "{}", self.local_name(*def_id))
            }

            HIRExprKind::IntLiteral(value) => write!(out, "{}", value),
            HIRExprKind::FloatLiteral(value) => write!(out, "{}", value),
            HIRExprKind::StringLiteral(value) => write!(out, "{:?}", value),
            HIRExprKind::CharLiteral(value) => write!(out, "{:?}", value),
            HIRExprKind::BoolLiteral(value) => write!(out, "{}", value),
            HIRExprKind::NONE => write!(out, "None"),

            HIRExprKind::OptionalPresent(value) => {
                write!(out, "present(")?;
                self.fmt_expr(out, value, indent, true)?;
                write!(out, ")")
            }

            HIRExprKind::OrElse { optional, fallback } => {
                write!(out, "(")?;
                self.fmt_expr(out, optional, indent, true)?;
                write!(out, " orelse ")?;
                self.fmt_expr(out, fallback, indent, true)?;
                write!(out, ")")
            }

            HIRExprKind::StructInit { values, .. } => {
                let name = match parent_ty {
                    Some(Type::STRUCT(type_ref))
                    | Some(Type::GENERIC_INSTANCE(type_ref, _)) => {
                        type_ref.display_symbol
                            .as_deref()
                            .unwrap_or(&type_ref.symbol)
                    }

                    _ => "<struct>",
                };

                write!(out, "{} {{", name)?;

                if !values.is_empty() {
                    write!(out, " ")?;

                    for (index, value) in values.iter().enumerate() {
                        if index > 0 {
                            write!(out, ", ")?;
                        }

                        write!(out, ".{} = ", index)?;
                        self.fmt_expr(out, value, indent, true)?;
                    }

                    write!(out, " ")?;
                }

                write!(out, "}}")
            }

            HIRExprKind::ArrayInit { elements } => {
                write!(out, "{{")?;

                for (index, value) in elements.iter().enumerate() {
                    if index > 0 {
                        write!(out, ", ")?;
                    }

                    self.fmt_expr(out, value, indent, true)?;
                }

                write!(out, "}}")
            }

            HIRExprKind::SliceInit { elements } => {
                write!(out, "[")?;

                for (index, value) in elements.iter().enumerate() {
                    if index > 0 {
                        write!(out, ", ")?;
                    }

                    self.fmt_expr(out, value, indent, true)?;
                }

                write!(out, "]")
            }

            HIRExprKind::ArrayAccess { array, index } => {
                self.fmt_expr(out, array, indent, false)?;
                write!(out, "[")?;
                self.fmt_expr(out, index, indent, false)?;
                write!(out, "]")
            }

            HIRExprKind::FieldAccess { object, field_index } => {
                self.fmt_expr(out, object, indent, false)?;
                write!(out, ".{}", field_index)
            }

            HIRExprKind::Call { callee, args, .. } => {
                let name = callee.display_symbol.as_deref().unwrap_or(&callee.symbol);

                write!(out, "call {}(", name)?;

                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        write!(out, ", ")?;
                    }

                    self.fmt_expr(out, arg, indent, true)?;
                }

                write!(out, ")")
            }

            HIRExprKind::IntrinsicCall { kind, args, type_args, .. } => {
                write!(out, "intrinsic {:?}", kind)?;

                if !type_args.is_empty() {
                    write!(out, "<")?;

                    for (index, ty) in type_args.iter().enumerate() {
                        if index > 0 {
                            write!(out, ", ")?;
                        }

                        write!(out, "{}", ty)?;
                    }

                    write!(out, ">")?;
                }

                write!(out, "(")?;

                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        write!(out, ", ")?;
                    }

                    self.fmt_expr(out, arg, indent, true)?;
                }

                write!(out, ")")
            }

            HIRExprKind::Binary { op, lhs, rhs } => {
                write!(out, "(")?;
                self.fmt_expr(out, lhs, indent, true)?;
                write!(out, " {} ", op)?;
                self.fmt_expr(out, rhs, indent, true)?;
                write!(out, ")")
            }

            HIRExprKind::Unary { op, operand } => {
                write!(out, "{}", op)?;
                self.fmt_expr(out, operand, indent, false)
            }

            HIRExprKind::Cast { expr, kind } => {
                write!(out, "(")?;
                self.fmt_expr(out, expr, indent, false)?;

                if let Some(target_ty) = parent_ty {
                    write!(out, " as {}", target_ty)?;
                } else {
                    write!(out, " as {:?}", kind)?;
                }

                write!(out, ")")
            }

            HIRExprKind::Assign { target, value } => {
                self.fmt_expr(out, target, indent, false)?;
                write!(out, " = ")?;
                self.fmt_expr(out, value, indent, true)
            }

            HIRExprKind::Borrow { is_mut, target } => {
                if *is_mut {
                    write!(out, "&mut ")?;
                } else {
                    write!(out, "&")?;
                }

                self.fmt_expr(out, target, indent, false)
            }

            HIRExprKind::Dereference { target } => {
                write!(out, "*")?;
                self.fmt_expr(out, target, indent, false)
            }

            HIRExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                write!(out, "if ")?;
                self.fmt_expr(out, cond, indent, true)?;
                write!(out, " ")?;
                self.fmt_block(out, then_block, indent)?;

                if let Some(else_block) = else_block {
                    write!(out, " else ")?;
                    self.fmt_block(out, else_block, indent)?;
                }

                Ok(())
            }

            HIRExprKind::Loop(block) => {
                write!(out, "loop ")?;
                self.fmt_block(out, block, indent)
            }

            HIRExprKind::Break => write!(out, "break"),
            HIRExprKind::Continue => write!(out, "continue"),

            HIRExprKind::Return(Some(expr)) => {
                write!(out, "return ")?;
                self.fmt_expr(out, expr, indent, true)
            }

            HIRExprKind::Return(None) => write!(out, "return"),

            HIRExprKind::Block(block) => {
                self.fmt_block(out, block, indent)
            }
        }
    }
}

impl fmt::Display for HIRProgramDisplay<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printer = HIRPrinter;

        for (index, function) in self.program.functions.iter().enumerate() {
            if index > 0 {
                write!(out, "\n\n")?;
            }

            printer.fmt_function(out, function)?;
        }

        Ok(())
    }
}

impl fmt::Display for HIRFunctionDisplay<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        HIRPrinter.fmt_function(out, self.function)
    }
}

impl fmt::Display for HIRFunction {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        HIRPrinter.fmt_function(out, self)
    }
}

impl fmt::Display for HIRBlock {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        HIRPrinter.fmt_block(out, self, 0)
    }
}

impl fmt::Display for HIRStmt {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        HIRPrinter.fmt_stmt(out, self, 0)
    }
}

impl fmt::Display for HIRExpr {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        HIRPrinter.fmt_expr(out, self, 0, true)
    }
}

impl fmt::Display for HIRExprKind {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        HIRPrinter.fmt_expr_kind(out, self, None, 0)
    }
}

impl fmt::Display for HIRBinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            HIRBinOp::Add => "+",
            HIRBinOp::Sub => "-",
            HIRBinOp::Mul => "*",
            HIRBinOp::Div => "/",
            HIRBinOp::Mod => "%",
            HIRBinOp::Eq => "==",
            HIRBinOp::Ne => "!=",
            HIRBinOp::Lt => "<",
            HIRBinOp::Le => "<=",
            HIRBinOp::Gt => ">",
            HIRBinOp::Ge => ">=",
            HIRBinOp::And => "&&",
            HIRBinOp::Or => "||",
        };

        write!(f, "{}", op)
    }
}

impl fmt::Display for HIRUnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            HIRUnaryOp::Neg => "-",
            HIRUnaryOp::Not => "!",
            HIRUnaryOp::AddrOf => "&",
            HIRUnaryOp::Deref => "*",
        };

        write!(f, "{}", op)
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params_str = self.params.iter().map(|(n, t)| format!("{}: {}", n, t)).collect::<Vec<_>>().join(", ");

        write!(
            f,
            "fn {}({}) -> {} {}",
            self.name,
            params_str,
            self.return_type,
            self.body,
        )
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

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constant::Int(v, _) => write!(f, "{}", v),
            Constant::Float(v, _) => write!(f, "{}", v),
            Constant::Bool(v) => write!(f, "{}", v),
            Constant::Char(v) => write!(f, "'{:?}'", v),
            Constant::String(v) => write!(f, "{:?}", v),
        }
    }
}
