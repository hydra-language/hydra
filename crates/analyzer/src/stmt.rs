use super::Analyzer;
use errors::error::HydraError;
use parser::ast::Stmt as ASTStmt;
use ir::types::Type as IRType;
use ir::context::DefKind;
use ir::hir::{HIRStmt, HIRExpr, HIRExprKind, HIRBlock}; 

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_block(&mut self, block: &parser::ast::Block) -> Result<HIRBlock, HydraError> {
        let mut stmts = Vec::new();
        for stmt in &block.statements {
            stmts.push(self.lower_stmt(stmt)?);
        }
        Ok(HIRBlock { stmts, span: crate::utils::get_stmt_span(block.statements.first().unwrap()) }) // approx span
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
                }

                let final_type = expected.unwrap_or(init_expr.ty.clone());

                let def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", format!("resolution failed for `{}`", name.lexeme), name.span))?;

                let mut info = self.context.get_def(def_id).unwrap().clone();
                if *is_const {
                    info.kind = DefKind::Constant {
                        ty: final_type.clone(),
                        value: ir::Constant::Float(0.0, final_type.clone()),
                    };
                } else {
                    info.kind = DefKind::Variable { ty: final_type.clone(), is_mutable: true };
                }
                self.context.update_def(def_id, info);
                
                Ok(HIRStmt::VarDecl { def_id, init: Some(init_expr), span: name.span })
            },

            ASTStmt::Expr(expr) => {
                let ir_expr = self.lower_expr(expr)?;
                Ok(HIRStmt::Expr(ir_expr))
            },

            ASTStmt::Return { value, .. } => {
                if let Some(v) = value {
                    let mut val = self.lower_expr(v)?;
                    if let Some(expected) = &self.current_return_type {
                        val = self.coerce_primitive(val, expected);             
                    }

                    Ok(HIRStmt::Expr(HIRExpr {
                        kind: HIRExprKind::Return(Some(Box::new(val))),
                        ty: IRType::VOID,
                        span
                    }))
                } else {
                    Ok(HIRStmt::Expr(HIRExpr {
                        kind: HIRExprKind::Return(None),
                        ty: IRType::VOID,
                        span
                    }))
                }
            },

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
}
