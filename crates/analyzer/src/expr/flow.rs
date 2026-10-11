use super::super::Analyzer;

use errors::error::{HydraError, Span};

use ir::context::{DefID, DefKind, SymbolInfo};
use ir::hir::{HIRBinOp, HIRBlock, HIRExpr, HIRExprKind, HIRStmt, HIRUnaryOp};
use ir::types::Type as IRType;

use lexer::{Token, TokenType};
use parser::ast::{
    Expr as ASTExpr, Stmt as ASTStmt,
    MatchArm, MatchElse, MatchPattern,
    NodeID, Block as ASTBlock,
};

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_control_flow_expr(&mut self, node: &ASTExpr, expected: Option<&IRType>, span: Span)
        -> Result<HIRExpr, HydraError> 
    {
        match node {

            ASTExpr::Match { value, arms, else_arm, .. } => {
                self.lower_match_expr(value, arms, else_arm.as_ref(), expected, span)
            }

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
                let mut start_expr = self.lower_expr(start)?;
                let mut end_expr = self.lower_expr(end)?;

                if !self.is_int_type(&start_expr.ty) || !self.is_int_type(&end_expr.ty) {
                    return Err(self.error(
                        "S001",
                        "for range bounds must be integers",
                        span,
                    ));
                }

                if start_expr.ty != end_expr.ty {
                    let end_ty = end_expr.ty.clone();
                    let coerced_start = self.coerce_primitive(start_expr.clone(), &end_ty);

                    if coerced_start.ty == end_ty {
                        start_expr = coerced_start;
                    } else {
                        let start_ty = start_expr.ty.clone();
                        end_expr = self.coerce_primitive(end_expr, &start_ty);
                    }
                }

                if start_expr.ty != end_expr.ty {
                    return Err(self.error(
                        "S001",
                        format!(
                            "for range bounds have incompatible types `{}` and `{}`",
                            start_expr.ty,
                            end_expr.ty,
                        ),
                        span,
                    ));
                }

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
                    local: self.hir_local(var_def_id, loop_ty.clone()),
                    init: Some(start_expr.clone()),
                    has_type_annotation: false,
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
                    local: self.hir_local(end_def, loop_ty.clone()),
                    init: Some(end_expr),
                    has_type_annotation: false,
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
                    local: self.hir_local(descending_def, IRType::BOOL),
                    init: Some(HIRExpr {
                        kind: HIRExprKind::Binary {
                            op: HIRBinOp::Gt,
                            lhs: Box::new(HIRExpr {
                                kind: HIRExprKind::VarRef(var_def_id),
                                ty: loop_ty.clone(),
                                span: variable.span,
                            }),
                            rhs: Box::new(HIRExpr {
                                kind: HIRExprKind::VarRef(end_def),
                                ty: loop_ty.clone(),
                                span: variable.span,
                            }),
                        },
                        ty: IRType::BOOL,
                        span: variable.span,
                    }),
                    has_type_annotation: false,
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
                            kind: HIRExprKind::VarRef(var_def_id),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),

                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Add,
                                lhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::VarRef(var_def_id),
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

                    ty: IRType::VOID,
                    span: variable.span,
                };

                //
                // i = i - 1
                //
                let decrement_expr = HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(var_def_id),
                            ty: loop_ty.clone(),
                            span: variable.span,
                        }),

                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Sub,
                                lhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::VarRef(var_def_id),
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

                    ty: IRType::VOID,
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
                    local: self.hir_local(arr_def, iter_expr.ty.clone()),
                    init: Some(iter_expr.clone()),
                    has_type_annotation: false,
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
                    local: self.hir_local(idx_def, IRType::I32),
                    init: Some(HIRExpr {
                        kind: HIRExprKind::IntLiteral(0),
                        ty: IRType::I32,
                        span: item.span,
                    }),
                    has_type_annotation: false,
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
                    local: self.hir_local(item_def, inner_ty.clone()),
                    init: Some(HIRExpr {
                        kind: HIRExprKind::ArrayAccess {
                            array: Box::new(HIRExpr {
                                kind: HIRExprKind::VarRef(arr_def),
                                ty: iter_expr.ty.clone(),
                                span: item.span,
                            }),
                            index: Box::new(HIRExpr {
                                kind: HIRExprKind::VarRef(idx_def),
                                ty: IRType::I32,
                                span: item.span,
                            }),
                        },
                        ty: inner_ty.clone(),
                        span: item.span,
                    }),
                    has_type_annotation: false,
                };

                loop_stmts.push(init_item);

                loop_stmts.extend(self.lower_block(body)?.stmts);

                let inc_idx = HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(HIRExpr {
                            kind: HIRExprKind::VarRef(idx_def),
                            ty: IRType::I32,
                            span: item.span,
                        }),

                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Add,
                                lhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::VarRef(idx_def),
                                    ty: IRType::I32,
                                    span: item.span,
                                }),
                                rhs: Box::new(HIRExpr {
                                    kind: HIRExprKind::IntLiteral(1),
                                    ty: IRType::I32,
                                    span: item.span,
                                }),
                            },
                            ty: IRType::I32,
                            span: item.span,
                        }),
                    },
                    ty: IRType::VOID,
                    span: item.span,
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

            ASTExpr::OrElse { optional, fallback, .. } => {
                //
                // do not give the left side an optional expected type. it must
                // genuinely produce T? rather than being promoted from T
                //
                let optional = self.lower_expr(optional)?;

                let IRType::OPTIONAL(inner) = &optional.ty else {
                    return Err(self.error(
                        "S001",
                        format!("`orelse` requires an optional left operand, found `{}`",
                            optional.ty
                        ),
                        optional.span
                    ));
                };

                let result_ty = inner.as_ref().clone();

                //
                // the fallback is evaluated as the underlying T, not T?
                //
                let mut fallback = self.lower_expr_with_type(fallback, Some(&result_ty))?;
                fallback = self.coerce_primitive(fallback, &result_ty);

                if !self.check_type_compatibility(&result_ty, &fallback.ty) {
                    return Err(self.error(
                        "S001",
                        format!(
                            "`orelse` fallback expected {}, found {}",
                            result_ty,
                            fallback.ty,
                        ),
                        fallback.span,
                    ));
                }

                Ok(HIRExpr {
                    kind: HIRExprKind::OrElse {
                        optional: Box::new(optional),
                        fallback: Box::new(fallback),
                    },
                    ty: result_ty,
                    span,
                })
            }

            _ => unreachable!()
        }
    }

    fn lower_value_block( &mut self, block: &ASTBlock, expected: Option<&IRType>) -> Result<(HIRBlock, IRType), HydraError> 
    {
        let Some((last, prefix)) = block.statements.split_last() else {
            return Err(
                self.error("S001", "value producing branch must produce a value", Span::default())
            );
        };

        let mut stmts = Vec::with_capacity(block.statements.len());

        for stmt in prefix {
            stmts.push(self.lower_stmt(stmt)?);
        }

        let ASTStmt::Expr(value_expr) = last else {
            return Err(
                self.error("S001","value producing branch must end with an expression", crate::utils::get_stmt_span(last))
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

    fn lower_match_expr(
        &mut self,
        value: &ASTExpr,
        arms: &[MatchArm],
        else_arm: Option<&MatchElse>,
        expected: Option<&IRType>,
        span: Span,
    ) -> Result<HIRExpr, HydraError> 
    {
        let subject = self.lower_expr(value)?;
        let subject_ty = subject.ty.clone();

        self.check_match_exhaustiveness(
            &subject_ty,
            arms,
            else_arm.is_some(),
            span,
        )?;

        let subject_def = self.context.insert_def(SymbolInfo {
            name: "_match_value".to_string(),
            span,
            absolute_path: vec![],
            kind: DefKind::Variable {
                ty: subject_ty.clone(),
                is_mutable: false,
            },
            is_pub: false,
        });

        let subject_decl = HIRStmt::VarDecl {
            local: self.hir_local(subject_def, subject_ty.clone()),
            init: Some(subject),
            has_type_annotation: false,
        };

        let none_handled = arms.iter().any(|arm| Self::is_none_match_pattern(&arm.pattern));

        let branches_produce_values = !arms.is_empty() && 
            arms.iter().all(|arm| matches!(arm.body.statements.last(), Some(ASTStmt::Expr(_)))) &&
            else_arm.is_none_or(|arm| matches!(arm.body.statements.last(), Some(ASTStmt::Expr(_)))
        );

        let is_value_match = expected.is_some() || branches_produce_values;

        let mut lowered_arms = Vec::with_capacity(arms.len());

        for arm in arms {
            let condition = self.lower_match_pattern_condition(
                subject_def,
                &subject_ty,
                &arm.pattern,
                span,
            )?;

            let binding = if arm.binding.is_some() {
                let (binding_ty, binding_value) = match &subject_ty {
                    IRType::OPTIONAL(_) if Self::is_none_match_pattern(&arm.pattern) => {
                        (subject_ty.clone(), Self::match_subject_ref(subject_def, &subject_ty, span))
                    }

                    IRType::OPTIONAL(inner) => {
                        let inner_ty = inner.as_ref().clone();

                        (inner_ty.clone(), Self::match_optional_payload_ref(subject_def, &subject_ty, &inner_ty, span))
                    }

                    _ => (subject_ty.clone(), Self::match_subject_ref(subject_def, &subject_ty, span))
                };

                self.lower_match_binding(arm.binding.as_ref(), binding_ty, binding_value)?
            } else {
                None
            };

            let (mut body, branch_ty) = if is_value_match {
                let (body, ty) = self.lower_value_block(
                    &arm.body,
                    expected,
                )?;

                (body, ty)
            } else {
                (
                    self.lower_block(&arm.body)?,
                    IRType::VOID,
                )
            };

            if let Some(binding) = binding {
                body.stmts.insert(0, binding);
            }

            lowered_arms.push((
                condition,
                body,
                branch_ty,
            ));
        }

        let mut lowered_else = if let Some(else_arm) = else_arm {
            let binding = if else_arm.binding.is_some() {
                let (binding_ty, binding_value) = match &subject_ty {
                    IRType::OPTIONAL(inner) if none_handled => {
                        let inner_ty = inner.as_ref().clone();

                        (
                            inner_ty.clone(),
                            Self::match_optional_payload_ref(
                                subject_def,
                                &subject_ty,
                                &inner_ty,
                                span,
                            ),
                        )
                    }

                    _ => {
                        (
                            subject_ty.clone(),
                            Self::match_subject_ref(
                                subject_def,
                                &subject_ty,
                                span,
                            ),
                        )
                    }
                };

                self.lower_match_binding(
                    else_arm.binding.as_ref(),
                    binding_ty,
                    binding_value,
                )?
            } else {
                None
            };

            let (mut body, branch_ty) = if is_value_match {
                let (body, ty) = self.lower_value_block(
                    &else_arm.body,
                    expected,
                )?;

                (body, ty)
            } else {
                (
                    self.lower_block(&else_arm.body)?,
                    IRType::VOID,
                )
            };

            if let Some(binding) = binding {
                body.stmts.insert(0, binding);
            }

            Some((body, branch_ty))
        } else {
            None
        };

        let result_ty = if !is_value_match {
            IRType::VOID
        } else if let Some(expected) = expected {
            expected.clone()
        } else {
            let mut branch_types = lowered_arms
                .iter()
                .map(|(_, _, ty)| ty)
                .chain(lowered_else.iter().map(|(_, ty)| ty));

            let first = branch_types
                .next()
                .cloned()
                .ok_or_else(|| {
                    self.error(
                        "S001",
                        "value-producing match has no branches",
                        span,
                    )
                })?;

            branch_types.fold(first, |current, next| {
                self.join_match_result_types(&current, next)
            })
        };

        if is_value_match {
            for (_, body, _) in &mut lowered_arms {
                self.coerce_match_value_block(
                    body,
                    &result_ty,
                )?;
            }

            if let Some((body, _)) = &mut lowered_else {
                self.coerce_match_value_block(
                    body,
                    &result_ty,
                )?;
            }
        }

        let mut final_else = lowered_else.map(|(body, _)| Box::new(body));

        if else_arm.is_none() {
            let (_, final_body, _) = lowered_arms.pop().expect("ICE: exhaustive match without else has no arms");
            final_else = Some(Box::new(final_body));
        }

        let mut chain = None;

        for (condition, body, _) in lowered_arms.into_iter().rev() {
            let else_block = if let Some(next) = chain.take() {
                Some(Box::new(HIRBlock {
                    stmts: vec![
                        HIRStmt::Expr(next)
                    ],
                    span,
                }))
            } else {
                final_else.take()
            };

            chain = Some(HIRExpr {
                kind: HIRExprKind::If {
                    cond: Box::new(condition),
                    then_block: Box::new(body),
                    else_block,
                },
                ty: result_ty.clone(),
                span,
            });
        }

        let dispatch = if let Some(chain) = chain {
            chain
        } else if let Some(else_block) = final_else {
            HIRExpr {
                kind: HIRExprKind::Block(*else_block),
                ty: result_ty.clone(),
                span,
            }
        } else {
            return Err(self.error(
                "S001",
                "match has no branches",
                span,
            ));
        };

        Ok(HIRExpr {
            kind: HIRExprKind::Block(HIRBlock {
                stmts: vec![
                    subject_decl,
                    HIRStmt::Expr(dispatch),
                ],
                span,
            }),
            ty: result_ty,
            span,
        })
    }

    fn lower_match_pattern_condition(
        &mut self,
        subject_def: DefID,
        subject_ty: &IRType,
        pattern: &MatchPattern,
        span: Span,
    ) -> Result<HIRExpr, HydraError> {
        if Self::is_none_match_pattern(pattern) {
            let IRType::OPTIONAL(_) = subject_ty else {
                return Err(self.error(
                    "S001",
                    format!(
                        "`None` cannot match non-optional type `{}`",
                        subject_ty,
                    ),
                    span,
                ));
            };

            return Ok(HIRExpr {
                kind: HIRExprKind::Unary {
                    op: HIRUnaryOp::Not,
                    operand: Box::new(
                        Self::match_optional_present_ref(
                            subject_def,
                            subject_ty,
                            span,
                        ),
                    ),
                },
                ty: IRType::BOOL,
                span,
            });
        }

        let target_ty = match subject_ty {
            IRType::OPTIONAL(inner) => inner.as_ref(),
            _ => subject_ty,
        };

        let subject_value = match subject_ty {
            IRType::OPTIONAL(_) => {
                Self::match_optional_payload_ref(
                    subject_def,
                    subject_ty,
                    target_ty,
                    span,
                )
            }

            _ => {
                Self::match_subject_ref(
                    subject_def,
                    subject_ty,
                    span,
                )
            }
        };

        let condition = match pattern {
            MatchPattern::Value(pattern) => {
                let pattern = self.lower_match_pattern_value(
                    pattern,
                    target_ty,
                )?;

                HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: HIRBinOp::Eq,
                        lhs: Box::new(subject_value),
                        rhs: Box::new(pattern),
                    },
                    ty: IRType::BOOL,
                    span,
                }
            }

            MatchPattern::Range {
                start,
                end,
                is_inclusive,
            } => {
                if !self.is_int_type(target_ty) {
                    return Err(self.error(
                        "S001",
                        format!(
                            "match range requires an integer type, found `{}`",
                            target_ty,
                        ),
                        span,
                    ));
                }

                let start = self.lower_match_pattern_value(
                    start,
                    target_ty,
                )?;

                let end = self.lower_match_pattern_value(
                    end,
                    target_ty,
                )?;

                let lower_bound = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: HIRBinOp::Ge,
                        lhs: Box::new(subject_value.clone()),
                        rhs: Box::new(start),
                    },
                    ty: IRType::BOOL,
                    span,
                };

                let upper_bound = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: if *is_inclusive {
                            HIRBinOp::Le
                        } else {
                            HIRBinOp::Lt
                        },
                        lhs: Box::new(subject_value),
                        rhs: Box::new(end),
                    },
                    ty: IRType::BOOL,
                    span,
                };

                HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: HIRBinOp::And,
                        lhs: Box::new(lower_bound),
                        rhs: Box::new(upper_bound),
                    },
                    ty: IRType::BOOL,
                    span,
                }
            }
        };

        if matches!(subject_ty, IRType::OPTIONAL(_)) {
            Ok(HIRExpr {
                kind: HIRExprKind::Binary {
                    op: HIRBinOp::And,
                    lhs: Box::new(
                        Self::match_optional_present_ref(
                            subject_def,
                            subject_ty,
                            span,
                        ),
                    ),
                    rhs: Box::new(condition),
                },
                ty: IRType::BOOL,
                span,
            })
        } else {
            Ok(condition)
        }
    }

    fn lower_match_pattern_value(
        &mut self,
        pattern: &ASTExpr,
        target_ty: &IRType,
    ) -> Result<HIRExpr, HydraError> {
        let mut pattern = self.lower_expr_with_type(
            pattern,
            Some(target_ty),
        )?;

        pattern = self.coerce_primitive(
            pattern,
            target_ty,
        );

        if !self.check_type_compatibility(
            target_ty,
            &pattern.ty,
        ) {
            return Err(self.error(
                "S001",
                format!(
                    "match pattern expected `{}`, found `{}`",
                    target_ty,
                    pattern.ty,
                ),
                pattern.span,
            ));
        }

        Ok(pattern)
    }

    fn lower_match_binding(
        &mut self,
        binding: Option<&(NodeID, Token)>,
        ty: IRType,
        value: HIRExpr,
    ) -> Result<Option<HIRStmt>, HydraError> {
        let Some((id, name)) = binding else {
            return Ok(None);
        };

        let def_id = self.name_resolver
            .get_resolution(*id)
            .ok_or_else(|| {
                self.error(
                    "S002",
                    format!(
                        "match binding definition not found for `{}`",
                        name.lexeme,
                    ),
                    name.span,
                )
            })?;

        let mut info = self.context
            .get_def(def_id)
            .cloned()
            .ok_or_else(|| {
                self.error(
                    "S002",
                    format!(
                        "missing definition for match binding `{}`",
                        name.lexeme,
                    ),
                    name.span,
                )
            })?;

        info.kind = DefKind::Variable {
            ty: ty.clone(),
            is_mutable: false,
        };

        self.context.update_def(
            def_id,
            info,
        );

        Ok(Some(HIRStmt::VarDecl {
            local: self.hir_local(def_id, ty),
            init: Some(value),
            has_type_annotation: false,
        }))
    }

    fn join_match_result_types(
        &self,
        current: &IRType,
        next: &IRType,
    ) -> IRType {
        if current == next {
            return current.clone();
        }

        match (current, next) {
            (
                IRType::OPTIONAL(inner),
                other,
            ) if self.check_type_compatibility(inner, other) => {
                current.clone()
            }

            (
                other,
                IRType::OPTIONAL(inner),
            ) if self.check_type_compatibility(inner, other) => {
                next.clone()
            }

            _ => current.clone(),
        }
    }

    fn coerce_match_value_block(
        &self,
        block: &mut HIRBlock,
        target: &IRType,
    ) -> Result<(), HydraError> {
        let Some(HIRStmt::Expr(value)) = block.stmts.last_mut() else {
            return Err(self.error(
                "S001",
                "value-producing match branch must end with an expression",
                block.span,
            ));
        };

        if value.ty == *target {
            return Ok(());
        }

        if let IRType::OPTIONAL(inner) = target {
            if matches!(value.ty, IRType::OPTIONAL(_)) {
                return Err(self.error(
                    "S001",
                    format!(
                        "match branches have incompatible types: expected `{}`, found `{}`",
                        target,
                        value.ty,
                    ),
                    value.span,
                ));
            }

            let mut promoted = self.coerce_primitive(
                value.clone(),
                inner,
            );

            if !self.check_type_compatibility(
                inner,
                &promoted.ty,
            ) {
                return Err(self.error(
                    "S001",
                    format!(
                        "match branches have incompatible types: expected `{}`, found `{}`",
                        target,
                        promoted.ty,
                    ),
                    promoted.span,
                ));
            }

            let span = promoted.span;

            promoted = HIRExpr {
                kind: HIRExprKind::OptionalPresent(
                    Box::new(promoted),
                ),
                ty: target.clone(),
                span,
            };

            *value = promoted;

            return Ok(());
        }

        let coerced = self.coerce_primitive(
            value.clone(),
            target,
        );

        if !self.check_type_compatibility(target, &coerced.ty) {
            return Err(self.error(
                "S001",
                format!(
                    "match branches have incompatible types: expected `{}`, found `{}`",
                    target,
                    coerced.ty,
                ),
                coerced.span,
            ));
        }

        *value = coerced;

        Ok(())
    }

    fn is_none_match_pattern(pattern: &MatchPattern) -> bool {
        matches!(pattern, MatchPattern::Value(value) if matches!(&**value, ASTExpr::Literal { token, .. } if matches!(&token.token_type, TokenType::NONE)))
    }

    fn match_subject_ref(def_id: DefID, ty: &IRType, span: Span) -> HIRExpr {
        HIRExpr {
            kind: HIRExprKind::VarRef(def_id),
            ty: ty.clone(),
            span,
        }
    }

    fn match_optional_present_ref(def_id: DefID, optional_ty: &IRType, span: Span) -> HIRExpr {
        HIRExpr {
            kind: HIRExprKind::FieldAccess {
                object: Box::new(
                    Self::match_subject_ref(
                        def_id,
                        optional_ty,
                        span,
                    ),
                ),
                field_index: 0,
            },
            ty: IRType::BOOL,
            span,
        }
    }

    fn match_optional_payload_ref(def_id: DefID, optional_ty: &IRType, inner_ty: &IRType, span: Span) -> HIRExpr {
        HIRExpr {
            kind: HIRExprKind::FieldAccess {
                object: Box::new(
                    Self::match_subject_ref(
                        def_id,
                        optional_ty,
                        span,
                    ),
                ),
                field_index: 1,
            },
            ty: inner_ty.clone(),
            span,
        }
    }

    fn check_match_exhaustiveness(&self, subject_ty: &IRType, arms: &[MatchArm], has_else: bool, span: Span) -> Result<(), HydraError> 
    {
        if has_else {
            return Ok(());
        }

        let (value_ty, requires_none) = match subject_ty {
            IRType::OPTIONAL(inner) => (inner.as_ref(), true),
            _ => (subject_ty, false),
        };

        let none_covered = arms
            .iter()
            .any(|arm| Self::is_none_match_pattern(&arm.pattern));

        let values_covered = match value_ty {
            IRType::BOOL => {
                let mut has_true = false;
                let mut has_false = false;

                for arm in arms {
                    let MatchPattern::Value(pattern) = &arm.pattern else {
                        continue;
                    };

                    match Self::match_bool_pattern(pattern) {
                        Some(true) => has_true = true,
                        Some(false) => has_false = true,
                        None => {}
                    }
                }

                has_true && has_false
            }

            ty if Self::match_integer_domain(ty).is_some() => {
                let (min, max, _) = Self::match_integer_domain(ty).unwrap();
                let mut intervals = Vec::new();

                for arm in arms {
                    if let Some(interval) = Self::match_integer_interval(
                        &arm.pattern,
                        min,
                        max,
                    ) {
                        intervals.push(interval);
                    }
                }

                Self::integer_intervals_cover_domain(
                    &mut intervals,
                    min,
                    max,
                )
            }

            _ => false,
        };

        if values_covered && (!requires_none || none_covered) {
            return Ok(());
        }

        Err(self.non_exhaustive_match_error(
            subject_ty,
            span,
        ))
    }

    fn match_bool_pattern(pattern: &ASTExpr) -> Option<bool> {
        let ASTExpr::Literal { token, .. } = pattern else {
            return None;
        };

        match &token.token_type {
            TokenType::BoolLiteral(value) => Some(*value),
            _ => None,
        }
    }

    fn match_integer_domain(ty: &IRType) -> Option<(i128, i128, u128)> {
        match ty {
            IRType::I8 => Some((
                i8::MIN as i128,
                i8::MAX as i128,
                1u128 << 8,
            )),

            IRType::I16 => Some((
                i16::MIN as i128,
                i16::MAX as i128,
                1u128 << 16,
            )),

            IRType::I32 => Some((
                i32::MIN as i128,
                i32::MAX as i128,
                1u128 << 32,
            )),

            IRType::I64 | IRType::ISIZE => Some((
                i64::MIN as i128,
                i64::MAX as i128,
                1u128 << 64,
            )),

            IRType::U8 => Some((
                u8::MIN as i128,
                u8::MAX as i128,
                1u128 << 8,
            )),

            IRType::U16 => Some((
                u16::MIN as i128,
                u16::MAX as i128,
                1u128 << 16,
            )),

            IRType::U32 => Some((
                u32::MIN as i128,
                u32::MAX as i128,
                1u128 << 32,
            )),

            IRType::U64 | IRType::USIZE => Some((
                u64::MIN as i128,
                u64::MAX as i128,
                1u128 << 64,
            )),

            _ => None,
        }
    }

    fn match_integer_interval(pattern: &MatchPattern, domain_min: i128, domain_max: i128,) -> Option<(i128, i128)> 
    {
        let (start, end) = match pattern {
            MatchPattern::Value(value) => {
                let value = Self::match_integer_constant(value)?;
                (value, value)
            }

            MatchPattern::Range { start, end, is_inclusive } => {
                let start = Self::match_integer_constant(start)?;
                let mut end = Self::match_integer_constant(end)?;

                if !*is_inclusive {
                    end = end.checked_sub(1)?;
                }

                (start, end)
            }
        };

        let start = start.max(domain_min);
        let end = end.min(domain_max);

        if start > end {
            return None;
        }

        Some((start, end))
    }

    fn match_integer_constant(expr: &ASTExpr) -> Option<i128> {
        match expr {
            ASTExpr::Literal { token, .. } => {
                match &token.token_type {
                    TokenType::IntLiteral(value) => {
                        Some(*value as i128)
                    }

                    _ => None,
                }
            }

            ASTExpr::Unary { operator, right, .. } if matches!(&operator.token_type, TokenType::Minus) => {
                Self::match_integer_constant(right)?.checked_neg()
            }

            _ => None,
        }
    }

    fn integer_intervals_cover_domain(intervals: &mut [(i128, i128)], domain_min: i128, domain_max: i128) -> bool {
        intervals.sort_unstable_by_key(|(start, _)| *start);

        let mut next_required = domain_min;

        for &(start, end) in intervals.iter() {
            if end < next_required {
                continue;
            }

            if start > next_required {
                return false;
            }

            if end >= domain_max {
                return true;
            }

            next_required = end + 1;
        }

        false
    }

    fn non_exhaustive_match_error(&self, subject_ty: &IRType, span: Span) -> HydraError {
        let mut error = self.error("M001", format!("non-exhaustive match on `{}`", subject_ty), span);

        let (value_ty, is_optional) = match subject_ty {
            IRType::OPTIONAL(inner) => {
                (inner.as_ref(), true)
            }

            _ => (subject_ty, false)
        };

        if let Some((_, _, value_count)) = Self::match_integer_domain(value_ty) {
            let total = if is_optional {
                value_count + 1
            } else {
                value_count
            };

            let total = Self::format_match_state_count(
                total,
            );

            if is_optional {
                error = error.with_help(format!(
                    "handle all {} possible states of `{}` or add an `else` block",
                    total,
                    subject_ty,
                ));
            } else {
                error = error.with_help(format!(
                    "handle all {} possible `{}` values or add an `else` block",
                    total,
                    subject_ty,
                ));
            }
        } else if *value_ty == IRType::BOOL {
            let total = if is_optional {
                3
            } else {
                2
            };

            if is_optional {
                error = error.with_help(format!(
                    "handle all {} possible states of `{}` or add an `else` block",
                    total,
                    subject_ty,
                ));
            } else {
                error = error.with_help(
                    "handle both possible `bool` values or add an `else` block",
                );
            }
        } else {
            error = error.with_help(
                "add an `else` block to cover all remaining values",
            );
        }

        error.with_note(
            "unless you have ambitious plans for this match, consider `else`",
            span,
        )
    }

    fn format_match_state_count(value: u128) -> String {
        let digits = value.to_string();
        let len = digits.len();

        let mut formatted = String::with_capacity(
            len + len / 3,
        );

        for (index, digit) in digits.chars().enumerate() {
            if index != 0 && (len - index).is_multiple_of(3){
                formatted.push(',');
            }

            formatted.push(digit);
        }

        formatted
    }
}
