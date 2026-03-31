use super::Analyzer;
use errors::HydraError;
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use ir::{stmt::{Stmt, Block, LoopKind, AssignmentTarget}, expr::{Expr, ExprKind, BinaryOp}, types::Type};
use crate::Symbol;

impl Analyzer {

    pub(crate) fn lower_statement(&mut self, node: ASTNode) -> Result<Stmt, HydraError<'static>> {
        match node {
            ASTNode::VariableDeclaration { name, type_annotation, initializer, is_const } => {
                let mut val = self.lower_expression(*initializer)?;

                if let ExprKind::STRING_LITERAL(_) = &val.kind {
                    if !is_const {
                        return Err(self.make_error(
                            "raw strings must be bound to const variables".to_string(), 
                            &name
                        ));
                    }
                }

                if let Some(ann) = type_annotation {
                    let mut explicit = self.lower_type(*ann)?;

                    if let Type::INFERRED_ARRAY(inner) = &explicit {
                        match &val.kind {
                            ExprKind::STRING_LITERAL(s) => {
                                explicit = Type::ARRAY(inner.clone(), s.len());
                            },

                            ExprKind::ArrayInit { elements } => {
                                explicit = Type::ARRAY(inner.clone(), elements.len())
                            },

                            _ => return Err(self.make_error(
                                "cannot infer array size from this initializer".to_string(),
                                &name
                            ))
                        }
                    }

                    if let ExprKind::INT_LITERAL(l) = val.kind {
                        if !self.check_and_promote_int_literal(l, &explicit) {
                            return Err(self.make_error(
                                format!("literal value {} does not fit into type {}", l, explicit),
                                &name
                            ));
                        }

                        val.ty = explicit.clone();
                    }

                    if !self.check_type_compatibility(&explicit, &val.ty) {
                        return Err(self.make_error(
                            format!("type mismatch: expected {}, found {}", explicit, val.ty),
                            &name
                        ));
                    }

                    if let ExprKind::ArrayInit { elements } = &mut val.kind {
                        if let Type::ARRAY(target, _) = &explicit {
                            for elem in elements {
                                if let ExprKind::INT_LITERAL(i) = elem.kind {
                                    if self.check_and_promote_int_literal(i, target) {
                                        elem.ty = *target.clone();
                                    }
                                }

                                if elem.ty != **target {
                                    return Err(self.make_error(
                                        format!("type mismatch: array initializer expected {}, found {}",
                                            target, elem.ty
                                        ),
                                        &name,
                                    ));
                                }
                            }

                            val.ty = explicit.clone();
                        }
                    }

                    if !self.check_type_compatibility(&explicit, &val.ty) {
                        return Err(self.make_error(
                            format!("type mismatch: expected {}, found {}", explicit, val.ty),
                            &name
                        ));
                    }

                    val.ty = explicit.clone();
                }

                self.scope.define(name.lexeme.to_string(), 
                    Symbol::Variable {
                        ty: val.ty.clone(),
                        is_mutable: !is_const
                    }).map_err(|msg| self.make_error(msg, &name)
                    )?;

                Ok(Stmt::Var {
                    name: name.lexeme.to_string(),
                    ty: val.ty.clone(),
                    init: val,
                    is_mutable: !is_const
                })
            },

            ASTNode::AssignmentExpression { target, operator, value } => {
                let mut rhs = self.lower_expression(*value)?;

                match *target {
                    ASTNode::UnaryExpression { ref operator, ref right } 
                    if operator.token_type == TokenType::Star => 
                    {
                        let ptr_deref = self.lower_expression(*right.clone())?;
                        
                        if !matches!(ptr_deref.ty, Type::POINTER(_) | Type::REF(_) | Type::CONST_REF(_)) {
                            return Err(self.make_error(
                                "cannot dereference non-pointer type for assignment".into(), 
                                operator
                            ));
                        }

                        Ok(Stmt::Assign {
                            target: AssignmentTarget::PointerDeref(Box::new(ptr_deref)),
                            value: rhs
                        })
                    },

                    ASTNode::VariableExpression { name } => {
                        let var_name = name.lexeme.to_string();

                        let symbol = self.scope.resolve(&var_name)
                            .ok_or(self.make_error(
                                format!("cannot assign to undefined variable '{}'", var_name),
                                &name
                            ))?;

                        let (expected, is_mutable) = match symbol {
                            Symbol::Variable { ty, is_mutable } => (ty.clone(), *is_mutable),
                            _ => return Err(self.make_error(
                                format!("'{}' is not a variable", var_name),
                                &name
                            ))
                        };

                        if !is_mutable {
                            return Err(self.make_error(
                                format!("cannot assign to immutable variable '{}'", var_name),
                                &operator
                            ));
                        }

                        if !self.check_type_compatibility(&expected, &rhs.ty) {
                            return Err(self.make_error(
                                format!("type mismatch: cannot assign '{}' to variable of type '{}'", rhs.ty, expected),
                                &operator
                            ));
                        }

                        if let ExprKind::INT_LITERAL(l) = rhs.kind {
                            if self.check_and_promote_int_literal(l, &expected) {
                                rhs.ty = expected.clone();
                            }
                        }

                        if let Some(op) = self.get_binary_op_from_token(&operator.token_type) {
                            let lhs = Expr {
                                kind: ExprKind::VariableReference { name: var_name.clone() },
                                ty: expected.clone()
                            };

                            rhs = Expr {
                                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                                ty: expected.clone(),
                            };
                        }

                        Ok(Stmt::Assign {
                            target: AssignmentTarget::Variable(var_name),
                            value: rhs
                        })
                    },

                    ASTNode::ArrayAccess { array, index, token } => {
                        let arr_expr = self.lower_expression(*array)?;
                        let idx_expr = self.lower_expression(*index)?;

                        let inner_ty = match &arr_expr.ty {
                            Type::ARRAY(inner, size) => {
                                if let ExprKind::INT_LITERAL(idx_val) = idx_expr.kind {
                                    if idx_val < 0 || idx_val >= (*size as i64) {
                                        return Err(self.make_error(
                                            format!("index out of bounds: the len is {} but the index is {}", size, idx_val),
                                            &token
                                        ));
                                    }
                                }

                                inner.clone()
                            },

                            Type::INFERRED_ARRAY(inner) => inner.clone(),

                            _ => return Err(self.make_error(
                                format!("type '{}' cannot be indexed", arr_expr.ty), 
                                &token
                            ))
                        };

                        if !idx_expr.ty.is_numeric() {
                            return Err(self.make_error(
                                format!("array index must be numeric, found {}", idx_expr.ty), 
                                &token
                            ));
                        }

                        if !self.check_type_compatibility(&inner_ty, &rhs.ty) {
                            return Err(self.make_error(
                                format!("type mismatch: expected {}, found {}", inner_ty, rhs.ty),
                                &operator
                            ));
                        }

                        if let ExprKind::INT_LITERAL(l) = rhs.kind {
                            if self.check_and_promote_int_literal(l, &inner_ty) {
                                rhs.ty = *inner_ty.clone();
                            }
                        }

                        if let Some(op) = self.get_binary_op_from_token(&operator.token_type) {
                            let lhs = Expr {
                                kind: ExprKind::ArrayAccess {
                                    array: Box::new(arr_expr.clone()),
                                    index: Box::new(idx_expr.clone())
                                },
                                ty: *inner_ty.clone()
                            };

                            rhs = Expr {
                                kind: ExprKind::Binary {
                                    op,
                                    lhs: Box::new(lhs),
                                    rhs: Box::new(rhs),
                                },
                                ty: *inner_ty.clone(),
                            };
                        }

                        Ok(Stmt::Assign {
                            target: AssignmentTarget::ArrayAccess {
                                array: arr_expr,
                                index: idx_expr,
                            },
                            value: rhs
                        })
                    },

                    ASTNode::MemberExpression { object, property } => {
                        let target_expr = self.lower_expression(
                            ASTNode::MemberExpression {
                                object: object.clone(), 
                                property: property.clone() 
                            }
                        )?;

                        let (obj_expr, field_index) = if let ExprKind::MemberAccess { 
                            object: ref obj, 
                            index, 
                            .. 
                        } = target_expr.kind {
                            (obj.clone(), index)
                        } else {
                            unreachable!("something went wrong")
                        };

                        let actual_type = match &obj_expr.ty {
                            Type::REF(inner) | Type::CONST_REF(inner) => inner.as_ref(),
                            _ => &obj_expr.ty
                        };

                        if let Type::STRUCT(ref struct_name) = actual_type {
                            if let Some(Symbol::Struct { fields }) = self.scope.resolve(struct_name) {
                                if let Some((_, _, is_const)) = fields.iter().find(|(n, _, _)| n == &property.lexeme) {
                                    if *is_const {
                                        return Err(self.make_error(
                                            format!("cannot assign to constant struct property '{}'", property.lexeme),
                                            &property
                                        ));
                                    }
                                }
                            }
                        }

                        if let Some(op) = self.get_binary_op_from_token(&operator.token_type) {
                            rhs = Expr {
                                kind: ExprKind::Binary {
                                    op,
                                    lhs: Box::new(target_expr.clone()),
                                    rhs: Box::new(rhs),
                                },
                                ty: target_expr.ty.clone(),
                            };
                        }

                        Ok(Stmt::Assign {
                            target: AssignmentTarget::MemberAccess {
                                object: match target_expr.kind {
                                    ExprKind::MemberAccess { object, .. } => object,
                                    _ => unreachable!(),
                                },
                                member: property.lexeme.to_string(),
                                index: field_index,
                            },
                            value: rhs,
                        })
                    },

                    _ => Err(self.make_generic_error("assignment target must be a variable or array access".to_string()))
                }
            },

            ASTNode::ReturnStatement { value } => {
                let token = self.get_token_from_node(&value);
                let mut val = self.lower_expression(*value)?;

                if let Some(expected) = &self.current_return_type {
                    if let ExprKind::INT_LITERAL(i) = val.kind {
                        if self.check_and_promote_int_literal(i, expected) {
                            val.ty = expected.clone();
                        }
                    }

                    if val.ty != *expected {
                        return Err(self.make_error(
                            format!("type mismatch: expected {}, found {}", expected, val.ty),
                            &token
                        ));
                    }                
                } else {
                    return Err(self.make_error("return outside function body".to_string(), &token));
                }

                Ok(Stmt::Return(Some(val)))
            },

            ASTNode::IfStatement { condition, then_branch, else_branch } => {
                let cond = self.lower_expression(*condition)?;

                if cond.ty != Type::BOOL {
                    return Err(self.make_error(
                        format!("if condition must be a boolean, found {}", cond.ty),
                        &self.dummy_token()
                    ));
                }

                self.enter_scope();

                let mut then_stmts = Vec::new();

                for stmt in then_branch {
                    then_stmts.push(self.lower_statement(stmt)?);
                }
                self.leave_scope();

                let else_block = if let Some(else_stmts_ast) = else_branch {
                    self.enter_scope();

                    let mut else_stmts = Vec::new();

                    for stmt in else_stmts_ast {
                        else_stmts.push(self.lower_statement(stmt)?);
                    }
                    self.leave_scope();

                    Some(Block { stmts: else_stmts })
                } else {
                    None
                };

                Ok(Stmt::If {
                    cond,
                    then_block: Block {
                        stmts: then_stmts
                    },
                    else_block
                })
            }

            ASTNode::ForLoop { variable, start, end, is_inclusive, body } => {
                self.lower_for_loop(variable, *start, *end, is_inclusive, body)
            },

            ASTNode::ForEach { item, iterable, body } => {
                self.lower_foreach_loop(item, *iterable, body)
            },

            ASTNode::WhileLoop { condition, body } => {
                let cond = self.lower_expression(*condition)?;
                
                let mut ir_body = Vec::new();
                for stmt in body {
                    ir_body.push(self.lower_statement(stmt)?);
                }

                Ok(Stmt::While {
                    cond,
                    body: Block { stmts: ir_body },
                    kind: ir::stmt::LoopKind::While,
                })
            },

            ASTNode::Break { condition } => {
                let break_stmt = Stmt::Break;

                if let Some(expr) = condition {
                    let cond = self.lower_expression(*expr)?;

                    if cond.ty != Type::BOOL {
                        return Err(self.make_generic_error("break condition must be a boolean".into()));
                    }

                    Ok(Stmt::If {
                        cond,
                        then_block: Block {
                            stmts: vec![break_stmt]
                        },
                        else_block: None,
                    })
                } else {
                    Ok(break_stmt)
                }
            },

            ASTNode::Continue { condition } => {
                let continue_stmt = Stmt::Continue;

                if let Some(expr) = condition {
                    let cond = self.lower_expression(*expr)?;

                    if cond.ty != Type::BOOL {
                        return Err(self.make_generic_error("continue condition must be a boolean".into()));
                    }

                    Ok(Stmt::If {
                        cond,
                        then_block: Block {
                            stmts: vec![continue_stmt]
                        },
                        else_block: None
                    })
                } else {
                    Ok(continue_stmt)
                }
            },

            ASTNode::FunctionCallExpression { .. } | ASTNode::BinaryExpression { .. } | 
            ASTNode::VariableExpression { .. } | ASTNode::Expression { .. } => {
                let expr = self.lower_expression(node)?;
                Ok(Stmt::Expr(expr))
            }, 

            ASTNode::MethodCallExpression { .. } => {
                 let expr = self.lower_expression(node)?;
                 Ok(Stmt::Expr(expr))
            },

            _ => Err(self.make_generic_error(format!("statement type {:?} not supported", node)))
        }
    }

    pub(crate) fn lower_for_loop(&mut self, variable: Token, start: ASTNode, 
        end: ASTNode, is_inclusive: bool, body: Vec<ASTNode>
    ) -> Result<Stmt, HydraError<'static>> 
    {
        let start_expr = self.lower_expression(start)?;
        let end_expr = self.lower_expression(end)?;
        let var_name = variable.lexeme.to_string();

        // 1. Enter Scope
        self.enter_scope();
        
        let mut outer_stmts = Vec::new();

        // 2. Initialize loop variable: let i = start;
        outer_stmts.push(Stmt::Var {
            name: var_name.clone(),
            ty: start_expr.ty.clone(),
            init: start_expr.clone(),
            is_mutable: true,
        });

        // Register in Scope (so the body knows 'i' exists)
        self.scope.define(var_name.clone(), Symbol::Variable { 
            ty: start_expr.ty.clone(), 
            is_mutable: true 
        }).map_err(|msg| self.make_error(msg, &variable))?;

        // 3. Lower Body
        let mut ir_body = Vec::new();
        for stmt in body {
            ir_body.push(self.lower_statement(stmt)?);
        }

        // 4. Increment: i = i + 1
        ir_body.push(Stmt::Assign {
            target: ir::stmt::AssignmentTarget::Variable(var_name.clone()),
            value: Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::ADD,
                    lhs: Box::new(Expr { 
                        kind: ExprKind::VariableReference { name: var_name.clone() }, 
                        ty: start_expr.ty.clone() 
                    }),
                    rhs: Box::new(Expr { kind: ExprKind::INT_LITERAL(1), ty: start_expr.ty.clone() })
                },
                ty: start_expr.ty.clone()
            }
        });

        // 5. While Loop: while i < end
        let op = if is_inclusive { BinaryOp::LE } else { BinaryOp::LT };
        outer_stmts.push(Stmt::While {
            cond: Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(Expr { 
                        kind: ExprKind::VariableReference { name: var_name.clone() }, 
                        ty: start_expr.ty.clone() 
                    }),
                    rhs: Box::new(end_expr)
                },
                ty: Type::BOOL
            },
            body: Block { stmts: ir_body },
            kind: LoopKind::For,
        });

        self.leave_scope();

        // 6. Return as Block
        Ok(Stmt::Block(Block { 
            stmts: outer_stmts }
        ))
    }

    pub(crate) fn lower_foreach_loop(&mut self, item: Token, iterable: ASTNode, body: Vec<ASTNode>) -> Result<Stmt, HydraError<'static>> {
        let iter_expr = self.lower_expression(iterable)?; 

        // 1. Resolve types and length
        let (inner_ty, array_len) = match &iter_expr.ty {
            Type::ARRAY(inner, size) => (*inner.clone(), *size as i64),
            _ => return Err(self.make_error("foreach requires an array".to_string(), &item)),
        };

        // Create a new scope for the entire desugared structure
        self.enter_scope(); 

        let mut outer_stmts = Vec::new();
        let idx_name = format!("_idx_{}", item.line);
        let item_name = item.lexeme.to_string();

        // 2. Initialize index variable: let _idx_X = 0;
        outer_stmts.push(Stmt::Var {
            name: idx_name.clone(),
            ty: Type::I32,
            init: Expr { kind: ExprKind::INT_LITERAL(0), ty: Type::I32 },
            is_mutable: true,
        });
        self.scope.define(idx_name.clone(), Symbol::Variable { ty: Type::I32, is_mutable: true }).unwrap();

        let mut loop_body_stmts = Vec::new();

        // 3. Loop Body Item Binding: const item = arr[_idx];
        self.scope.define(item_name.clone(), Symbol::Variable { ty: inner_ty.clone(), is_mutable: false }).unwrap();
        loop_body_stmts.push(Stmt::Var {
            name: item_name.clone(),
            ty: inner_ty.clone(),
            is_mutable: false,
            init: Expr {
                kind: ExprKind::ArrayAccess {
                    array: Box::new(iter_expr.clone()),
                    index: Box::new(Expr { 
                        kind: ExprKind::VariableReference { name: idx_name.clone() }, 
                        ty: Type::I32 
                    })
                },
                ty: inner_ty
            }
        });

        // 4. Lower the user's provided body statements
        for stmt in body {
            loop_body_stmts.push(self.lower_statement(stmt)?);
        }

        // 5. Increment: _idx = _idx + 1;
        loop_body_stmts.push(Stmt::Assign {
            target: ir::stmt::AssignmentTarget::Variable(idx_name.clone()),
            value: Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::ADD,
                    lhs: Box::new(Expr { kind: ExprKind::VariableReference { name: idx_name.clone() }, ty: Type::I32 }),
                    rhs: Box::new(Expr { kind: ExprKind::INT_LITERAL(1), ty: Type::I32 })
                },
                ty: Type::I32
            }
        });

        // 6. Build the While Loop
        let while_loop = Stmt::While {
            cond: Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::LT,
                    lhs: Box::new(Expr { kind: ExprKind::VariableReference { name: idx_name }, ty: Type::I32 }),
                    rhs: Box::new(Expr { kind: ExprKind::INT_LITERAL(array_len), ty: Type::I32 }),
                },
                ty: Type::BOOL,
            },
            body: Block { stmts: loop_body_stmts },
            kind: LoopKind::ForEach,
        };
        outer_stmts.push(while_loop);

        self.leave_scope(); 

        // Wrap the initialization and the loop in a single Block
        Ok(Stmt::Block(Block { stmts: outer_stmts }))
    }
}
