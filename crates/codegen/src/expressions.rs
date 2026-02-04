use inkwell::values::BasicValueEnum;
use ir::expr::{Expr, ExprKind, BinaryOp};
use crate::CodeGen;

impl<'c> CodeGen<'c> {
    
    pub fn compile_expr(&mut self, expr: &Expr) -> Result<BasicValueEnum<'c>, String> {
        match &expr.kind {
            ExprKind::INT_LITERAL(val) => {
                Ok(self.context.i32_type().const_int(*val as u64, false).into())
            },

            ExprKind::VariableReference { name } => {
                let ptr = self.variables.get(name)
                    .expect(&format!("ICE: analyzer failed to validate variable '{}'", name));

                Ok(self.builder.build_load(*ptr, name).into())
            },

            ExprKind::Binary { op, lhs, rhs } => {
                let left = self.compile_expr(lhs)?;
                let right = self.compile_expr(rhs)?;

                let lhs_val = left.into_int_value();
                let rhs_val = right.into_int_value();

                let result = match op {
                    BinaryOp::ADD => self.builder.build_int_add(lhs_val, rhs_val, "tmpadd"),
                    BinaryOp::SUB => self.builder.build_int_sub(lhs_val, rhs_val, "tmpsub"),
                    BinaryOp::MUL => self.builder.build_int_mul(lhs_val, rhs_val, "tmpmul"),
                    BinaryOp::DIV => self.builder.build_int_signed_div(lhs_val, rhs_val, "tmpdiv"),

                    _ => return Err("binary op not yet supported".to_string())
                };

                Ok(result.into())
            },

            ExprKind::Call { callee, args } => {
                if callee == "println" {
                    return self.compile_println(args);
                }

                let func_value = self.module.get_function(callee)
                    .ok_or(format!("ICE: function '{}' not found in module", callee))?;

                let mut compiled_args = Vec::with_capacity(args.len());
                for arg in args {
                    compiled_args.push(self.compile_expr(arg)?.into());
                }

                let call_site = self.builder.build_call(func_value, &compiled_args, "tmpcall");

                match call_site.try_as_basic_value() {
                    item if item.is_left() => Ok(item.left().unwrap()),
                    _ => Ok(self.context.i32_type().const_zero().into()),
                }
            }

            _ => unimplemented!()
        }
    }
}
