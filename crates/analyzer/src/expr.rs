use super::Analyzer;
use errors::HydraError;
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use ir::{expr::{Expr, ExprKind, UnaryOp, BinaryOp}, types::Type};
use crate::scope::Symbol;

impl Analyzer {

    pub(crate) fn lower_expression(&mut self, node: ASTNode) -> Result<Expr, HydraError<'static>> {
        match node {
            ASTNode::UnaryExpression { operator, right } => {
                let rhs = self.lower_expression(*right)?;

                match operator.token_type {
                    TokenType::Minus => {
                        if !rhs.ty.is_numeric() {
                            return Err(self.make_error(format!("cannot negate '{}'", rhs.ty), &operator));
                        }
                        Ok(Expr {
                            kind: ExprKind::Unary { op: UnaryOp::NEG, operand: Box::new(rhs.clone()) },
                            ty: rhs.ty
                        })
                    },

                    TokenType::ExclamationMark => {
                        if rhs.ty != Type::BOOL {
                            return Err(self.make_error(format!("cannot logic-not '{}'", rhs.ty), &operator));
                        }

                        Ok(Expr {
                            kind: ExprKind::Unary { 
                                op: UnaryOp::NOT, 
                                operand: Box::new(rhs) 
                            },
                            ty: Type::BOOL,
                        })
                    },

                    TokenType::Ampersand => {
                        Ok(Expr {
                            kind: ExprKind::Unary { 
                                op: UnaryOp::ADDR_OF, 
                                operand: Box::new(rhs.clone()) 
                            },
                            ty: Type::REF(Box::new(rhs.ty))
                        })
                    },

                    TokenType::Star => {
                        let inner_ty = match &rhs.ty {
                            Type::REF(t) | Type::CONST_REF(t) | Type::POINTER(t) => *t.clone(),
                            _ => return Err(self.make_error(format!("cannot dereference '{}'", rhs.ty), &operator)),
                        };
                        Ok(Expr {
                            kind: ExprKind::Unary { op: UnaryOp::DEREF, operand: Box::new(rhs) },
                            ty: inner_ty
                        })
                    },

                    _ => Err(self.make_error(format!("unknown unary op: {}", operator.lexeme), &operator))
                }
            },

            ASTNode::CastExpression { value, target } => {
                let expr = self.lower_expression(*value)?;
                let target_type = self.lower_type(*target)?;

                let valid = match (&expr.ty, &target_type) {
                    (t1, t2) if t1 == t2 => true,

                    // --- 2. Float <-> Int
                    (Type::F64, Type::I64) => true,
                    (Type::I64, Type::F64) => true, // math mixing (i as f64)
                    (Type::F64, Type::I32) => true,
                    (Type::I32, Type::F64) => true,
                    (Type::F32, Type::I64) => true, 
                    (Type::I64, Type::F32) => true,

                    // --- 3. Int <-> Int (Widening & Narrowing) ---
                    (Type::I32, Type::I64) => true, // Widen
                    (Type::I64, Type::I32) => true, // Narrow (Truncate)
                    (Type::I16, Type::I32) => true,
                    (Type::I8,  Type::I64) => true,

                    // --- 4. Float Precision (Promotion & Demotion) ---
                    (Type::F32, Type::F64) => true, // Promote
                    (Type::F64, Type::F32) => true, // Demote (lossy)

                    // --- 5. Unsigned/Byte Support
                    (Type::F64, Type::U8)  => true, 
                    (Type::I64, Type::U8)  => true, // i64 -> u8
                    (Type::U8,  Type::I64) => true, // u8 -> i64
                    (Type::U8,  Type::F64) => true, // u8 -> f64

                    // --- 6. Indexing (usize) ---
                    (Type::I64, Type::USIZE) => true,
                    (Type::USIZE, Type::I64) => true,
                    (Type::POINTER(inner), Type::POINTER(_)) if **inner == Type::U8 => true,
                    (Type::POINTER(_), Type::POINTER(_)) => true,
                    (Type::REF(_), Type::POINTER(_)) => true,

                    (Type::CHAR, Type::U8) => true,
                    (Type::U8, Type::CHAR) => true,

                    (Type::BOOL, Type::U8) => true,
                    (Type::BOOL, Type::I8) => true,
                    (Type::BOOL, Type::U16) => true,
                    (Type::BOOL, Type::I16) => true,
                    (Type::BOOL, Type::I32) => true,
                    (Type::BOOL, Type::U32) => true,
                    (Type::BOOL, Type::I64) => true,
                    (Type::BOOL, Type::U64) => true,

                    _ => false,
                };

                if !valid {
                    return Err(self.make_error(
                        format!("cannot cast type {} to {}", expr.ty, target_type),
                        &self.dummy_token(),
                    ));
                }

                Ok(Expr {
                    kind: ExprKind::Cast { expr: Box::new(expr) },
                    ty: target_type
                })
            }

            ASTNode::Expression { token } => {
                match token.token_type {
                    TokenType::IntLiteral(val) => {
                        let ty = if val >= (i32::MIN as i64) && val <= (i32::MAX as i64) {
                            Type::I32
                        } else {
                            Type::I64
                        };

                        Ok(Expr { kind: ExprKind::INT_LITERAL(val), ty })
                    },

                    TokenType::FloatLiteral(val) => {
                        Ok(Expr {
                            ty: Type::F64, 
                            kind: ExprKind::FLOAT_LITERAL(val) 
                        })
                    },

                    TokenType::StringLiteral(ref s) => Ok(Expr {
                        kind: ExprKind::STRING_LITERAL(s.clone()),
                        ty: Type::ARRAY(Box::new(Type::U8), s.len())
                    }),

                    TokenType::CharLiteral(c) => Ok(Expr { kind: ExprKind::CHAR_LITERAL(c), ty: Type::CHAR }),
                    TokenType::BoolLiteral(b) => Ok(Expr {
                        kind: ExprKind::INT_LITERAL(if b { 1 } else { 0 }),
                        ty: Type::BOOL,
                    }),

                    _ => Err(self.make_error(format!("unexpected literal: {:?}", token), &token))
                }
            },

            ASTNode::ArrayInitializer { elements, .. } => {
                let mut ir_elements = Vec::new();

                for elem in &elements {
                    ir_elements.push(self.lower_expression(elem.clone())?);
                }

                let element_type = ir_elements.first().map(|f| f.ty.clone()).unwrap_or(Type::VOID);

                Ok(Expr {
                    kind: ExprKind::ArrayInit { elements: ir_elements },
                    ty: Type::ARRAY(Box::new(element_type), elements.len())
                })
            },

            ASTNode::ArrayAccess { array, index, token } => {
                let arr = self.lower_expression(*array)?;
                let idx_expr = self.lower_expression(*index)?;

                if !idx_expr.ty.is_numeric() {
                    return Err(self.make_error(format!("index must be numeric, found {}", idx_expr.ty), &token));
                }

                match &arr.ty.clone() {
                    Type::ARRAY(inner, size) => {
                        if let ExprKind::INT_LITERAL(idx_val) = idx_expr.kind {
                            if idx_val < 0 || idx_val >= (*size as i64) {
                                return Err(self.make_error(
                                    format!("index out of bounds: len is {} but index is {}", size, idx_val),
                                    &token
                                ));
                            }
                        }

                        Ok(Expr {
                            kind: ExprKind::ArrayAccess { 
                                array: Box::new(arr), 
                                index: Box::new(idx_expr) 
                            },
                            ty: *inner.clone()
                        })
                    },

                    Type::INFERRED_ARRAY(inner) => {
                        Ok(Expr {
                            kind: ExprKind::ArrayAccess { 
                                array: Box::new(arr), 
                                index: Box::new(idx_expr) 
                            },
                            ty: *inner.clone()
                        })
                    },

                    _ => Err(self.make_error(format!("type '{}' cannot be indexed", arr.ty), &token))
                }
            }

            ASTNode::VariableExpression { name, .. } => {
                let symbol = self.scope.resolve(name.lexeme)
                    .ok_or(self.make_error(format!("undefined variable: {}", name.lexeme), &name))?;

                match symbol {
                    Symbol::Variable { ty, .. } => Ok(Expr {
                        kind: ExprKind::VariableReference { 
                            name: name.lexeme.to_string() 
                        },
                        ty: ty.clone(),
                    }),

                    _ => Err(self.make_error(format!("'{}' is not a variable", name.lexeme), &name))
                }
            },

            ASTNode::BinaryExpression { left, operator, right } => {
                let mut lhs = self.lower_expression(*left)?;
                let mut rhs = self.lower_expression(*right)?;

                if let ExprKind::INT_LITERAL(val) = lhs.kind {
                    if self.check_and_promote_int_literal(val, &rhs.ty) {
                        lhs.ty = rhs.ty.clone();
                    }
                }

                if let ExprKind::INT_LITERAL(val) = rhs.kind {
                    if self.check_and_promote_int_literal(val, &lhs.ty) {
                        rhs.ty = lhs.ty.clone();
                    }
                }

                if lhs.ty != rhs.ty {
                    return Err(self.make_error(
                        format!("type mismatch: {} and {} differ", lhs.ty, rhs.ty),
                        &operator
                    ));
                }

                let (op, ty) = match operator.token_type {
                    TokenType::Plus => (BinaryOp::ADD, lhs.ty.clone()),
                    TokenType::Minus => (BinaryOp::SUB, lhs.ty.clone()),
                    TokenType::Star => (BinaryOp::MUL, lhs.ty.clone()),
                    TokenType::ForwardSlash => (BinaryOp::DIV, lhs.ty.clone()),
                    TokenType::Modulo => (BinaryOp::MOD, lhs.ty.clone()),

                    TokenType::LeftAngle => (BinaryOp::LT, Type::BOOL),
                    TokenType::LessEqual => (BinaryOp::LE, Type::BOOL),
                    TokenType::RightAngle => (BinaryOp::GT, Type::BOOL),
                    TokenType::GreaterEqual => (BinaryOp::GE, Type::BOOL),
                    TokenType::DoubleEqual => (BinaryOp::EQ, Type::BOOL),
                    TokenType::ExclamEqual => (BinaryOp::NE, Type::BOOL),

                    TokenType::DoubleAmpersand => (BinaryOp::AND, Type::BOOL),
                    TokenType::DoublePipe => (BinaryOp::OR,  Type::BOOL),

                    _ => return Err(self.make_error(
                        format!("unknown binary operator: {}", operator.lexeme),
                        &operator
                    ))
                };

                Ok(Expr {
                    kind: ExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    ty,
                })
            },

            ASTNode::MethodCallExpression { object, method, arguments, generic_args } => {
                if let ASTNode::VariableExpression { name } = &*object {
                    if let Some(Symbol::Struct { .. }) = self.scope.resolve(&name.lexeme) {

                        let static_method_name = format!("{}::{}", name.lexeme, method.lexeme);

                        let mut lowered_generic_args = Vec::new();
                        for arg in generic_args {
                            lowered_generic_args.push(self.lower_type(arg)?);
                        }

                        return self.lower_call_logic(
                            static_method_name, 
                            method, 
                            arguments, 
                            lowered_generic_args
                        );
                    }
                }

                let lhs = self.lower_expression(*object)?;
                let method_name = method.lexeme;

                let actual_type = match &lhs.ty {
                    Type::REF(inner) | Type::CONST_REF(inner) => inner.as_ref(),
                    _ => &lhs.ty
                };

                let target_type_name = match actual_type {
                    Type::STRUCT(name) => name.clone(),
                    Type::I8 => "i8".to_string(),
                    Type::I16 => "i16".to_string(),
                    Type::I32 => "i32".to_string(),
                    Type::I64 => "i64".to_string(),
                    Type::ISIZE => "isize".to_string(),
                    Type::U8 => "u8".to_string(),
                    Type::U16 => "u16".to_string(),
                    Type::U32 => "u32".to_string(),
                    Type::U64 => "u64".to_string(),
                    Type::USIZE => "usize".to_string(),
                    Type::F32 => "f32".to_string(),
                    Type::F64 => "f64".to_string(),
                    Type::BOOL => "bool".to_string(),
                    Type::CHAR => "char".to_string(),
                    _ => return Err(self.make_error(format!("type '{}' has no methods", lhs.ty), &method)),
                };

                let namespaced_name = format!("{}::{}", target_type_name, method_name);

                let symbol = self.scope.resolve(&namespaced_name)
                    .ok_or(self.make_error(format!("method '{}' not found on type '{}'", method_name, target_type_name), &method))?;

                let (param_types, return_type) = match symbol {
                    Symbol::Function { params, return_type, .. } => (params.clone(), return_type.clone()),
                    _ => return Err(self.make_error(format!("'{}' is not a function", namespaced_name), &method)),
                };

                let mut args = Vec::new();

                let expected_self_ty = param_types.first().ok_or(
                    self.make_error(format!("method '{}' expects at least 1 argument (self)", method_name), &method)
                )?;

                let self_arg = match (expected_self_ty, &lhs.ty) {
                    (Type::REF(inner), actual) | (Type::CONST_REF(inner), actual) if inner.as_ref() == actual => {
                        let ty = lhs.ty.clone();

                        Expr {
                            kind: ExprKind::Unary { op: UnaryOp::ADDR_OF, operand: Box::new(lhs) },
                            ty: Type::REF(Box::new(ty))
                        }
                    },

                    _ => lhs
                };
                args.push(self_arg);

                for arg in arguments {
                    args.push(self.lower_expression(arg)?);
                }                

                // 4. Validate Arguments (Count and Types)
                if args.len() != param_types.len() {
                    return Err(self.make_error(
                        format!("expected {} args, got {}", param_types.len(), args.len()),
                        &method
                    ));
                }

                for (i, arg) in args.iter_mut().enumerate() {
                    let expected = &param_types[i];

                    if let Type::REF(inner) | Type::CONST_REF(inner) = &arg.ty {
                        if inner.as_ref() == expected {
                            *arg = Expr {
                                kind: ExprKind::Unary { op: UnaryOp::DEREF, operand: Box::new(arg.clone()) },
                                ty: *inner.clone()
                            };
                        }
                    }

                    // Integer promotion
                    if let ExprKind::INT_LITERAL(l) = arg.kind {
                        if self.check_and_promote_int_literal(l, expected) {
                            arg.ty = expected.clone();
                        }
                    }

                    if !self.check_type_compatibility(expected, &arg.ty) {
                        return Err(self.make_error(
                            format!("argument {} type mismatch: expected {}, found {}", i, expected, arg.ty),
                            &method
                        ));
                    }
                }

                let mut lowered_generics = Vec::new();
                for node in generic_args {
                    lowered_generics.push(self.lower_type(node)?);
                }

                Ok(Expr {
                    kind: ExprKind::Call { callee: namespaced_name, args, generic_args: lowered_generics },
                    ty: return_type
                })
            },

            ASTNode::FunctionCallExpression { name, mut arguments, generic_args } => {
                let mut call_name = name.lexeme.to_string();

                if call_name.contains("::") {
                    let parts: Vec<&str> = name.lexeme.split("::").collect();
                    let prefix = parts[0];     
                    let method_name = parts[1];

                    let prefix_symbol = self.scope.resolve(prefix);

                    if let Some(Symbol::Variable { ty: prefix_ty, .. }) = prefix_symbol {
                        let struct_name = match prefix_ty {
                            Type::STRUCT(n) => Some(n.clone()),
                            Type::REF(inner) | Type::CONST_REF(inner) => {
                                if let Type::STRUCT(n) = inner.as_ref() {
                                    Some(n.clone())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };

                        if let Some(s_name) = struct_name {
                            let namespaced_name = format!("{}::{}", s_name, method_name);

                            if let Some(Symbol::Function { params, .. }) = self.scope.resolve(&namespaced_name) {
                                if let Some(first_param_ty) = params.first() {
                                    if matches!(first_param_ty, Type::REF(_) | Type::CONST_REF(_) | Type::STRUCT(_)) {

                                        let self_token = Token {
                                            lexeme: prefix,
                                            ..name.clone()
                                        };
                                        let self_node = ASTNode::VariableExpression { name: self_token };

                                        // dont wrap in another '&' if already a reference (self or
                                        // &var)
                                        if matches!(prefix_ty, Type::REF(_) | Type::CONST_REF(_)) {
                                            arguments.insert(0, self_node);
                                        } else {
                                            if matches!(first_param_ty, Type::REF(_) | Type::CONST_REF(_)) {
                                                arguments.insert(0, ASTNode::UnaryExpression {
                                                    operator: Token {
                                                        token_type: TokenType::Ampersand,
                                                        ..name.clone()
                                                    },
                                                    right: Box::new(self_node),
                                                });
                                            } else {
                                                arguments.insert(0, self_node);
                                            }
                                        }
                                    } 
                                }

                                call_name = namespaced_name;
                            }
                        }
                    }
                }

                let mut lowered_generics = Vec::new();
                for node in generic_args {
                    lowered_generics.push(self.lower_type(node)?);
                }


                self.lower_call_logic(call_name, name, arguments, lowered_generics)
            }

            ASTNode::StructInitializer { name, fields } => {
                let struct_name = &name.lexeme;

                let def_fields = match self.scope.resolve(struct_name) {
                    Some(Symbol::Struct { fields }) => fields.clone(),

                    _ => return Err(self.make_error(
                        format!("'{}' is not a defined struct", struct_name),
                        &name
                    )),
                };

                let mut lowered_values = Vec::new();

                for (def_name, def_type, is_const) in &def_fields { 
                    if *is_const { continue; }

                    let matching_field = fields.iter().find(|(f_token, _)| f_token.lexeme == *def_name);

                    match matching_field {
                        Some((_, value_node)) => {
                            let mut val = self.lower_expression(*value_node.clone())?;

                            if let ExprKind::INT_LITERAL(i) = val.kind {
                                if self.check_and_promote_int_literal(i, def_type) {
                                    val.ty = def_type.clone();
                                }
                            }

                            if !self.check_type_compatibility(def_type, &val.ty) {
                                return Err(self.make_error(
                                    format!("field '{}' expected type {}, found {}", def_name, def_type, val.ty),
                                    &name
                                ));
                            }

                            lowered_values.push(val);
                        }

                        None => return Err(self.make_error(
                            format!("missing field '{}' in initializer '{}'", def_name, struct_name),
                            &name
                        )),
                    }
                }

                if fields.len() > def_fields.len() {
                    return Err(self.make_error(
                        format!("too many fields provided for struct '{}'", struct_name),
                        &name
                    ));
                }

                Ok(Expr {
                    kind: ExprKind::StructInit {
                        name: struct_name.to_string(),
                        values: lowered_values,
                    },
                    ty: Type::STRUCT(struct_name.to_string()),
                })
            }

            ASTNode::MemberExpression { object, property } => {
                if let ASTNode::TypeIdentifier { type_token } = &*object {
                    let target = type_token.lexeme;
                    let const_name = format!("{}::{}", target, property.lexeme);

                    if let Some(Symbol::Variable { ty, .. }) = self.scope.resolve(&const_name) {
                        return Ok(Expr {
                            kind: ExprKind::VariableReference { 
                                name: const_name 
                            },
                            ty: ty.clone()
                        });
                    } else {
                        return Err(self.make_error(
                            format!("no constant '{}' found on type '{}'", property.lexeme, target),
                            &property
                        ));
                    }
                }

                if let ASTNode::VariableExpression { ref name } = *object {
                    if let Some(Symbol::Struct { fields }) = self.scope.resolve(name.lexeme) {
                        if let Some((_, field_type, is_const)) = fields.iter().find(|(n, _, _)| n == &property.lexeme) {
                            if *is_const {
                                return Ok(Expr {
                                    kind: ExprKind::VariableReference { 
                                        name: format!("{}.{}", name.lexeme, property.lexeme) 
                                    },
                                    ty: field_type.clone()
                                });
                            }
                        }
                    }
                }

                let lhs = self.lower_expression(*object)?;

                let actual_type = match &lhs.ty {
                    Type::REF(inner) | Type::CONST_REF(inner) => inner.as_ref(),
                    _ => &lhs.ty
                };

                match &actual_type {
                    Type::STRUCT(name) => {
                        if let Some(Symbol::Struct { fields }) = self.scope.resolve(name) {
                            if let Some(idx) = fields.iter()
                                .position(|(field_name, _, _)| field_name == &property.lexeme) 
                            {
                                let (_, field_type, _) = &fields[idx]; 

                                return Ok(Expr {
                                    kind: ExprKind::MemberAccess {
                                        object: Box::new(lhs),
                                        member: property.lexeme.to_string(),
                                        index: idx as u32,
                                    },
                                    ty: field_type.clone()
                                });
                            }
                        }

                        Err(self.make_error(
                            format!("struct '{}' has no field '{}'", name, property.lexeme),
                            &property
                        ))
                    },

                    Type::ARRAY(_, size) if property.lexeme == "len" => {
                        Ok(Expr { 
                            kind: ExprKind::INT_LITERAL(*size as i64), 
                            ty: Type::I32 
                        })
                    },

                    Type::INFERRED_ARRAY(_) => {
                        match &lhs.kind {
                            ExprKind::STRING_LITERAL(s) => Ok(Expr {
                                kind: ExprKind::INT_LITERAL(s.len() as i64), 
                                ty: Type::I32 
                            }),

                            ExprKind::ArrayInit { elements } => Ok(Expr { 
                                kind: ExprKind::INT_LITERAL(elements.len() as i64), 
                                ty: Type::I32 
                            }),

                            _ => Err(self.make_error("len cannot be determined at compile time".to_string(), &property))
                        }
                    },

                    _ => Err(self.make_error(
                        format!("'{}' has no property 'len'", lhs.ty), 
                        &property
                    ))
                }
            },

            _ => Err(self.make_generic_error(format!("expression not supported: {:?}", node))),
        }
    }


    pub(crate) fn lower_call_logic(&mut self, call_name: String, token: Token, 
        arguments: Vec<ASTNode>, generic_args: Vec<Type>) 
        -> Result<Expr, HydraError<'static>> 
    {
        if call_name == "println" || call_name == "print" {
            let mut args = Vec::new();

            for arg in arguments {
                args.push(self.lower_expression(arg)?);
            }
            
            return Ok(Expr {
                kind: ExprKind::Call { 
                    callee: call_name, 
                    args,
                    generic_args
                },
                ty: Type::VOID // println always returns void
            });
        }

        let symbol = self.scope.resolve(&call_name)
            .ok_or(self.make_error(format!("undefined function: {}", call_name), &token))?;

        let (param_types, return_type, annotations) = match symbol {
            Symbol::Function { params, return_type, annotations, .. } => 
                (params.clone(), return_type.clone(), annotations.clone()),

            _ => return Err(self.make_error(format!("'{}' is not a function", call_name), &token)),
        };

        if let Some(builtin) = annotations.iter().find(|a| a.name == "builtin") {
            if let Some(builtin_type) = builtin.args.first() {
                match builtin_type.as_str() {
                    "size_of" => {
                        if generic_args.len() != 1 {
                            return Err(self.make_error("size_of expects one generic type argument".into(), &token));
                        }

                        let size = self.get_type_size(&generic_args[0])?;

                        return Ok(Expr {
                            kind: ExprKind::INT_LITERAL(size),
                            ty: Type::USIZE,
                        });
                    },

                    _ => return Err(self.make_error(format!("unknown builtin '{}'", builtin_type), &token))
                }
            }
        }

        if arguments.len() != param_types.len() {
            return Err(self.make_error(
                format!("expected {} args, got {}", param_types.len(), arguments.len()),
                &token
            ));
        }

        let mut args = Vec::new();
        for (i, node) in arguments.into_iter().enumerate() {
            let arg_token = self.get_token_from_node(&node);

            let mut arg = self.lower_expression(node)?;
            let expected = &param_types[i];

            if let Type::REF(inner) | Type::CONST_REF(inner) = &arg.ty.clone() {
                if inner.as_ref() == expected {
                        arg = Expr {
                        kind: ExprKind::Unary { op: UnaryOp::DEREF, operand: Box::new(arg) },
                        ty: *inner.clone()
                        };
                }
            }

            if let ExprKind::INT_LITERAL(l) = arg.kind {
                if self.check_and_promote_int_literal(l, expected) {
                    arg.ty = expected.clone();
                }
            }

            if !self.check_type_compatibility(expected, &arg.ty) {
                return Err(self.make_error(
                    format!("type mismatch: expected {}, found {}", expected, arg.ty),
                    &arg_token
                ));
            }

            args.push(arg);
        }

        Ok(Expr {
            kind: ExprKind::Call { callee: call_name, args, generic_args},
            ty: return_type
        })
    }
}
