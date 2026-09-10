use super::Analyzer;

use errors::error::HydraError;
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::Type as IRType;
use parser::ast::Expr as ASTExpr;

mod aggregates;
mod calls;
mod coercion;
mod flow;
mod methods;
mod operators;
mod places;
mod values;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_expr(&mut self, node: &ASTExpr) -> Result<HIRExpr, HydraError> {
        self.lower_expr_with_type(node, None)
    }

    pub(crate) fn lower_expr_with_type(&mut self, node: &ASTExpr, expected: Option<&IRType>) -> Result<HIRExpr, HydraError> 
    {
        let span = crate::utils::get_expr_span(node);

        match node {
            ASTExpr::Literal { .. } | ASTExpr::Variable { .. } | ASTExpr::Path { .. } => {
                self.lower_value_expr(node, expected, span)
            }

            ASTExpr::ArrayInitializer { .. } | ASTExpr::SliceInitializer { .. } | 
            ASTExpr::ArrayAccess { .. } | ASTExpr::StructInitializer { .. } => {
                self.lower_aggregate_expr(node, expected, span)
            }

            ASTExpr::Binary { .. } | ASTExpr::Unary { .. } | ASTExpr::Cast { .. } => {
                self.lower_operator_expr(node, expected, span)
            }

            ASTExpr::FunctionCall { .. } => {
                self.lower_call_expr(node, expected, span)
            }

            ASTExpr::MethodCall { .. } => {
                self.lower_method_expr(node, expected, span)
            }

            ASTExpr::Assignment { .. } | ASTExpr::Member { .. } | ASTExpr::Borrow { .. } | ASTExpr::Dereference { .. } => {
                self.lower_place_expr(node, expected, span)
            }

            ASTExpr::If { .. } | ASTExpr::While { .. } | ASTExpr::For { .. } | ASTExpr::ForEach { .. } => {
                self.lower_control_flow_expr(node, expected, span)
            }

            _ => Err(self.error("S006", format!("expression not supported: {:?}", node), span)),
        }
    }
}
