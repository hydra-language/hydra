use super::Analyzer;

use errors::error::{HydraError, Span};

use parser::ast::{Stmt as ASTStmt, Expr as ASTExpr};

use ir::types::Type as IRType;
use ir::context::DefKind;
use ir::hir::{HIRStmt, HIRExpr, HIRExprKind, HIRBlock}; 

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_block(&mut self, block: &parser::ast::Block) -> Result<HIRBlock, HydraError> {
        let mut stmts = Vec::new();

        for stmt in &block.statements {
            stmts.push(self.lower_stmt(stmt)?);
        }

        let span = block.statements.first()
            .map(crate::utils::get_stmt_span)
            .unwrap_or_default();

        Ok(HIRBlock { 
            stmts, 
            span  
        }) // approx span
    }

    pub(crate) fn lower_stmt(&mut self, stmt: &ASTStmt) -> Result<HIRStmt, HydraError> {
        let span = crate::utils::get_stmt_span(stmt);
        
        match stmt {
            ASTStmt::VariableDecl { id, is_const, name, type_annotation, initializer } => {
                let expected = if let Some(annotation) = type_annotation {
                    Some(self.lower_type(annotation)?)
                } else {
                    None
                };
                
                let mut init_expr = self.lower_expr_with_type(initializer, expected.as_ref())?;

                if let Some(ref target) = expected {
                    init_expr = self.coerce_primitive(init_expr, target);

                    if !self.check_type_compatibility(target, &init_expr.ty) {
                        return Err(self.error("S001", format!("type mismatch: expected {}, found {}", target, init_expr.ty), init_expr.span))
                    }
                }


                let final_type = expected.unwrap_or(init_expr.ty.clone());

                let def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", format!("resolution failed for `{}`", name.lexeme), name.span))?;

                let mut info = self.context.get_def(def_id).unwrap().clone();
                if *is_const {
                    info.kind = DefKind::Variable {
                        ty: final_type.clone(),
                        is_mutable: !*is_const,
                    }
                } else {
                    info.kind = DefKind::Variable { ty: final_type.clone(), is_mutable: true };
                }
                self.context.update_def(def_id, info);
                
                Ok(HIRStmt::VarDecl {
                    local: self.hir_local(def_id, final_type),
                    init: Some(init_expr),
                    has_type_annotation: type_annotation.is_some(),
                })
            },

            ASTStmt::Expr(expr) => {
                let ir_expr = self.lower_expr(expr)?;
                Ok(HIRStmt::Expr(ir_expr))
            },

            ASTStmt::Return { value, .. } => {
                Ok(HIRStmt::Expr(
                    self.lower_return_expr(
                        value.as_deref(),
                        span,
                    )?,
                ))
            }

            ASTStmt::Break { condition, .. } => {
                let break_expr = HIRExpr { kind: HIRExprKind::Break, ty: IRType::VOID, span };
                if let Some(cond_node) = condition {
                    let cond = self.lower_expr(cond_node)?;
                    Ok(HIRStmt::Expr(HIRExpr {
                        kind: HIRExprKind::If {
                            cond: Box::new(cond),
                            then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(break_expr)], span }),
                            else_block: None
                        },
                        ty: IRType::VOID,
                        span
                    }))
                } else {
                    Ok(HIRStmt::Expr(break_expr))
                }
            },

            ASTStmt::Continue { condition, .. } => {
                let cont_expr = HIRExpr { kind: HIRExprKind::Continue, ty: IRType::VOID, span };
                if let Some(cond_node) = condition {
                    let cond = self.lower_expr(cond_node)?;
                    Ok(HIRStmt::Expr(HIRExpr {
                        kind: HIRExprKind::If {
                            cond: Box::new(cond),
                            then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(cont_expr)], span }),
                            else_block: None
                        },
                        ty: IRType::VOID,
                        span
                    }))
                } else {
                    Ok(HIRStmt::Expr(cont_expr))
                }
            },
        }
    }

    pub(crate) fn lower_return_expr(&mut self, value: Option<&ASTExpr>, span: Span) -> Result<HIRExpr, HydraError> {
        let expected = self.current_return_type.clone().expect("ICE: return lowered outside of a function");

        let value = match value {
            Some(value) => {
                let mut value = self.lower_expr_with_type(
                    value,
                    Some(&expected),
                )?;

                if expected != IRType::VOID {
                    value = self.coerce_primitive(
                        value,
                        &expected,
                    );
                }

                if !self.check_type_compatibility(
                    &expected,
                    &value.ty,
                ) {
                    return Err(self.error(
                        "S001",
                        format!(
                            "type mismatch: expected '{}', found '{}'",
                            expected,
                            value.ty,
                        ),
                        value.span,
                    ));
                }

                Some(Box::new(value))
            }

            None => {
                if expected != IRType::VOID {
                    return Err(self.error(
                        "S001",
                        format!(
                            "type mismatch: expected '{}', found 'void'",
                            expected,
                        ),
                        span,
                    ));
                }

                None
            }
        };

        Ok(HIRExpr {
            kind: HIRExprKind::Return(value),
            ty: IRType::VOID,
            span,
        })
    }
}
