use super::super::Analyzer;

use ir::hir::{CastKind, HIRExpr, HIRExprKind};
use ir::types::Type as IRType;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn is_int_type(&self, ty: &IRType) -> bool {
        matches!(
            ty, 
            IRType::I8 | IRType::I16 | IRType::I32 | IRType::I64 | IRType::ISIZE | 
            IRType::U8 | IRType::U16 | IRType::U32 | IRType::U64 | IRType::USIZE
        )
    }

    pub(crate) fn is_float_type(&self, ty: &IRType) -> bool {
        matches!(ty, IRType::F32 | IRType::F64)
    }

    pub(crate) fn coerce_primitive(&self, mut expr: HIRExpr, target: &IRType) -> HIRExpr {
        if expr.ty == *target { return expr; }

        if let HIRExprKind::IntLiteral(val) = expr.kind {
            if self.check_and_promote_int_literal(val, target) {
                expr.ty = target.clone();
                return expr;
            }
        }

        let from_size = self.get_type_size(&expr.ty).unwrap_or(8);
        let to_size = self.get_type_size(target).unwrap_or(8);

        let safe_to_cast = if self.is_float_type(&expr.ty) && self.is_float_type(target) {
            from_size <= to_size 
        } else if self.is_int_type(&expr.ty) && self.is_int_type(target) {
            from_size <= to_size 
        } else if self.is_int_type(&expr.ty) && self.is_float_type(target) {
            true 
        } else {
            false
        };
        
        if safe_to_cast {
            let span = expr.span;
            HIRExpr {
                kind: HIRExprKind::Cast { expr: Box::new(expr), kind: CastKind::Numeric },
                ty: target.clone(),
                span
            }
        } else {
            expr 
        }
    }
}
