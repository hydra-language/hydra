use super::Analyzer;
use errors::error::HydraError;
use parser::ast::ASTNode;
use ir::types::Type;
use ir::context::{DefKind, SymbolInfo};
use ir::hir::{HIRStmt, HIRExpr, HIRExprKind, HIRBlock, HIRBinOp, HIRUnaryOp}; 

impl Analyzer {

    pub(crate) fn lower_statement(&mut self, node: ASTNode) -> Result<HIRStmt, HydraError> {
        let span = self.get_token_from_node(&node).span;
        
        match node {
            ASTNode::VariableDeclaration { name, type_annotation, initializer, is_const, .. } => {
                let expected = if let Some(annotation) = type_annotation {
                    Some(self.lower_type(*annotation)?)
                } else {
                    None
                };
                
                let mut init_expr = self.lower_expr_with_type(*initializer, expected.as_ref())?;

                if let Some(ref target) = expected {
                    init_expr = self.coerce_primitive(init_expr, target);
                }

                let final_type = expected.unwrap_or(init_expr.ty.clone());

                let kind = if is_const {
                    DefKind::Constant {
                        ty: final_type.clone(),
                        value: ir::Constant::Float(0.0, final_type.clone()),
                    }
                } else {
                    DefKind::Variable { ty: final_type.clone(), is_mutable: true }
                };

                let info = SymbolInfo {
                    name: name.lexeme.to_string(),
                    span: name.span,
                    absolute_path: vec![name.lexeme.to_string()],
                    kind,
                };
                
                let def_id = self.context.insert_def(info);
                self.scope.define(name.lexeme.to_string(), def_id).map_err(|e| self.error("S002", e, name.span))?;
                
                Ok(HIRStmt::VarDecl { def_id, init: Some(init_expr) })
            },

            ASTNode::AssignmentExpression { .. } => {
                let expr = self.lower_expression(node)?;
                Ok(HIRStmt::Expr(expr))
            },

            ASTNode::ReturnStatement { value } => {
                let mut val = self.lower_expression(*value)?;
                
                if let Some(expected) = &self.current_return_type {
                    val = self.coerce_primitive(val, expected);             
                }

                Ok(HIRStmt::Expr(HIRExpr {
                    kind: HIRExprKind::Return(Some(Box::new(val))),
                    ty: Type::VOID,
                    span
                }))
            },

            ASTNode::IfStatement { condition, then_branch, else_branch } => {
                let cond = self.lower_expression(*condition)?;

                self.enter_scope();
                let mut then_stmts = Vec::new();
                for stmt in then_branch { then_stmts.push(self.lower_statement(stmt)?); }
                self.leave_scope();
                
                let else_block = if let Some(else_stmts_ast) = else_branch {
                    self.enter_scope();
                    let mut else_stmts = Vec::new();
                    for stmt in else_stmts_ast { else_stmts.push(self.lower_statement(stmt)?); }
                    self.leave_scope();
                    Some(Box::new(HIRBlock { stmts: else_stmts, span }))
                } else {
                    None
                };
                
                Ok(HIRStmt::Expr(HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(cond),
                        then_block: Box::new(HIRBlock { stmts: then_stmts, span }),
                        else_block
                    },
                    ty: Type::VOID,
                    span
                }))
            }

