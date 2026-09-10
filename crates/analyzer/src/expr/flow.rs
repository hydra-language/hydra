use super::super::Analyzer;

use errors::error::{HydraError, Span};

use ir::context::{DefKind, SymbolInfo};
use ir::hir::{HIRBinOp, HIRBlock, HIRExpr, HIRExprKind, HIRStmt, HIRUnaryOp};
use ir::types::Type as IRType;

use parser::ast::Expr as ASTExpr;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_control_flow_expr(&mut self, node: &ASTExpr, _expected: Option<&IRType>, span: Span)
        -> Result<HIRExpr, HydraError> 
    {
        match node {

            ASTExpr::If { condition, then_branch, else_branch, .. } => {
                let cond = self.lower_expr(condition)?;
                let then_block = self.lower_block(then_branch)?;
                let else_block = if let Some(eb) = else_branch { Some(Box::new(self.lower_block(eb)?)) } else { None };

                Ok(HIRExpr {
                    kind: HIRExprKind::If { cond: Box::new(cond), then_block: Box::new(then_block), else_block },
                    ty: IRType::VOID,
                    span
                })
            },

            ASTExpr::While { condition, body, .. } => {
                let cond_expr = self.lower_expr(condition)?;
                
                let break_expr = HIRExpr { kind: HIRExprKind::Break, ty: IRType::VOID, span };
                let not_cond = HIRExpr {
                    kind: HIRExprKind::Unary { op: HIRUnaryOp::Not, operand: Box::new(cond_expr) },
                    ty: IRType::BOOL,
                    span
                };
                
                let break_if = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(not_cond),
                        then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(break_expr)], span }),
                        else_block: None,
                    },
                    ty: IRType::VOID,
                    span,
                };

                let mut loop_stmts = vec![HIRStmt::Expr(break_if)];
                loop_stmts.extend(self.lower_block(body)?.stmts);

                Ok(HIRExpr {
                    kind: HIRExprKind::Loop(Box::new(HIRBlock { stmts: loop_stmts, span })),
                    ty: IRType::VOID,
                    span
                })
            },

            ASTExpr::For { id, variable, start, end, is_inclusive, body } => {
                let start_expr = self.lower_expr(start)?;
                let end_expr = self.lower_expr(end)?;

                let var_def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", "loop variable definition not found", variable.span))?;
                
                let mut info = self.context.get_def(var_def_id).unwrap().clone();
                info.kind = DefKind::Variable { ty: start_expr.ty.clone(), is_mutable: true };
                self.context.update_def(var_def_id, info);

                let init_stmt = HIRStmt::VarDecl { 
                    def_id: var_def_id, 
                    init: Some(start_expr.clone()), 
                    has_type_annotation: false,
                    span: variable.span 
                };

                let op = if *is_inclusive { HIRBinOp::Gt } else { HIRBinOp::Ge };
                let check_cond = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op,
                        lhs: Box::new(HIRExpr { kind: HIRExprKind::VarRef(var_def_id), ty: start_expr.ty.clone(), span: variable.span }),
                        rhs: Box::new(end_expr)
                    },
                    ty: IRType::BOOL,
                    span: variable.span
                };
                let break_if = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(check_cond),
                        then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(HIRExpr { kind: HIRExprKind::Break, ty: IRType::VOID, span })], span }),
                        else_block: None,
                    },
                    ty: IRType::VOID,
                    span,
                };

                let mut loop_stmts = vec![HIRStmt::Expr(break_if)];
                loop_stmts.extend(self.lower_block(body)?.stmts);

                let increment_expr = HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(HIRExpr { kind: HIRExprKind::VarRef(var_def_id), ty: start_expr.ty.clone(), span: variable.span }),
                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Add,
                                lhs: Box::new(HIRExpr { kind: HIRExprKind::VarRef(var_def_id), ty: start_expr.ty.clone(), span: variable.span }),
                                rhs: Box::new(HIRExpr { kind: HIRExprKind::IntLiteral(1), ty: start_expr.ty.clone(), span: variable.span })
                            },
                            ty: start_expr.ty.clone(),
                            span: variable.span
                        })
                    },
                    ty: start_expr.ty,
                    span: variable.span
                };
                loop_stmts.push(HIRStmt::Expr(increment_expr));

                let loop_expr = HIRExpr {
                    kind: HIRExprKind::Loop(Box::new(HIRBlock { stmts: loop_stmts, span })),
                    ty: IRType::VOID,
                    span
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Block(HIRBlock { stmts: vec![init_stmt, HIRStmt::Expr(loop_expr)], span }),
                    ty: IRType::VOID,
                    span
                })
            },

            ASTExpr::ForEach { id, item, iterable, body } => {
                let iter_expr = self.lower_expr(iterable)?;
                let (inner_ty, array_len) = match &iter_expr.ty {
                    IRType::ARRAY(inner, size) => (*inner.clone(), *size as i64),
                    _ => return Err(self.error("S014", "foreach requires an array", item.span)),
                };

                // Create hidden array def
                let arr_name = format!("_iter_arr_{}", item.span.line);

                let arr_info = SymbolInfo { 
                    name: arr_name.clone(), 
                    span: item.span, 
                    absolute_path: vec![arr_name.clone()], 
                    kind: DefKind::Variable { 
                        ty: iter_expr.ty.clone(), 
                        is_mutable: false 
                    }, 
                    is_pub: false 
                };

                let arr_def = self.context.insert_def(arr_info);
                let init_arr = HIRStmt::VarDecl { 
                    def_id: arr_def, 
                    init: Some(iter_expr.clone()), 
                    has_type_annotation: false,
                    span: item.span 
                };

                // Create index def
                let idx_name = format!("_idx_{}", item.span.line);

                let idx_info = SymbolInfo { 
                    name: idx_name.clone(), 
                    span: item.span, 
                    absolute_path: vec![idx_name.clone()], 
                    kind: DefKind::Variable { 
                        ty: IRType::I32, 
                        is_mutable: true 
                    }, 
                    is_pub: false 
                };

                let idx_def = self.context.insert_def(idx_info);
                let init_idx = HIRStmt::VarDecl { 
                    def_id: idx_def, 
                    init: Some(HIRExpr { 
                        kind: HIRExprKind::IntLiteral(0), 
                        ty: IRType::I32, 
                        span: item.span 
                    }), 
                    has_type_annotation: false,
                    span: item.span 
                };

                let mut loop_stmts = Vec::new();

                let break_cond = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: HIRBinOp::Ge,
                        lhs: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: IRType::I32, span: item.span }),
                        rhs: Box::new(HIRExpr { kind: HIRExprKind::IntLiteral(array_len), ty: IRType::I32, span: item.span })
                    },
                    ty: IRType::BOOL,
                    span: item.span
                };

                let break_if = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(break_cond),
                        then_block: Box::new(HIRBlock { 
                            stmts: vec![HIRStmt::Expr(HIRExpr { 
                                kind: HIRExprKind::Break, 
                                ty: IRType::VOID, 
                                span: item.span 
                            })], 
                            span: item.span 
                        }),

                        else_block: None
                    },
                    ty: IRType::VOID,
                    span: item.span
                };
                loop_stmts.push(HIRStmt::Expr(break_if));

                let item_def = self.name_resolver.get_resolution(*id).unwrap();
                let mut item_info = self.context.get_def(item_def).unwrap().clone();
                item_info.kind = DefKind::Variable { ty: inner_ty.clone(), is_mutable: false };
                self.context.update_def(item_def, item_info);

                let init_item = HIRStmt::VarDecl {
                    def_id: item_def,
                    init: Some(HIRExpr {
                        kind: HIRExprKind::ArrayAccess {
                            array: Box::new(HIRExpr { kind: HIRExprKind::VarRef(arr_def), ty: iter_expr.ty, span: item.span }),
                            index: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: IRType::I32, span: item.span })
                        },
                        ty: inner_ty.clone(),
                        span: item.span
                    }),
                    has_type_annotation: false,
                    span: item.span
                };
                loop_stmts.push(init_item);

                loop_stmts.extend(self.lower_block(body)?.stmts);

                let inc_idx = HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: IRType::I32, span: item.span }),
                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Add,
                                lhs: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: IRType::I32, span: item.span }),
                                rhs: Box::new(HIRExpr { kind: HIRExprKind::IntLiteral(1), ty: IRType::I32, span: item.span })
                            },
                            ty: IRType::I32,
                            span: item.span
                        })
                    },
                    ty: IRType::I32,
                    span: item.span
                };
                loop_stmts.push(HIRStmt::Expr(inc_idx));

                let loop_expr = HIRExpr {
                    kind: HIRExprKind::Loop(Box::new(HIRBlock { stmts: loop_stmts, span: item.span })),
                    ty: IRType::VOID,
                    span: item.span
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Block(HIRBlock { stmts: vec![init_arr, init_idx, HIRStmt::Expr(loop_expr)], span: item.span }),
                    ty: IRType::VOID,
                    span: item.span
                })
            },
            
            _ => unreachable!()
        }
    }
}
