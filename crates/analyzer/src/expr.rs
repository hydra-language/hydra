use super::Analyzer;
use errors::error::HydraError;
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use ir::types::Type;
use ir::context::DefKind;
use ir::hir::{HIRExpr, HIRExprKind, HIRBinOp, HIRUnaryOp, CastKind}; 

impl Analyzer {

    pub(crate) fn is_int_type(&self, ty: &Type) -> bool {
        matches!(
            ty, 
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::ISIZE | 
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::USIZE
        )
    }

    pub(crate) fn is_float_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::F32 | Type::F64)
    }

    pub(crate) fn coerce_primitive(&self, mut expr: HIRExpr, target: &Type) -> HIRExpr {
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

    pub(crate) fn lower_expression(&mut self, node: ASTNode) -> Result<HIRExpr, HydraError> {
        self.lower_expr_with_type(node, None)
    }

    pub(crate) fn lower_expr_with_type(&mut self, node: ASTNode, expected: Option<&Type>) -> Result<HIRExpr, HydraError> {
        let span = self.get_token_from_node(&node).span;

        match node {
            ASTNode::Expression { token } => {
                match token.token_type {
                    TokenType::IntLiteral(val) => {
                        let mut ty = Type::I32;
                        if let Some(exp) = expected { ty = exp.clone(); }
                        Ok(HIRExpr { kind: HIRExprKind::IntLiteral(val), ty, span })
                    },
                    TokenType::FloatLiteral(val) => {
                        let mut ty = Type::F64; 
                        if let Some(exp) = expected {
                            if matches!(exp, Type::F32 | Type::F64) { ty = exp.clone(); }
                        }
                        Ok(HIRExpr { kind: HIRExprKind::FloatLiteral(val), ty, span })
                    },
                    TokenType::StringLiteral(ref s) => Ok(HIRExpr {
                        kind: HIRExprKind::StringLiteral(s.clone()),
                        ty: Type::ARRAY(Box::new(Type::U8), s.len()),
                        span
                    }),
                    TokenType::CharLiteral(c) => Ok(HIRExpr { kind: HIRExprKind::CharLiteral(c), ty: Type::CHAR, span }),
                    TokenType::BoolLiteral(b) => Ok(HIRExpr {
                        kind: HIRExprKind::BoolLiteral(b), ty: Type::BOOL, span
                    }),
                    _ => Err(self.error("S003", format!("unexpected literal: {:?}", token.token_type), token.span))
                }
            },

            ASTNode::ArrayInitializer { elements, token } => {
                let mut ir_elements = Vec::new();
                let inner_expected = match expected {
                    Some(Type::ARRAY(inner, _)) => Some(&**inner),
                    _ => None,
                };
                for element in &elements {
                    let mut ir_element = self.lower_expr_with_type(element.clone(), inner_expected)?;
                    if let Some(target) = inner_expected {
                        ir_element = self.coerce_primitive(ir_element, target);
                    }
                    ir_elements.push(ir_element);
                }

                let element_type = if let Some(target) = inner_expected {
                    target.clone()
                } else {
                    ir_elements.first().map(|e| e.ty.clone()).ok_or_else(|| {
                        self.error("S007", "cannot infer type of array", token.span)
                    })?
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::ArrayInit { elements: ir_elements },
                    ty: Type::ARRAY(Box::new(element_type.clone()), elements.len()),
                    span,
                })
            },

            ASTNode::ArrayAccess { array, index, token } => {
                let arr = self.lower_expr_with_type(*array, None)?;
                let idx_expr = self.lower_expr_with_type(*index, Some(&Type::USIZE))?;

                if !idx_expr.ty.is_numeric() {
                    return Err(self.error("S001", format!("index must be numeric, found {}", idx_expr.ty), token.span));
                }

                match &arr.ty.clone() {
                    Type::ARRAY(inner, size) => {
                        if let HIRExprKind::IntLiteral(idx_val) = idx_expr.kind {
                            if idx_val < 0 || idx_val >= (*size as i64) {
                                return Err(self.error("S008", format!("index out of bounds: len is {} but index is {}", size, idx_val), token.span));
                            }
                        }
                        Ok(HIRExpr { kind: HIRExprKind::ArrayAccess { array: Box::new(arr), index: Box::new(idx_expr) }, ty: *inner.clone(), span })
                    },
                    Type::INFERRED_ARRAY(inner) => Ok(HIRExpr {
                        kind: HIRExprKind::ArrayAccess { array: Box::new(arr), index: Box::new(idx_expr) }, ty: *inner.clone(), span
                    }),
                    _ => Err(self.error("S003", format!("type '{}' cannot be indexed", arr.ty), token.span))
                }
            },

            ASTNode::PathExpression { segments } => {
                let path_strings: Vec<String> = segments.iter().map(|s| s.lexeme.to_string()).collect();
                let abs_path = self.scope.resolve_path(&path_strings, &self.context);

                if let Some(def_id) = self.global_symbols.get(&abs_path) {
                    if let Some(info) = self.context.get_def(*def_id) {
                        match &info.kind {
                            DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } => {
                                return Ok(HIRExpr { kind: HIRExprKind::VarRef(*def_id), ty: ty.clone(), span });
                            }
                            DefKind::Function { return_type: ty, .. } => {
                                return Ok(HIRExpr { kind: HIRExprKind::VarRef(*def_id), ty: ty.clone(), span });
                            }
                            DefKind::Struct { .. } => {
                                // For now, return a dummy expression or handle constructor logic.
                                // If this path is used for a call (Rectangle::new), 
                                // the MethodCallExpression handler will take over later.
                                return Ok(HIRExpr { kind: HIRExprKind::VarRef(*def_id), ty: Type::STRUCT(abs_path.join("::")), span });
                            }
                            _ => {}
                        }
                    }
                }

                Err(self.error("S002", format!("undefined path: {}", path_strings.join("::")), span))
            },

            ASTNode::StructInitializer { name, fields } => {
               let struct_name = match &*name {
                    ASTNode::VariableExpression { name: n } => n.lexeme.to_string(),
                    ASTNode::PathExpression { segments } => segments.iter().map(|s| s.lexeme).collect::<Vec<_>>().join("::"),
                    _ => return Err(self.error("S002", "invalid struct name", span)),
                };

                let def_id = self.scope.resolve(&struct_name, &self.context)
                    .ok_or_else(|| self.error("S002", format!("'{}' is not a defined struct", struct_name), span))?;

                let info = self.context.get_def(def_id).unwrap();
                let absolute_struct_name = info.absolute_path.join("::");

                let def_fields = match &info.kind {
                    DefKind::Struct { fields, .. } => fields.clone(),
                    _ => return Err(self.error("S002", format!("'{}' is not a struct", struct_name), span)),
                }; 

                let mut lowered_values = Vec::new();

                for (def_name, def_type, is_const) in &def_fields { 
                    if *is_const { continue; }

                    if let Some((_, value_node)) = fields.iter().find(|(f_token, _)| f_token.lexeme == *def_name) {
                        let mut val = self.lower_expr_with_type(*value_node.clone(), Some(def_type))?;
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
                    ty: Type::STRUCT(absolute_struct_name),
                    span,
                })
            },

            ASTNode::BinaryExpression { left, operator, right } => {
                let mut lhs = self.lower_expr_with_type(*left, expected)?;
                let mut rhs = self.lower_expr_with_type(*right, Some(&lhs.ty))?;

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
                    TokenType::LeftAngle => (HIRBinOp::Lt, Type::BOOL),
                    TokenType::LessEqual => (HIRBinOp::Le, Type::BOOL),
                    TokenType::RightAngle => (HIRBinOp::Gt, Type::BOOL),
                    TokenType::GreaterEqual => (HIRBinOp::Ge, Type::BOOL),
                    TokenType::DoubleEqual => (HIRBinOp::Eq, Type::BOOL),
                    TokenType::ExclamEqual => (HIRBinOp::Ne, Type::BOOL),
                    TokenType::DoubleAmpersand => (HIRBinOp::And, Type::BOOL),
                    TokenType::DoublePipe => (HIRBinOp::Or,  Type::BOOL),
                    _ => return Err(self.error("S003", format!("unknown op: {}", operator.lexeme), operator.span))
                };
                
                Ok(HIRExpr { 
                    kind: HIRExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, 
                    ty,
                    span
                })
            },

            ASTNode::AssignmentExpression { target, operator, value } => {
                let lowered_target = self.lower_expr_with_type(*target, None)?;
                let mut lowered_value = self.lower_expr_with_type(*value, Some(&lowered_target.ty))?;
                lowered_value = self.coerce_primitive(lowered_value, &lowered_target.ty);
                
                // Desugar +=, -=, etc immediately
                let assign_value = if let Some(bin_op) = match operator.token_type {
                    TokenType::PlusEqual => Some(HIRBinOp::Add),
                    TokenType::MinusEqual => Some(HIRBinOp::Sub),
                    TokenType::StarEqual => Some(HIRBinOp::Mul),
                    TokenType::ForwardSlashEqual => Some(HIRBinOp::Div),
                    _ => None,
                } {
                    HIRExpr {
                        kind: HIRExprKind::Binary {
                            op: bin_op,
                            lhs: Box::new(lowered_target.clone()),
                            rhs: Box::new(lowered_value.clone())
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

            ASTNode::VariableExpression { name, .. } => {
                let def_id = self.scope.resolve(name.lexeme, &self.context)
                    .ok_or_else(|| self.error("S002", format!("undefined variable: {}", name.lexeme), name.span))?;
                let info = self.context.get_def(def_id).unwrap();
                match &info.kind {
                    DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } | DefKind::Function { return_type: ty, .. } => {
                        Ok(HIRExpr { kind: HIRExprKind::VarRef(def_id), ty: ty.clone(), span })
                    },
                    _ => Err(self.error("S003", format!("'{}' is not a variable", name.lexeme), name.span))
                }
            },

            ASTNode::MemberExpression { object, property } => {
                let lhs = self.lower_expr_with_type(*object, None)?;
                
                let actual_type = match &lhs.ty {
                    Type::REF(inner) | Type::CONST_REF(inner) => inner.as_ref(),
                    _ => &lhs.ty
                };
                
                let lookup_type = match actual_type {
                    Type::GENERIC_INSTANCE(base, _) => base.as_ref(),
                    other => other,
                };

                match lookup_type {
                    Type::STRUCT(name) => {
                        if let Some(def_id) = self.scope.resolve(name, &self.context) {
                            if let Some(info) = self.context.get_def(def_id) {
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
                        } else {
                            return Err(self.error("S002", format!("ICE: absolute struct '{}' not found in global_symbols during field access", name), span));
                        }

                        Err(self.error("S005", format!("struct '{}' has no field '{}'", name, property.lexeme), property.span))
                    },

                    Type::ARRAY(_, size) if property.lexeme == "len" => Ok(HIRExpr { kind: HIRExprKind::IntLiteral(*size as i64), ty: Type::I32, span }),
                    _ => Err(self.error("S005", format!("'{}' has no property '{}'", lhs.ty, property.lexeme), property.span))
                }
            },

            ASTNode::MethodCallExpression { object, method, arguments, generic_args } => {
                let method_name = method.lexeme;
                let span = method.span;

                let lhs_expr = self.lower_expr_with_type(*object.clone(), None)?;
                let actual_type = match &lhs_expr.ty {
                    Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) => inner.as_ref().clone(),
                    _ => lhs_expr.ty.clone()
                };

                let lookup_type = match &actual_type {
                    Type::GENERIC_INSTANCE(base, _) => *base.clone(),
                    other => other.clone(),
                };

                let method_def_id = {
                    let struct_name = match &lookup_type {
                        Type::STRUCT(name) => name.clone(),
                        _ => return Err(self.error("S005", format!("type '{}' has no methods", actual_type), span)),
                    };
                    let type_methods = self.impl_registry.get(&struct_name)
                        .ok_or_else(|| self.error("S005", format!("type '{}' has no methods", actual_type), span))?;

                    *type_methods.get(method_name)
                        .ok_or_else(|| self.error("S005", format!("method '{}' not found", method_name), span))?
                };
                
                let info = self.context.get_def(method_def_id).unwrap();
                let (param_types, return_type) = match &info.kind {
                    DefKind::Function { params, return_type, .. } => (params.clone(), return_type.clone()),
                    _ => return Err(self.error("S003", format!("'{}' is not a function", method_name), span)),
                };
                
                let mut args = Vec::new();
                let expected_self_ty = param_types.first().ok_or_else(|| self.error("S004", "method expects self", span))?;

                let self_arg = match (expected_self_ty, &lhs_expr.ty) {
                    (Type::REF(inner), actual) | (Type::CONST_REF(inner), actual) 
                    if inner.as_ref() == actual => 
                    {
                        let is_mut = matches!(expected_self_ty, Type::REF(_));

                        let ty = if is_mut {
                            Type::REF(Box::new(lhs_expr.ty.clone()))
                        } else {
                            Type::CONST_REF(Box::new(lhs_expr.ty.clone()))
                        };

                        HIRExpr {
                            kind: HIRExprKind::Borrow { is_mut, target: Box::new(lhs_expr) },
                            ty,
                            span
                        }
                    },

                    _ => lhs_expr
                };

                args.push(self_arg);

                for arg_node in arguments {
                    let expected_ty = param_types.get(args.len());
                    let mut lowered_arg = self.lower_expr_with_type(arg_node, expected_ty)?;

                    if let Some(target) = expected_ty {
                        lowered_arg = self.coerce_primitive(lowered_arg, target);
                    }

                    args.push(lowered_arg);
                }

                let mut lowered_generics = Vec::new();
                for node in generic_args { lowered_generics.push(self.lower_type(node)?); }

                Ok(HIRExpr {
                    kind: HIRExprKind::Call { callee: method_def_id, args, generic_args: lowered_generics },
                    ty: return_type,
                    span
                })
            },

            ASTNode::FunctionCallExpression { callee, mut arguments, generic_args } => {
                let mut resolved_def_id = None;
                let call_name_debug;

                match &*callee {
                    ASTNode::VariableExpression { name } => {
                        call_name_debug = name.lexeme.to_string();

                        if call_name_debug == "print" || call_name_debug == "println" {
                            let mut args = Vec::new();

                            for arg in arguments { args.push(self.lower_expression(arg)?); }

                            return Ok(HIRExpr {
                                kind: HIRExprKind::BuiltinCall { name: call_name_debug, args },
                                ty: Type::VOID,
                                span
                            });
                        }

                        if let Some(id) = self.scope.resolve(&call_name_debug, &self.context) {
                            resolved_def_id = Some(id);
                        }
                    },

                    ASTNode::PathExpression { segments } => {
                        let method_name = segments.last().unwrap().lexeme;
                        call_name_debug = segments.iter().map(|s| s.lexeme).collect::<Vec<_>>().join("::");
                        
                        let prefix_strings: Vec<String> = segments[..segments.len() - 1].iter()
                            .map(|t| t.lexeme.to_string())
                            .collect();

                        let absolute_prefix = self.scope.resolve_path(&prefix_strings, &self.context);
                        
                        if let Some(def_id) = self.global_symbols.get(&absolute_prefix) {
                            if let Some(info) = self.context.get_def(*def_id) {
                                if matches!(info.kind, DefKind::Struct { .. }) {
                                    let struct_ty = absolute_prefix.join("::");
                                    if let Some(type_methods) = self.impl_registry.get(&struct_ty) {
                                        if let Some(m_def_id) = type_methods.get(method_name) {
                                            resolved_def_id = Some(*m_def_id);
                                        }
                                    }
                                }
                            }
                        } else if !prefix_strings.is_empty() {
                            let def_id_opt = self.global_symbols.get(&absolute_prefix).copied()
                                .or_else(|| self.scope.resolve(&prefix_strings[0], &self.context));

                            if let Some(def_id) = def_id_opt {
                                if let Some(info) = self.context.get_def(def_id) {
                                    if let DefKind::Variable { ty: prefix_ty, .. } = &info.kind {
                                        let actual_ty = match prefix_ty {
                                            Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) => inner.as_ref().clone(),
                                            _ => prefix_ty.clone()
                                        };

                                        let struct_key = match &actual_ty {
                                            Type::STRUCT(name) => name.clone(),
                                            _ => String::new(),
                                        };
                                        
                                        if let Some(type_methods) = self.impl_registry.get(&struct_key) {
                                            if let Some(m_def_id) = type_methods.get(method_name) {
                                                let prefix_tokens = segments[..segments.len() - 1].to_vec();
                                                let self_node = if prefix_tokens.len() == 1 {
                                                    ASTNode::VariableExpression { name: prefix_tokens[0].clone() }
                                                } else {
                                                    ASTNode::PathExpression { segments: prefix_tokens }
                                                };

                                                let m_info = self.context.get_def(*m_def_id).unwrap();
                                                if let DefKind::Function { params, .. } = &m_info.kind {
                                                    if let Some(expected_self) = params.first() {
                                                        if matches!(expected_self, Type::REF(_) | Type::CONST_REF(_)) 
                                                            && !matches!(prefix_ty, Type::REF(_) | Type::CONST_REF(_)) 
                                                        {
                                                            arguments.insert(0, ASTNode::UnaryExpression {
                                                                operator: Token { token_type: TokenType::Ampersand, lexeme: "&", span: segments[0].span },
                                                                right: Box::new(self_node)
                                                            });
                                                        } else {
                                                            arguments.insert(0, self_node);
                                                        }
                                                    }
                                                }

                                                resolved_def_id = Some(*m_def_id);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if resolved_def_id.is_none() {
                            let abs_path = self.scope.resolve_path(&segments.iter().map(|s| s.lexeme.to_string()).collect::<Vec<_>>(), &self.context);
                            if let Some(id) = self.global_symbols.get(&abs_path) {
                                resolved_def_id = Some(*id);
                            }
                        }
                    },

                    _ => {
                        return Err(self.error("S006", "complex function calls (e.g. function pointers) are not yet supported", span));
                    }
                }

                let def_id = resolved_def_id.ok_or_else(|| self.error("S002", format!("undefined function: {}", call_name_debug), span))?;
                
                let info = self.context.get_def(def_id).unwrap();
                let (param_types, return_type) = match &info.kind {
                    DefKind::Function { params, return_type, .. } => (params.clone(), return_type.clone()),
                    _ => return Err(self.error("S003", format!("'{}' is not a function", call_name_debug), span)),
                };

                let mut args = Vec::new();
                for (i, node) in arguments.into_iter().enumerate() {
                    let expected = param_types.get(i).and_then(|t| {
                        // Don't use generic params as type hints — they're not concrete
                        if matches!(t, Type::GENERIC(_)) { None } else { Some(t) }
                    });
                    let mut arg = self.lower_expr_with_type(node, expected)?;
                    if let Some(target) = expected { arg = self.coerce_primitive(arg, target); }
                    args.push(arg);
                }

                let mut lowered_generics = Vec::new();
                for node in generic_args { lowered_generics.push(self.lower_type(node)?); }

                Ok(HIRExpr { 
                    kind: HIRExprKind::Call { callee: def_id, args, generic_args: lowered_generics }, 
                    ty: return_type,
                    span
                })
            },

            ASTNode::CastExpression { value, target } => {
                let expr = self.lower_expr_with_type(*value, None)?;
                let target_type = self.lower_type(*target)?;

                // Decide if this is a math cast or a pointer cast
                let cast_kind = match (&expr.ty, &target_type) {
                    (Type::REF(_), Type::POINTER(_)) |
                    (Type::CONST_REF(_), Type::POINTER(_)) |
                    (Type::POINTER(_), Type::POINTER(_)) => CastKind::Pointer,
                    _ => CastKind::Numeric,
                };

                Ok(HIRExpr { 
                    kind: HIRExprKind::Cast { expr: Box::new(expr), kind: cast_kind }, 
                    ty: target_type,
                    span
                })
            },

            ASTNode::BorrowExpression { is_mut, right } => {
                let target_expr = self.lower_expression(*right)?;

                let ty = if is_mut {
                    Type::REF(Box::new(target_expr.ty.clone()))
                } else {
                    Type::CONST_REF(Box::new(target_expr.ty.clone()))
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Borrow { 
                        is_mut, 
                        target: Box::new(target_expr) 
                    },
                    ty,
                    span, // Ensure you pass the span from your AST node
                })
            }

            ASTNode::DereferenceExpression { right } => {
                let target_expr = self.lower_expression(*right)?;

                let inner_ty = match &target_expr.ty {
                    Type::REF(inner) => *inner.clone(),
                    Type::CONST_REF(inner) => *inner.clone(),
                    Type::POINTER(inner) => *inner.clone(),
                    _ => return Err(self.error("T004", format!("cannot dereference type `{}`", target_expr.ty), span)),
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Dereference { target: Box::new(target_expr) },
                    ty: inner_ty,
                    span,
                })
            }

            ASTNode::UnaryExpression { operator, right } => {
                let rhs = self.lower_expr_with_type(*right, expected)?;
                match operator.token_type {
                    TokenType::Minus => Ok(HIRExpr {
                        kind: HIRExprKind::Unary { op: HIRUnaryOp::Neg, operand: Box::new(rhs.clone()) },
                        ty: rhs.ty, span
                    }),

                    TokenType::ExclamationMark => Ok(HIRExpr {
                        kind: HIRExprKind::Unary { op: HIRUnaryOp::Not, operand: Box::new(rhs) },
                        ty: Type::BOOL, span
                    }),

                    TokenType::Ampersand => {
                        let ty = Type::CONST_REF(Box::new(rhs.ty.clone()));
                        Ok(HIRExpr {
                            kind: HIRExprKind::Borrow { is_mut: false, target: Box::new(rhs) },
                            ty, span
                        })
                    },

                    TokenType::Star => {
                        let inner_ty = match &rhs.ty {
                            Type::REF(t) | Type::CONST_REF(t) | Type::POINTER(t) => *t.clone(),
                            _ => return Err(self.error("S001", format!("cannot dereference '{}'", rhs.ty), operator.span)),
                        };

                        Ok(HIRExpr {
                            kind: HIRExprKind::Dereference { target: Box::new(rhs) },
                            ty: inner_ty, span
                        })
                    },

                    _ => Err(self.error("S003", format!("unknown unary op: {}", operator.lexeme), operator.span))
                }
            },
            _ => Err(self.error("S006", format!("expression not supported: {:?}", node), span)),
        }
    }
}
