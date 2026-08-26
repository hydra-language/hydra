use super::Analyzer;
use errors::error::HydraError;
use lexer::TokenType;
use parser::ast::Expr as ASTExpr;
use ir::types::Type as IRType;
use ir::context::{DefKind, SymbolInfo};
use ir::hir::{HIRExpr, HIRExprKind, HIRBinOp, HIRUnaryOp, CastKind, HIRBlock, HIRStmt}; 
use crate::utils;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn is_int_type(&self, ty: &IRType) -> bool {
        matches!(
            ty, 
            IRType::I8 | IRType::I16 | IRType::I32 | IRType::I64 | IRType::ISIZE | 
            IRType::U8 | IRType::U16 | IRType::U32 | IRType::U64 | IRType::USIZE
        )
    }

    pub(crate) fn is_float_type(&self, ty: &IRType) -> bool {
        matches!(ty, IRType::F32 | IRType::F64)
    }

    pub(crate) fn coerce_primitive(&self, mut expr: HIRExpr, target: &IRType) -> HIRExpr {
        if expr.ty == *target { return expr; }

        if let HIRExprKind::IntLiteral(val) = expr.kind {
            if self.check_and_promote_int_literal(val, target) {
                expr.ty = target.clone();
                return expr;
            }
        }

        let from_size = self.get_type_size(&expr.ty).unwrap_or(8);
        let to_size = self.get_type_size(target).unwrap_or(8);

        let safe_to_cast = if self.is_float_type(&expr.ty) && self.is_float_type(target) {
            from_size <= to_size 
        } else if self.is_int_type(&expr.ty) && self.is_int_type(target) {
            from_size <= to_size 
        } else if self.is_int_type(&expr.ty) && self.is_float_type(target) {
            true 
        } else {
            false
        };
        
        if safe_to_cast {
            let span = expr.span;
            HIRExpr {
                kind: HIRExprKind::Cast { expr: Box::new(expr), kind: CastKind::Numeric },
                ty: target.clone(),
                span
            }
        } else {
            expr 
        }
    }

    pub(crate) fn lower_expr(&mut self, node: &ASTExpr) -> Result<HIRExpr, HydraError> {
        self.lower_expr_with_type(node, None)
    }

    pub(crate) fn lower_expr_with_type(&mut self, node: &ASTExpr, expected: Option<&IRType>) -> Result<HIRExpr, HydraError> {
        let span = crate::utils::get_expr_span(node);

        match node {
            ASTExpr::Literal { token, .. } => {
                match &token.token_type {
                    TokenType::IntLiteral(val) => {
                        let mut ty = IRType::I32;
                        if let Some(exp) = expected { ty = exp.clone(); }
                        Ok(HIRExpr { kind: HIRExprKind::IntLiteral(*val), ty, span })
                    },
                    TokenType::FloatLiteral(val) => {
                        let mut ty = IRType::F64; 
                        if let Some(exp) = expected {
                            if matches!(exp, IRType::F32 | IRType::F64) { ty = exp.clone(); }
                        }
                        Ok(HIRExpr { kind: HIRExprKind::FloatLiteral(*val), ty, span })
                    },
                    TokenType::StringLiteral(ref s) => Ok(HIRExpr {
                        kind: HIRExprKind::StringLiteral(s.clone()),
                        ty: IRType::ARRAY(Box::new(IRType::U8), s.len()),
                        span
                    }),
                    TokenType::CharLiteral(c) => Ok(HIRExpr { kind: HIRExprKind::CharLiteral(*c), ty: IRType::CHAR, span }),
                    TokenType::BoolLiteral(b) => Ok(HIRExpr { kind: HIRExprKind::BoolLiteral(*b), ty: IRType::BOOL, span }),
                    _ => Err(self.error("S003", format!("unexpected literal: {:?}", token.token_type), token.span))
                }
            },

            ASTExpr::Variable { id, name } => {
                let def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", format!("undefined variable `{}`", name.lexeme), name.span))?;
                
                let info = self.context.get_def(def_id).unwrap();
                let ty = match &info.kind {
                    DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } | DefKind::Function { return_type: ty, .. } => ty.clone(),
                    _ => return Err(self.error("S003", format!("`{}` cannot be used as a value", name.lexeme), name.span))
                };
                Ok(HIRExpr { kind: HIRExprKind::VarRef(def_id), ty, span })
            },

            ASTExpr::Path { id, segments } => {
                let def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", format!("undefined path `{}`", segments[0].lexeme), span))?;
                
                let info = self.context.get_def(def_id).unwrap();
                let ty = match &info.kind {
                    DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } | DefKind::Function { return_type: ty, .. } => ty.clone(),
                    DefKind::Struct { .. } => IRType::STRUCT(info.absolute_path.join("::")),
                    _ => return Err(self.error("S003", format!("`{}` cannot be used as a value", segments[0].lexeme), span))
                };
                Ok(HIRExpr { kind: HIRExprKind::VarRef(def_id), ty, span })
            },

            ASTExpr::ArrayInitializer { elements, .. } => {
                let mut ir_elements = Vec::new();
                let inner_expected = match expected {
                    Some(IRType::ARRAY(inner, _)) => Some(&**inner),
                    _ => None,
                };
                for element in elements {
                    let mut ir_element = self.lower_expr_with_type(element, inner_expected)?;
                    if let Some(target) = inner_expected {
                        ir_element = self.coerce_primitive(ir_element, target);
                    }
                    ir_elements.push(ir_element);
                }

                let element_type = if let Some(target) = inner_expected {
                    target.clone()
                } else {
                    ir_elements.first().map(|e| e.ty.clone()).ok_or_else(|| self.error("S007", "cannot infer type of array", span))?
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::ArrayInit { elements: ir_elements },
                    ty: IRType::ARRAY(Box::new(element_type), elements.len()),
                    span,
                })
            },

            ASTExpr::ArrayAccess { array, index, .. } => {
                let mut arr = self.lower_expr_with_type(array, None)?;

                let idx_expr = self.lower_expr_with_type(
                    index,
                    Some(&IRType::USIZE),
                )?;

                if !idx_expr.ty.is_numeric() {
                    return Err(self.error(
                        "S001",
                        format!(
                            "index must be numeric, found {}",
                            idx_expr.ty
                        ),
                        span,
                    ));
                }

                //
                // indexing auto-dereferences references:
                //
                //     &[T]     -> [T]
                //     &mut [T] -> [T]
                //
                // do this in HIR rather than merely inspecting the inner type,
                // because MIR needs the Deref projection too.
                //
                loop {
                    let inner = match &arr.ty {
                        IRType::REF(inner) | IRType::CONST_REF(inner) => {
                            Some(inner.as_ref().clone())
                        }

                        _ => None,
                    };

                    let Some(inner_ty) = inner else {
                        break;
                    };

                    arr = HIRExpr {
                        kind: HIRExprKind::Dereference {
                            target: Box::new(arr),
                        },
                        ty: inner_ty,
                        span,
                    };
                }

                match arr.ty.clone() {
                    IRType::ARRAY(inner, size) => {
                        if let HIRExprKind::IntLiteral(idx_val) = &idx_expr.kind {
                            if *idx_val < 0 || *idx_val >= size as i64 {
                                return Err(self.error(
                                    "S008",
                                    format!(
                                        "index out of bounds: len is {} but index is {}",
                                        size,
                                        idx_val
                                    ),
                                    span,
                                ));
                            }
                        }

                        Ok(HIRExpr {
                            kind: HIRExprKind::ArrayAccess {
                                array: Box::new(arr),
                                index: Box::new(idx_expr),
                            },
                            ty: *inner,
                            span,
                        })
                    }

                    IRType::INFERRED_ARRAY(inner) | IRType::SLICE(inner) => {
                        Ok(HIRExpr {
                            kind: HIRExprKind::ArrayAccess {
                                array: Box::new(arr),
                                index: Box::new(idx_expr),
                            },
                            ty: *inner,
                            span,
                        })
                    }

                    _ => Err(self.error(
                        "S003",
                        format!(
                            "type '{}' cannot be indexed",
                            arr.ty
                        ),
                        span,
                    )),
                }
            }

            ASTExpr::StructInitializer { name, fields, .. } => {
                let name_id = utils::get_expr_id(name);
                let def_id = self.name_resolver.get_resolution(name_id)
                    .ok_or_else(|| self.error("S002", "undefined struct", span))?;

                let info = self.context.get_def(def_id).unwrap();
                let absolute_struct_name = info.absolute_path.join("::");

                let def_fields = match &info.kind {
                    DefKind::Struct { fields, .. } => fields.clone(),
                    _ => return Err(self.error("S002", "not a struct", span)),
                }; 

                let mut lowered_values = Vec::new();

                for (def_name, def_type, is_const) in &def_fields { 
                    if *is_const { continue; }

                    if let Some((_, value_node)) = fields.iter().find(|(f_token, _)| f_token.lexeme == *def_name) {
                        let mut val = self.lower_expr_with_type(value_node, Some(def_type))?;
                        val = self.coerce_primitive(val, def_type);

                        if !self.check_type_compatibility(def_type, &val.ty) {
                            return Err(self.error("S001", format!("field '{}' expected {}, found {}", def_name, def_type, val.ty), span));
                        }
                        lowered_values.push(val);
                    } else {
                        return Err(self.error("S005", format!("missing field '{}'", def_name), span));
                    }
                }

                Ok(HIRExpr {
                    kind: HIRExprKind::StructInit { def_id, values: lowered_values },
                    ty: IRType::STRUCT(absolute_struct_name),
                    span,
                })
            },

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

            ASTExpr::Assignment { target, operator, value, .. } => {
                let lowered_target = self.lower_expr_with_type(target, None)?;
                self.check_assignable_place(&lowered_target)?;

                let mut lowered_value = self.lower_expr_with_type(
                    value,
                    Some(&lowered_target.ty)
                )?;

                lowered_value = self.coerce_primitive(lowered_value, &lowered_target.ty);

                let assign_value = if let Some(bin_op) = utils::get_binary_op_from_token(&operator.token_type)
                {
                    HIRExpr {
                        kind: HIRExprKind::Binary {
                            op: bin_op,
                            lhs: Box::new(lowered_target.clone()),
                            rhs: Box::new(lowered_value.clone()),
                        },
                        ty: lowered_target.ty.clone(),
                        span: operator.span,
                    }
                } else {
                    lowered_value
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(lowered_target.clone()),
                        value: Box::new(assign_value),
                    },
                    ty: lowered_target.ty,
                    span,
                })
            },

            ASTExpr::Member { object, property, .. } => {
                let lhs = self.lower_expr_with_type(object, None)?;
                
                let actual_type = match &lhs.ty {
                    IRType::REF(inner) | IRType::CONST_REF(inner) => inner.as_ref(),
                    _ => &lhs.ty
                };
                
                let lookup_type = match actual_type {
                    IRType::GENERIC_INSTANCE(base, _) => base.as_ref(),
                    other => other,
                };

                match lookup_type {
                    IRType::STRUCT(name) => {
                        if let Some(def_id) = self.global_symbols.get(&name.split("::").map(|s| s.to_string()).collect::<Vec<_>>()) {
                            if let Some(info) = self.context.get_def(*def_id) {
                                if let DefKind::Struct { fields, .. } = &info.kind {
                                    if let Some(idx) = fields.iter().position(|(field_name, _, _)| field_name == &property.lexeme) {
                                        let (_, field_type, _) = &fields[idx];
                                        return Ok(HIRExpr {
                                            kind: HIRExprKind::FieldAccess { object: Box::new(lhs), field_index: idx },
                                            ty: field_type.clone(),
                                            span
                                        });
                                    }
                                }
                            }
                        }
                        Err(self.error("S005", format!("struct '{}' has no field '{}'", name, property.lexeme), property.span))
                    },
                    IRType::ARRAY(_, size) if property.lexeme == "len" => Ok(HIRExpr { kind: HIRExprKind::IntLiteral(*size as i64), ty: IRType::I32, span }),
                    _ => Err(self.error("S005", format!("'{}' has no property '{}'", lhs.ty, property.lexeme), property.span))
                }
            },

            ASTExpr::MethodCall { object, method, arguments, generic_args, .. } => {
                let lhs_expr = self.lower_expr_with_type(object, None)?;
                self.lower_instance_method_call(lhs_expr, &method.lexeme, arguments, generic_args, span)
            }

            ASTExpr::FunctionCall { callee, arguments, generic_args, .. } => {
                let call_name_debug = match &**callee {
                    ASTExpr::Variable { name, .. } => name.lexeme.to_string(),
                    ASTExpr::Path { segments, .. } => segments.iter().map(|s| s.lexeme.as_str()).collect::<Vec<_>>().join("::"),
                    _ => "".to_string()
                };

                if call_name_debug == "print" || call_name_debug == "println" {
                    let mut args = Vec::new();
                    for arg in arguments { args.push(self.lower_expr(arg)?); }
                    return Ok(HIRExpr {
                        kind: HIRExprKind::BuiltinCall { name: call_name_debug, args },
                        ty: IRType::VOID,
                        span
                    });
                }

                // fetch the ID of the callee expression, not the outer FunctionCall
                let callee_id = crate::utils::get_expr_id(callee);
                let def_id = self.name_resolver.get_resolution(callee_id)
                    .ok_or_else(|| self.error("S002", format!("undefined function `{}`", call_name_debug), span))?;    

                let info = self.context.get_def(def_id).cloned().ok_or_else(|| 
                    {
                        self.error("S002", format!("missing definition for `{}`", call_name_debug), span)
                    }
                )?;

                // a path such as:
                //
                //     math::multiply(...)
                //
                // may have resolved to the local value `math`.
                //
                // in that case the final path segment is a method name,
                // not part of the receiver's definition path.
                if matches!(info.kind, DefKind::Variable { .. } | DefKind::Constant { .. }) {
                    if let ASTExpr::Path { segments, .. } = &**callee {
                        if segments.len() >= 2 {
                            let method_name = &segments.last().unwrap().lexeme;

                            // lower_expr(Path) is safe here:
                            //
                            // the resolver associated this path's NodeID
                            // with the receiver's DefID, so this produces
                            // VarRef(math), not a reference to multiply.
                            let lhs_expr = self.lower_expr_with_type(callee, None)?;
                            return self.lower_instance_method_call(
                                lhs_expr,
                                method_name,
                                arguments,
                                generic_args,
                                span,
                            );
                        }
                    }
                }

                let actual_def_id = if let DefKind::Struct { .. } = info.kind {
                    if let ASTExpr::Path { segments, .. } = &**callee {
                        let method_name = &segments.last().unwrap().lexeme;
                        let struct_name = info.absolute_path.join("::");

                        if let Some(type_methods) = self.impl_registry.get(&struct_name) {
                            if let Some(&m_def_id) = type_methods.get(method_name) {
                                m_def_id
                            } else {
                                return Err(self.error("S005", format!("struct `{}` has no associated function `{}`", struct_name, method_name), span));
                            }
                        } else {
                            return Err(self.error("S005", format!("struct `{}` has no associated function `{}`", struct_name, method_name), span));
                        }
                    } else {
                        return Err(self.error("S003", format!("target is a struct, not a function"), span));
                    }
                } else {
                    def_id
                };

                let actual_info = self.context.get_def(actual_def_id).unwrap();

                let (param_types, return_type, intrinsic) =
                match &actual_info.kind {
                    DefKind::Function {
                        params,
                        return_type,
                        intrinsic,
                        ..
                    } => (
                        params.clone(),
                        return_type.clone(),
                        *intrinsic,
                    ),

                    _ => {
                        return Err(self.error(
                            "S003",
                            "target is not a function",
                            span,
                        ));
                    }
                };

                let mut args = Vec::new();
                for (i, node) in arguments.iter().enumerate() {
                    let expected = param_types.get(i).and_then(|t| if matches!(t, IRType::GENERIC(_)) { None } else { Some(t) });
                    let mut arg = self.lower_expr_with_type(node, expected)?;
                    if let Some(target) = expected { arg = self.coerce_primitive(arg, target); }
                    args.push(arg);
                }

                let mut lowered_generics = Vec::new();
                for node in generic_args { lowered_generics.push(self.lower_type(node)?); }

                if let Some(kind) = intrinsic {
                    return Ok(HIRExpr {
                        kind: HIRExprKind::IntrinsicCall {
                            callee: actual_def_id,
                            kind,
                            args,
                            type_args: lowered_generics,
                        },
                        ty: return_type,
                        span,
                    });
                }

                Ok(HIRExpr { 
                    kind: HIRExprKind::Call {
                        callee: actual_def_id,
                        args,
                        generic_args: lowered_generics,
                    },
                    ty: return_type,
                    span,
                })
            },

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

            ASTExpr::Borrow { is_mut, right, .. } => {
                let target_expr = self.lower_expr(right)?;
                let ty = if *is_mut { 
                    IRType::REF(Box::new(target_expr.ty.clone())) 
                } else { 
                    IRType::CONST_REF(Box::new(target_expr.ty.clone())) 
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Borrow { is_mut: *is_mut, target: Box::new(target_expr) },
                    ty,
                    span, 
                })
            }

            ASTExpr::Dereference { right, .. } => {
                let target_expr = self.lower_expr(right)?;

                let inner_ty = match &target_expr.ty {
                    IRType::REF(inner) | IRType::CONST_REF(inner) | 
                    IRType::POINTER(inner) | IRType::CONST_POINTER(inner) => *inner.clone(),
                    _ => return Err(self.error("T004", format!("cannot dereference type `{}`", target_expr.ty), span)),
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Dereference { target: Box::new(target_expr) },
                    ty: inner_ty,
                    span,
                })
            }

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
            },

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
                let arr_info = SymbolInfo { name: arr_name.clone(), span: item.span, absolute_path: vec![arr_name.clone()], kind: DefKind::Variable { ty: iter_expr.ty.clone(), is_mutable: false }, is_pub: false };
                let arr_def = self.context.insert_def(arr_info);
                let init_arr = HIRStmt::VarDecl { def_id: arr_def, init: Some(iter_expr.clone()), span: item.span };

                // Create index def
                let idx_name = format!("_idx_{}", item.span.line);
                let idx_info = SymbolInfo { name: idx_name.clone(), span: item.span, absolute_path: vec![idx_name.clone()], kind: DefKind::Variable { ty: IRType::I32, is_mutable: true }, is_pub: false };
                let idx_def = self.context.insert_def(idx_info);
                let init_idx = HIRStmt::VarDecl { def_id: idx_def, init: Some(HIRExpr { kind: HIRExprKind::IntLiteral(0), ty: IRType::I32, span: item.span }), span: item.span };

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
                        then_block: Box::new(HIRBlock { stmts: vec![HIRStmt::Expr(HIRExpr { kind: HIRExprKind::Break, ty: IRType::VOID, span: item.span })], span: item.span }),
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
            
            _ => Err(self.error("S006", format!("expression not supported: {:?}", node), span)),
        }
    }

    fn lower_instance_method_call(
        &mut self,
        lhs_expr: HIRExpr,
        method_name: &str,
        arguments: &[ASTExpr],
        generic_args: &[parser::ast::Type],
        span: errors::error::Span,
    ) -> Result<HIRExpr, HydraError> 
    {
        let actual_type = match &lhs_expr.ty {
            IRType::REF(inner) | IRType::CONST_REF(inner) | 
            IRType::POINTER(inner) | IRType::CONST_POINTER(inner) => inner.as_ref().clone(),

            _ => lhs_expr.ty.clone(),
        };

        let lookup_type = match &actual_type {
            IRType::GENERIC_INSTANCE(base, _) => *base.clone(),
            other => other.clone(),
        };

        let registry_key = self.get_impl_registry_key(&lhs_expr.ty);

        if registry_key.is_empty() {
            return Err(self.error(
                "S005",
                format!(
                    "type '{}' has no methods",
                    lhs_expr.ty
                ),
                span,
            ));
        }

        let method_def_id = {
            let type_methods = self
                .impl_registry
                .get(&registry_key)
                .ok_or_else(|| {
                    self.error(
                        "S005",
                        format!(
                            "type '{}' has no methods",
                            lhs_expr.ty
                        ),
                        span,
                    )
                })?;

            *type_methods
                .get(method_name)
                .ok_or_else(|| {
                    self.error(
                        "S005",
                        format!(
                            "method '{}' not found for type '{}'",
                            method_name,
                            lhs_expr.ty
                        ),
                        span,
                    )
                })?
        };

        let info = self
            .context
            .get_def(method_def_id)
            .ok_or_else(|| {
                self.error(
                    "S003",
                    format!("method '{}' has no definition", method_name),
                    span,
                )
            })?;

        let (param_types, return_type) = match &info.kind {
            DefKind::Function {
                params,
                return_type,
                ..
            } => (
                params.clone(),
                return_type.clone(),
            ),

            _ => {
                return Err(self.error(
                    "S003",
                    format!("'{}' is not a function", method_name),
                    span,
                ));
            }
        };

        let expected_self_ty = param_types
            .first()
            .ok_or_else(|| {
                self.error(
                    "S004",
                    format!("method '{}' does not accept self", method_name),
                    span,
                )
            })?;


        let self_arg = if self.receiver_type_matches(expected_self_ty, &lhs_expr.ty) {
            //
            // already has the form the method expects.
            //
            // &[i32] passed to &[T]
            //
            lhs_expr
        } else {
            match expected_self_ty {
                IRType::REF(_) => {
                    let actual = lhs_expr.ty.clone();

                    HIRExpr {
                        kind: HIRExprKind::Borrow {
                            is_mut: true,
                            target: Box::new(lhs_expr),
                        },
                        ty: IRType::REF(
                            Box::new(actual),
                        ),
                        span,
                    }
                }

                IRType::CONST_REF(_) => {
                    let actual = lhs_expr.ty.clone();

                    HIRExpr {
                        kind: HIRExprKind::Borrow {
                            is_mut: false,
                            target: Box::new(lhs_expr),
                        },
                        ty: IRType::CONST_REF(
                            Box::new(actual),
                        ),
                        span,
                    }
                }

                _ => lhs_expr,
            }
        };

        let expected_user_args = param_types.len().saturating_sub(1);

        if arguments.len() != expected_user_args {
            return Err(self.error(
                "S004",
                format!(
                    "method '{}' expected {} argument{}, found {}",
                    method_name,
                    expected_user_args,
                    if expected_user_args == 1 { "" } else { "s" },
                    arguments.len(),
                ),
                span,
            ));
        }

        let mut args = Vec::new();

        // self is always argument zero.
        args.push(self_arg);

        for arg_node in arguments {
            let expected_ty = param_types.get(args.len());

            let mut lowered_arg =
            self.lower_expr_with_type(
                arg_node,
                expected_ty,
            )?;

            if let Some(target) = expected_ty {
                lowered_arg =
                    self.coerce_primitive(
                        lowered_arg,
                        target,
                    );
            }

            args.push(lowered_arg);
        }

        let mut lowered_generics = Vec::new();

        for node in generic_args {
            lowered_generics.push(
                self.lower_type(node)?
            );
        }

        Ok(HIRExpr {
            kind: HIRExprKind::Call {
                callee: method_def_id,
                args,
                generic_args: lowered_generics,
            },
            ty: return_type,
            span,
        })
    }

    fn check_assignable_place(&self, expr: &HIRExpr) -> Result<(), HydraError> {
        match &expr.kind {
            //
            // x = ...
            //
            HIRExprKind::VarRef(def_id) => {
                let info = self
                    .context
                    .get_def(*def_id)
                    .ok_or_else(|| {
                        self.error(
                            "S002",
                            "missing definition for assignment target",
                            expr.span,
                        )
                    })?;

                match &info.kind {
                    DefKind::Variable {
                        is_mutable: true,
                        ..
                    } => Ok(()),

                    DefKind::Variable {
                        is_mutable: false,
                        ..
                    }
                    | DefKind::Constant { .. } => {
                        Err(self.error(
                            "S009",
                            format!(
                                "cannot assign to immutable binding `{}`",
                                info.name
                            ),
                            expr.span,
                        ))
                    }

                    _ => Err(self.error(
                        "S009",
                        "invalid assignment target",
                        expr.span,
                    )),
                }
            }

            //
            // *p = ...
            //
            // IMPORTANT:
            //
            // do not recurse into `target` here.
            //
            //     const p: *mut i32 = ...;
            //     *p = 42;
            //
            // `p` itself cannot be rebound, but the memory reachable through
            // the *mut pointer is writable.
            //
            HIRExprKind::Dereference { target } => {
                match &target.ty {
                    IRType::POINTER(_) | IRType::REF(_) => Ok(()),

                    IRType::CONST_POINTER(_) => {
                        Err(self.error(
                            "S009",
                            "cannot assign through immutable pointer",
                            expr.span,
                        ))
                    }

                    IRType::CONST_REF(_) => {
                        Err(self.error(
                            "S009",
                            "cannot assign through immutable reference",
                            expr.span,
                        ))
                    }

                    _ => Err(self.error(
                        "S009",
                        "invalid assignment target",
                        expr.span,
                    )),
                }
            }

            //
            // foo.field = ...
            //
            HIRExprKind::FieldAccess { object, .. } => {
                self.check_projection_base_assignable(object)
            }

            //
            // array[index] = ...
            //
            HIRExprKind::ArrayAccess { array, .. } => {
                self.check_projection_base_assignable(array)
            }

            _ => Err(self.error(
                "S009",
                "invalid assignment target",
                expr.span,
            )),
        }
    }

    fn check_projection_base_assignable(&self, base: &HIRExpr) -> Result<(), HydraError> {
        match &base.ty {
            //
            // field access through a mutable ref/pointer is an implicit
            // dereference boundary:
            //
            //     const p: &mut Foo = ...;
            //     p.value = 42;
            //
            // the binding `p` is immutable, but its pointee is mutable.
            //
            IRType::REF(_) | IRType::POINTER(_) => Ok(()),

            IRType::CONST_REF(_) => {
                Err(self.error(
                    "S009",
                    "cannot assign through immutable reference",
                    base.span,
                ))
            }

            IRType::CONST_POINTER(_) => {
                Err(self.error(
                    "S009",
                    "cannot assign through immutable pointer",
                    base.span,
                ))
            }

            //
            // ordinary projection:
            //
            //     x.field
            //     x[index]
            //
            // inherits assignability from x.
            //
            _ => self.check_assignable_place(base),
        }
    }

    fn receiver_type_matches(&self, expected: &IRType, actual: &IRType) -> bool {
        match (expected, actual) {
            //
            // a generic can unify with any concrete type.
            //
            (IRType::GENERIC(_), _) => true,

            (IRType::REF(expected), IRType::REF(actual)) | (IRType::CONST_REF(expected), IRType::CONST_REF(actual)) | 
            (IRType::CONST_REF(expected), IRType::REF(actual)) | (IRType::POINTER(expected), IRType::POINTER(actual)) | 
            (IRType::CONST_POINTER(expected), IRType::CONST_POINTER(actual)) | (IRType::SLICE(expected), IRType::SLICE(actual)) => 
            {
                self.receiver_type_matches(expected, actual)
            }

            (IRType::ARRAY(expected, expected_len), IRType::ARRAY(actual, actual_len)) => {
                expected_len == actual_len && self.receiver_type_matches(expected, actual)
            }

            (IRType::GENERIC_INSTANCE(expected_base, expected_args), IRType::GENERIC_INSTANCE(actual_base, actual_args)) => 
            {
                self.receiver_type_matches(expected_base, actual_base) && expected_args.len() == actual_args.len() && 
                expected_args.iter().zip(actual_args).all(|(expected, actual)| {
                    self.receiver_type_matches(expected, actual)
                })
            }

            _ => expected == actual,
        }
    }

}
