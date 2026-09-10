use super::super::Analyzer;

use errors::error::{
    HydraError,
    Span,
};
use ir::hir::{
    CastKind,
    HIRBinOp,
    HIRExpr,
    HIRExprKind,
    HIRUnaryOp,
};
use ir::types::Type as IRType;
use lexer::TokenType;
use parser::ast::Expr as ASTExpr;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_operator_expr(&mut self, node: &ASTExpr, expected: Option<&IRType>, span: Span) 
        -> Result<HIRExpr, HydraError> 
    {
        match node {
            ASTExpr::Binary { left, operator, right, .. } => {
                let mut lhs = self.lower_expr_with_type(left, expected)?;
                let mut rhs = self.lower_expr_with_type(right, Some(&lhs.ty))?;

                if lhs.ty != rhs.ty {
                    let l_size = self.get_type_size(&lhs.ty).unwrap_or(0);
                    let r_size = self.get_type_size(&rhs.ty).unwrap_or(0);

                    if self.is_float_type(&rhs.ty) && self.is_int_type(&lhs.ty) || (l_size < r_size && !self.is_float_type(&lhs.ty)) {
                        lhs = self.coerce_primitive(lhs, &rhs.ty);
                    } else {
                        rhs = self.coerce_primitive(rhs, &lhs.ty);
                    }
                }

                let (op, ty) = match operator.token_type {
                    TokenType::Plus => (HIRBinOp::Add, lhs.ty.clone()),
                    TokenType::Minus => (HIRBinOp::Sub, lhs.ty.clone()),
                    TokenType::Star => (HIRBinOp::Mul, lhs.ty.clone()),
                    TokenType::ForwardSlash => (HIRBinOp::Div, lhs.ty.clone()),
                    TokenType::Modulo => (HIRBinOp::Mod, lhs.ty.clone()),
                    TokenType::LeftAngle => (HIRBinOp::Lt, IRType::BOOL),
                    TokenType::LessEqual => (HIRBinOp::Le, IRType::BOOL),
                    TokenType::RightAngle => (HIRBinOp::Gt, IRType::BOOL),
                    TokenType::GreaterEqual => (HIRBinOp::Ge, IRType::BOOL),
                    TokenType::DoubleEqual => (HIRBinOp::Eq, IRType::BOOL),
                    TokenType::ExclamEqual => (HIRBinOp::Ne, IRType::BOOL),
                    TokenType::DoubleAmpersand => (HIRBinOp::And, IRType::BOOL),
                    TokenType::DoublePipe => (HIRBinOp::Or,  IRType::BOOL),
                    _ => return Err(self.error("S003", format!("unknown op: {}", operator.lexeme), operator.span))
                };
                
                Ok(HIRExpr { 
                    kind: HIRExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, 
                    ty,
                    span
                })
            },

            ASTExpr::Unary { operator, right, .. } => {
                let rhs = self.lower_expr_with_type(right, expected)?;
                match operator.token_type {
                    TokenType::Minus => Ok(HIRExpr {
                        kind: HIRExprKind::Unary { op: HIRUnaryOp::Neg, operand: Box::new(rhs.clone()) },
                        ty: rhs.ty, span
                    }),

                    TokenType::ExclamationMark => Ok(HIRExpr {
                        kind: HIRExprKind::Unary { op: HIRUnaryOp::Not, operand: Box::new(rhs) },
                        ty: IRType::BOOL, span
                    }),

                    _ => Err(self.error("S003", format!("unknown unary op: {}", operator.lexeme), operator.span))
                }
            }

            ASTExpr::Cast { value, target, .. } => {
                let expr = self.lower_expr_with_type(value, None)?;
                let target_type = self.lower_type(target)?;

                let source_is_pointer_like = matches!(
                    &expr.ty,
                    IRType::REF(_)
                    | IRType::CONST_REF(_)
                    | IRType::POINTER(_)
                    | IRType::CONST_POINTER(_)
                );

                let target_is_raw_pointer = matches!(
                    &target_type,
                    IRType::POINTER(_)
                    | IRType::CONST_POINTER(_)
                );

                let cast_kind =
                if source_is_pointer_like && target_is_raw_pointer {
                    CastKind::Pointer
                } else {
                    CastKind::Numeric
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Cast {
                        expr: Box::new(expr),
                        kind: cast_kind,
                    },
                    ty: target_type,
                    span,
                })
            }

            _ => unreachable!()
        }
    }
}