            ASTNode::WhileLoop { condition, body } => {
                let cond_span = self.get_token_from_node(&condition).span;
                let cond_expr = self.lower_expression(*condition)?;
                
                let break_expr = HIRExpr { kind: HIRExprKind::Break, ty: Type::VOID, span: cond_span };
                let not_cond = HIRExpr {
                    kind: HIRExprKind::Unary { op: HIRUnaryOp::Not, operand: Box::new(cond_expr) },
                    ty: Type::BOOL,
                    span: cond_span
                };
                
                let break_if = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(not_cond),
                        then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(break_expr)], span: cond_span }),
                        else_block: None,
                    },
                    ty: Type::VOID,
                    span: cond_span,
                };

                let mut loop_stmts = vec![HIRStmt::Expr(break_if)];
                
                self.enter_scope();
                for stmt in body { loop_stmts.push(self.lower_statement(stmt)?); }
                self.leave_scope();

                Ok(HIRStmt::Expr(HIRExpr {
                    kind: HIRExprKind::Loop(Box::new(HIRBlock { stmts: loop_stmts, span })),
                    ty: Type::VOID,
                    span
                }))
            },

            ASTNode::ForLoop { variable, start, end, is_inclusive, body } => {
                let start_expr = self.lower_expression(*start)?;
                let end_expr = self.lower_expression(*end)?;
                let var_name = variable.lexeme.to_string();

                self.enter_scope();
                
                let info = SymbolInfo {
                    name: var_name.clone(),
                    span: variable.span,
                    absolute_path: vec![var_name.clone()],
                    kind: DefKind::Variable { ty: start_expr.ty.clone(), is_mutable: true }
                };
                let var_def_id = self.context.insert_def(info);
                self.scope.define(var_name.clone(), var_def_id).unwrap();

                let init_stmt = HIRStmt::VarDecl { def_id: var_def_id, init: Some(start_expr.clone()) };

                let op = if is_inclusive { HIRBinOp::Gt } else { HIRBinOp::Ge };
                let check_cond = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op,
                        lhs: Box::new(HIRExpr { kind: HIRExprKind::VarRef(var_def_id), ty: start_expr.ty.clone(), span: variable.span }),
                        rhs: Box::new(end_expr)
                    },
                    ty: Type::BOOL,
                    span: variable.span
                };
                let break_if = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(check_cond),
                        then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(HIRExpr { kind: HIRExprKind::Break, ty: Type::VOID, span })], span }),
                        else_block: None,
                    },
                    ty: Type::VOID,
                    span,
                };

                let mut loop_stmts = vec![HIRStmt::Expr(break_if)];
                
                for stmt in body { loop_stmts.push(self.lower_statement(stmt)?); }

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
                    ty: Type::VOID,
                    span
                };

                self.leave_scope();

                Ok(HIRStmt::Expr(HIRExpr {
                    kind: HIRExprKind::Block(HIRBlock { stmts: vec![init_stmt, HIRStmt::Expr(loop_expr)], span }),
                    ty: Type::VOID,
                    span
                }))
            },

            // --- THE MISSING FOREACH DESUGARING ---
            ASTNode::ForEach { item, iterable, body } => {
                let iter_expr = self.lower_expression(*iterable)?;
                let (inner_ty, array_len) = match &iter_expr.ty {
                    Type::ARRAY(inner, size) => (*inner.clone(), *size as i64),
                    _ => return Err(self.error("S014", "foreach requires an array", item.span)),
                };
                
                self.enter_scope();
                
                let idx_name = format!("_idx_{}", item.span.line);
                let idx_info = SymbolInfo {
                    name: idx_name.clone(),
                    span: item.span,
                    absolute_path: vec![idx_name.clone()],
                    kind: DefKind::Variable { ty: Type::I32, is_mutable: true }
                };
                let idx_def = self.context.insert_def(idx_info);
                self.scope.define(idx_name.clone(), idx_def).unwrap();
                
                let init_idx = HIRStmt::VarDecl { 
                    def_id: idx_def, 
                    init: Some(HIRExpr { kind: HIRExprKind::IntLiteral(0), ty: Type::I32, span: item.span }) 
                };
                
                let mut loop_stmts = Vec::new();
                
                let break_cond = HIRExpr {
                    kind: HIRExprKind::Binary {
                        op: HIRBinOp::Ge,
                        lhs: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: Type::I32, span: item.span }),
                        rhs: Box::new(HIRExpr { kind: HIRExprKind::IntLiteral(array_len), ty: Type::I32, span: item.span })
                    },
                    ty: Type::BOOL,
                    span: item.span
                };
                
                let break_if = HIRExpr {
                    kind: HIRExprKind::If {
                        cond: Box::new(break_cond),
                        then_block: Box::new(HIRBlock { 
                            stmts: vec![HIRStmt::Expr(HIRExpr {
                                kind: HIRExprKind::Break, 
                                ty: Type::VOID, 
                                span: item.span 
                            })], 
                            span: item.span }
                        ),
                        else_block: None
                    },
                    ty: Type::VOID,
                    span: item.span
                };

                loop_stmts.push(HIRStmt::Expr(break_if));
                
                let item_info = SymbolInfo {
                    name: item.lexeme.to_string(),
                    span: item.span,
                    absolute_path: vec![item.lexeme.to_string()],
                    kind: DefKind::Variable { ty: inner_ty.clone(), is_mutable: false }
                };
                let item_def = self.context.insert_def(item_info);
                self.scope.define(item.lexeme.to_string(), item_def).unwrap();
                
                let init_item = HIRStmt::VarDecl {
                    def_id: item_def,
                    init: Some(HIRExpr {
                        kind: HIRExprKind::ArrayAccess {
                            array: Box::new(iter_expr.clone()),
                            index: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: Type::I32, span: item.span })
                        },
                        ty: inner_ty.clone(),
                        span: item.span
                    })
                };
                loop_stmts.push(init_item);
                
                for stmt in body {
                    loop_stmts.push(self.lower_statement(stmt)?);
                }
                
                let inc_idx = HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: Type::I32, span: item.span }),
                        value: Box::new(HIRExpr {
                            kind: HIRExprKind::Binary {
                                op: HIRBinOp::Add,
                                lhs: Box::new(HIRExpr { kind: HIRExprKind::VarRef(idx_def), ty: Type::I32, span: item.span }),
                                rhs: Box::new(HIRExpr { kind: HIRExprKind::IntLiteral(1), ty: Type::I32, span: item.span })
                            },
                            ty: Type::I32,
                            span: item.span
                        })
                    },
                    ty: Type::I32,
                    span: item.span
                };
                loop_stmts.push(HIRStmt::Expr(inc_idx));
                
                let loop_expr = HIRExpr {
                    kind: HIRExprKind::Loop(Box::new(HIRBlock { stmts: loop_stmts, span: item.span })),
                    ty: Type::VOID,
                    span: item.span
                };
                
                self.leave_scope();
                
                Ok(HIRStmt::Expr(HIRExpr {
                    kind: HIRExprKind::Block(HIRBlock { stmts: vec![init_idx, HIRStmt::Expr(loop_expr)], span: item.span }),
                    ty: Type::VOID,
                    span: item.span
                }))
            },

            // --- THE RESTORED CONDITIONAL BREAKS ---
            ASTNode::Break { condition } => {
                let break_expr = HIRExpr { kind: HIRExprKind::Break, ty: Type::VOID, span };
                if let Some(cond_node) = condition {
                    let cond = self.lower_expression(*cond_node)?;
                    Ok(HIRStmt::Expr(HIRExpr {
                        kind: HIRExprKind::If {
                            cond: Box::new(cond),
                            then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(break_expr)], span }),
                            else_block: None
                        },
                        ty: Type::VOID,
                        span
                    }))
                } else {
                    Ok(HIRStmt::Expr(break_expr))
                }
            },

            ASTNode::Continue { condition } => {
                let cont_expr = HIRExpr { kind: HIRExprKind::Continue, ty: Type::VOID, span };
                if let Some(cond_node) = condition {
                    let cond = self.lower_expression(*cond_node)?;
                    Ok(HIRStmt::Expr(HIRExpr {
                        kind: HIRExprKind::If {
                            cond: Box::new(cond),
                            then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(cont_expr)], span }),
                            else_block: None
                        },
                        ty: Type::VOID,
                        span
                    }))
                } else {
                    Ok(HIRStmt::Expr(cont_expr))
                }
            },

            ASTNode::FunctionCallExpression { .. } |
            ASTNode::BinaryExpression { .. } | 
            ASTNode::VariableExpression { .. } |
            ASTNode::Expression { .. } |
            ASTNode::MethodCallExpression { .. } => {
                let expr = self.lower_expression(node)?;
                Ok(HIRStmt::Expr(expr))
            }, 

            _ => Err(self.error("S013", "statement type not supported", span))
        }
    }
}
