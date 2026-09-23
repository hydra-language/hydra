use super::Analyzer;

use errors::error::HydraError;
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::Type as IRType;
use lexer::TokenType;
use parser::ast::Expr as ASTExpr;

mod aggregates;
mod calls;
mod coercion;
mod flow;
mod methods;
mod operators;
mod places;
mod values;
mod format;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_expr(&mut self, node: &ASTExpr) -> Result<HIRExpr, HydraError> {
        self.lower_expr_with_type(node, None)
    }

    pub(crate) fn lower_expr_with_type(&mut self, node: &ASTExpr, expected: Option<&IRType>) -> Result<HIRExpr, HydraError> 
    {
        let span = crate::utils::get_expr_span(node);

        //
        // if the surrounding context expects T?, ordinary expressions should
        // first be analyzed as T and then promoted to a present optional
        //
        // two cases deliberately retain the complete optional expectation:
        //
        //     None
        //
        // needs T? in order to know which optional type it belongs to
        //
        //     if ...
        //
        // needs T? propagated into its branches because one branch may produce
        // T while another produces None
        //
        let lowering_expected = match (node, expected) {
            (ASTExpr::Literal { token, .. }, Some(IRType::OPTIONAL(_))) if matches!(token.token_type, TokenType::NONE) => expected,
            (ASTExpr::If { .. }, Some(IRType::OPTIONAL(_))) => expected,
            (_, Some(IRType::OPTIONAL(inner))) => Some(inner.as_ref()),

            _ => expected,
        };

        let mut expr = match node {
            ASTExpr::Literal { .. } | ASTExpr::Variable { .. } | ASTExpr::Path { .. } => {
                self.lower_value_expr(node, lowering_expected, span)
            }

            ASTExpr::ArrayInitializer { .. } | ASTExpr::SliceInitializer { .. } | 
            ASTExpr::ArrayAccess { .. } | ASTExpr::StructInitializer { .. } => {
                self.lower_aggregate_expr(node, lowering_expected, span)
            }

            ASTExpr::Binary { .. } | ASTExpr::Unary { .. } | ASTExpr::Cast { .. } => {
                self.lower_operator_expr(node, lowering_expected, span)
            }

            ASTExpr::FunctionCall { .. } => {
                self.lower_call_expr(node, lowering_expected, span)
            }

            ASTExpr::MethodCall { .. } => {
                self.lower_method_expr(node, lowering_expected, span)
            }

            ASTExpr::Assignment { .. } | ASTExpr::Member { .. } | ASTExpr::Borrow { .. } | ASTExpr::Dereference { .. } => {
                self.lower_place_expr(node, lowering_expected, span)
            }

            ASTExpr::If { .. } | ASTExpr::While { .. } | 
            ASTExpr::For { .. } | ASTExpr::ForEach { .. } | ASTExpr::OrElse { .. } => {
                self.lower_control_flow_expr(node, lowering_expected, span)
            }

            _ => Err(self.error("S006", format!("expression not supported: {:?}", node), span)),
        }?;

        let Some(target @ IRType::OPTIONAL(inner)) = expected else {
            return Ok(expr);
        };

        //
        // None and expressions that already produce T? need no promotion
        //
        if expr.ty == *target {
            return Ok(expr);
        }

        //
        // primitive coercion occurs before optional promotion:
        //
        //     const x: i64? = 10;
        //
        // becomes:
        //
        //     present(10: i64): i64?
        //
        expr = self.coerce_primitive(expr, inner);

        if !self.check_type_compatibility(inner, &expr.ty) {
            return Err(self.error(
                "S001",
                format!(
                    "type mismatch: expected {}, found {}",
                    target,
                    expr.ty,
                ),
                expr.span,
            ));
        }

        Ok(HIRExpr {
            kind: HIRExprKind::OptionalPresent(Box::new(expr)),
            ty: target.clone(),
            span,
        })
    }
}
