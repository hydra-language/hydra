use super::super::Analyzer;

use errors::error::{HydraError, Span};

use ir::context::{DefKind, SymbolInfo};
use ir::hir::{HIRBinOp, HIRBlock, HIRExpr, HIRExprKind, HIRStmt, HIRUnaryOp};
use ir::types::Type as IRType;

use parser::ast::{Expr as ASTExpr, Stmt as ASTStmt};

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_control_flow_expr(&mut self, node: &ASTExpr, expected: Option<&IRType>, span: Span)
        -> Result<HIRExpr, HydraError> 
    {
        match node {

            ASTExpr::If { condition, then_branch, else_branch, .. } => {
                let cond = self.lower_expr_with_type(condition, Some(&IRType::BOOL))?;

                if !self.check_type_compatibility(&IRType::BOOL, &cond.ty) {
                    return Err(self.error("S001", format!("if condition expected bool, found {}", cond.ty), cond.span));
                }

                //
                // without an expected type, we can still infer an
                // if-expression whenever both branches visibly end in
                // expression statements
                //
                let branches_produce_values = if let Some(else_branch) = else_branch {
                    matches!(then_branch.statements.last(), Some(ASTStmt::Expr(_))) &&
                    matches!(else_branch.statements.last(), Some(ASTStmt::Expr(_)))
                } else {
                    false
                };

                let is_value_if = expected.is_some() || branches_produce_values;

                //
                // ordinary statement-style if
                //
                if !is_value_if {
                    let then_block = self.lower_block(then_branch)?;
                    let else_block = if let Some(else_branch) = else_branch {
                        Some(Box::new(self.lower_block(else_branch)?))
                    } else {
                        None
                    };

                    return Ok(HIRExpr {
                        kind: HIRExprKind::If {
                            cond: Box::new(cond),
                            then_block: Box::new(then_block),
                            else_block,
                        },
                        ty: IRType::VOID,
                        span
                    });
                }

                //
                // a value-producing if must have an else branch because
                // every control-flow path must produce the result
                //
                let else_branch = else_branch.as_ref()
                    .ok_or_else(|| self.error("S001", "if expression requires an else branch", span))?;

                //
                // if an outer context supplies the expected type, use it
                // for both branches
                //
                // otherwise infer from the first branch and use that to
                // constrain/coerce the second branch
                //
                let (then_block, then_ty) = self.lower_value_block(then_branch, expected)?;
                let result_ty = expected.cloned().unwrap_or_else(|| then_ty.clone());

                let (else_block, else_ty) = self.lower_value_block(else_branch, Some(&result_ty))?;

                if !self.check_type_compatibility(&result_ty, &else_ty) {
                    return Err(self.error(
                        "S001",
                        format!("if branches have incompatible types: {} and {}", result_ty, else_ty),
                        span,
                    ));
                }
        

                Ok(HIRExpr {
                    kind: HIRExprKind::If { 
                        cond: Box::new(cond), 
                        then_block: Box::new(then_block), 
                        else_block: Some(Box::new(else_block)),
                    },
                    ty: result_ty,
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
                let mut end_expr = self.lower_expr_with_type(end, Some(&start_expr.ty))?;

                end_expr = self.coerce_primitive(end_expr, &start_expr.ty);

                let loop_ty = start_expr.ty.clone();

                let var_def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", "loop variable definition not found", variable.span))?;
                
                let mut info = self.context.get_def(var_def_id).unwrap().clone();
                info.kind = DefKind::Variable { ty: loop_ty.clone(), is_mutable: true };
                self.context.update_def(var_def_id, info);

                //
                // let i = start;
                //
                let init_stmt = HIRStmt::VarDecl { 
                    def_id: var_def_id, 
                    ty: loop_ty.clone(),
                    init: Some(start_expr.clone()), 
                    has_type_annotation: false,
                    span: variable.span 
 
                };
                //
                // const _for_end = end;
                //
                // evaluate the ending bound exactly once.
                //
                let end_name = format!("_for_end_{}", variable.span.line);

                let end_def = self.context.insert_def(SymbolInfo {
                    name: end_name.clone(),
                    span: variable.span,
                    absolute_path: vec![end_name],
                    kind: DefKind::Variable {
                        ty: loop_ty.clone(),
                        is_mutable: false,
                    },
                    is_pub: false,
                });

                let init_end = HIRStmt::VarDecl {
                    def_id: end_def,
                    ty: loop_ty.clone(),
                    init: Some(end_expr),
                    has_type_annotation: false,
                    span: variable.span,
                };

                //
                // const _for_descending = i > _for_end;
                //
                // direction is determined once before iteration begins.
                //
                let descending_name = format!("_for_descending_{}", variable.span.line);

                let descending_def = self.context.insert_def(SymbolInfo {
                    name: descending_name.clone(),
                    span: variable.span,
                    absolute_path: vec![descending_name],
                    kind: DefKind::Variable {
                        ty: IRType::BOOL,
                        is_mutable: false,
                    },
                    is_pub: false,
                });

                let init_descending = HIRStmt::VarDecl {
                    def_id: descending_def,
                    init: Some(HIRExpr {
                        kind: HIRExprKind::Binary {
                            op: HIRBinOp::Gt,

                            lhs: Box::new(HIRExpr {
                                kind: HIRExprKind::VarRef(
                                    var_def_id,
                                ),
                                ty: loop_ty.clone(),
                                span: variable.span,
                            }),

                            rhs: Box::new(HIRExpr {
                                kind: HIRExprKind::VarRef(
                                    end_def,
                                ),
                                ty: loop_ty.clone(),
                                span: variable.span,
                            }),
                        },

                        ty: IRType::BOOL,
                        span: variable.span,
                    }),
                    ty: IRType::BOOL,

                    has_type_annotation: false,
                    span: variable.span,
                };

                //
                // ascending:
                //
                // exclusive: break when i >= end
                // inclusive: break when i > end
                //
                let ascending_op = if *is_inclusive { HIRBinOp::Gt } else { HIRBinOp::Ge };

                //
                // descending:
                //
                // exclusive: break when i <= end
                // inclusive: break when i < end
                //
                let descending_op = if *is_inclusive { HIRBinOp::Lt } else { HIRBinOp::Le };

                let ascending_break_cond = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: ascending_op,

                        lhs: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                var_def_id,
                            ),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),

                        rhs: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                end_def,
                            ),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),
                    },

                    ty: IRType::BOOL,
                    span: variable.span,
                };

                let descending_break_cond = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: descending_op,

                        lhs: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                var_def_id,
                            ),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),

                        rhs: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                end_def,
                            ),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),
                    },

                    ty: IRType::BOOL,
                    span: variable.span,
                };

                let break_expr = || HIRExpr {
                    kind: HIRExprKind::Break,
                    ty: IRType::VOID,
                    span,
                };

                let ascending_break = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(
                            ascending_break_cond,
                        ),

                        then_block: Box::new(HIRBlock {
                            stmts: vec![
                                HIRStmt::Expr(break_expr())
                            ],
                            span,
                        }),

                        else_block: None,
                    },

                    ty: IRType::VOID,
                    span,
                };

                let descending_break = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(
                            descending_break_cond,
                        ),

                        then_block: Box::new(HIRBlock {
                            stmts: vec![
                                HIRStmt::Expr(break_expr())
                            ],
                            span,
                        }),

                        else_block: None,
                    },

                    ty: IRType::VOID,
                    span,
                };

                //
                // if descending {
                //     break if i <=/< end;
                // } else {
                //     break if i >=/> end;
                // }
                //
                let range_check = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                descending_def,
                            ),
                            ty: IRType::BOOL,
                            span: variable.span,
                        }),

                        then_block: Box::new(HIRBlock {
                            stmts: vec![
                                HIRStmt::Expr(
                                    descending_break
                                )
                            ],
                            span,
                        }),

                        else_block: Some(Box::new(
                            HIRBlock {
                                stmts: vec![
                                    HIRStmt::Expr(
                                        ascending_break
                                    )
                                ],
                                span,
                            },
                        )),
                    },

                    ty: IRType::VOID,
                    span,
                };

                let mut loop_stmts = vec![
                    HIRStmt::Expr(range_check)
                ];

                loop_stmts.extend(
                    self.lower_block(body)?.stmts
                );

                //
                // i = i + 1
                //
                let increment_expr = HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                var_def_id,
                            ),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),

                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Add,

                                lhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::VarRef(
                                        var_def_id,
                                    ),
                                    ty: loop_ty.clone(),
                                    span: variable.span,
                                }),

                                rhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::IntLiteral(1),
                                    ty: loop_ty.clone(),
                                    span: variable.span,
                                }),
                            },

                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),
                    },

                    ty: loop_ty.clone(),
                    span: variable.span,
                };

                //
                // i = i - 1
                //
                let decrement_expr = HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                var_def_id,
                            ),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),

                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Sub,

                                lhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::VarRef(
                                        var_def_id,
                                    ),
                                    ty: loop_ty.clone(),
                                    span: variable.span,
                                }),

                                rhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::IntLiteral(1),
                                    ty: loop_ty.clone(),
                                    span: variable.span,
                                }),
                            },

                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),
                    },

                    ty: loop_ty.clone(),
                    span: variable.span,
                };

                //
                // if descending {
                //     i = i - 1;
                // } else {
                //     i = i + 1;
                // }
                //
                let step_expr = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(
                                descending_def,
                            ),
                            ty: IRType::BOOL,
                            span: variable.span,
                        }),

                        then_block: Box::new(HIRBlock {
                            stmts: vec![
                                HIRStmt::Expr(
                                    decrement_expr
                                )
                            ],
                            span,
                        }),

                        else_block: Some(Box::new(
                            HIRBlock {
                                stmts: vec![
                                    HIRStmt::Expr(
                                        increment_expr
                                    )
                                ],
                                span,
                            },
                        )),
                    },

                    ty: IRType::VOID,
                    span,
                };

                loop_stmts.push(
                    HIRStmt::Expr(step_expr)
                );

                let loop_expr = HIRExpr {
                    kind: HIRExprKind::Loop(
                        Box::new(HIRBlock {
                            stmts: loop_stmts,
                            span,
                        }),
                    ),

                    ty: IRType::VOID,
                    span,
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Block(
                        HIRBlock {
                            stmts: vec![
                                init_stmt,
                                init_end,
                                init_descending,
                                HIRStmt::Expr(loop_expr),
                            ],

                            span,
                        },
                    ),

                    ty: IRType::VOID,
                    span,
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
                    ty: iter_expr.ty.clone(),
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
                    ty: IRType::I32,
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
                    ty: inner_ty.clone(),
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

    fn lower_value_block( &mut self, block: &parser::ast::Block, expected: Option<&IRType>) -> Result<(HIRBlock, IRType), HydraError> 
    {
        let Some((last, prefix)) = block.statements.split_last() else {
            return Err(
                self.error("S001", "if expression branch must produce a value", Span::default())
            );
        };

        let mut stmts = Vec::with_capacity(block.statements.len());

        for stmt in prefix {
            stmts.push(self.lower_stmt(stmt)?);
        }

        let ASTStmt::Expr(value_expr) = last else {
            return Err(
                self.error("S001","if expression branch must end with an expression", crate::utils::get_stmt_span(last))
            );
        };

        let mut value = self.lower_expr_with_type(value_expr, expected)?;

        if let Some(expected_ty) = expected {
            value = self.coerce_primitive(value, expected_ty);

            if !self.check_type_compatibility(expected_ty, &value.ty) {
                return Err(self.error(
                    "S001", 
                    format!("type mismatch: expected {}, found {}", expected_ty, value.ty), 
                    value.span
                ));
            }
        }

        let value_ty = value.ty.clone();

        stmts.push(HIRStmt::Expr(value));

        let span = block.statements.first().map(crate::utils::get_stmt_span).unwrap_or_default();

        Ok(
            (HIRBlock { stmts, span }, value_ty)
        )
    }
}
