pub mod scope;

use std::mem;
use errors::{HydraError, generic::GenericError};
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use ir::{Function, Program, expr::{BinaryOp, Expr, ExprKind, UnaryOp}, stmt::{AssignmentTarget, Block, LoopKind, Stmt}, types::Type};
use scope::Scope;

use crate::scope::Symbol;

pub struct Analyzer {
    scope: Scope,
    current_return_type: Option<Type>,
}

impl Analyzer {

    pub fn new() -> Self {
        Self {
            scope: Scope::new(),
            current_return_type: None
        }
    }

    pub fn analyze(&mut self, nodes: Vec<ASTNode>) -> Result<Program, Vec<HydraError<'static>>> {
        let mut functions: Vec<Function> = Vec::new();
        let mut errors = Vec::new();
        let mut structs = Vec::new();
        let mut globals = Vec::new();

        for node in &nodes {
            match node {
                ASTNode::StructDeclaration { name, .. } => {
                    let struct_name = name.lexeme.to_string();
                    self.scope.define(struct_name, Symbol::Struct { fields: Vec::new() }).ok();
                }

                ASTNode::FunctionDeclaration { name, parameters, return_type, generic_params, .. } => {
                    let rt = self.lower_type(*return_type.clone()).unwrap_or(Type::VOID);
                    let mut param_types = Vec::new();

                    for (_, type_node) in parameters {
                        param_types.push(self.lower_type(*type_node.clone()).unwrap_or(Type::VOID));
                    }

                    let mut gps = Vec::new();
                    for gp in generic_params {
                        gps.push(gp.lexeme.to_string());
                    }

                    self.scope.define(name.lexeme.to_string(), Symbol::Function { 
                        params: param_types, 
                        annotations: Vec::new(),
                        return_type: rt,
                        generic_params: gps
                    }).ok();
                }

                _ => {}
            }
        }

        for node in &nodes {
            if let ASTNode::StructDeclaration { name, fields, methods, constants, generic_params } = node {
                let struct_name = name.lexeme.to_string();

                self.enter_scope();
                for gp in generic_params {
                    self.scope.define(gp.lexeme.to_string(), Symbol::Struct { fields: Vec::new() }).ok();
                }

                let mut struct_fields = Vec::new();

                for (field_name, field_type) in fields {
                    match self.lower_type(*field_type.clone()) {
                        Ok(t) => struct_fields.push((field_name.lexeme.to_string(), t, false)),
                        Err(e) => errors.push(e),
                    }
                }
                self.leave_scope();
            
                self.scope.define_or_update(struct_name.clone(), Symbol::Struct {
                    fields: struct_fields.clone()
                });

                for constant in constants {
                    if let ASTNode::VariableDeclaration { name: c_name, type_annotation, .. } = constant {
                        let ty = type_annotation.as_ref()
                            .and_then(|ann| self.lower_type(*ann.clone()).ok())
                            .unwrap_or(Type::VOID);

                        struct_fields.push((c_name.lexeme.to_string(), ty.clone(), true)); 

                        let const_name = format!("{}.{}", struct_name, c_name.lexeme);
                        self.scope.define(const_name, Symbol::Variable {
                            ty,
                            is_mutable: false,
                        }).ok();
                    }
                }

                self.scope.define_or_update(
                    struct_name.clone(), 
                    Symbol::Struct {
                        fields: struct_fields.clone() 
                    }
                );

                let ir_fields: Vec<(String, Type)> = struct_fields.iter()
                    .filter(|(_, _, is_const)| !*is_const)
                    .map(|(n, t, _)| (n.clone(), t.clone()))
                    .collect();

                structs.push((struct_name.clone(), ir_fields));

                for method in methods {
                    if let ASTNode::FunctionDeclaration { 
                        name: m_name, 
                        parameters, 
                        return_type, 
                        generic_params: method_generics, .. } = method 
                    {
                        let namespaced_name = format!("{}::{}", struct_name, m_name.lexeme);

                        if self.scope.resolve(&namespaced_name).is_some() { continue; }

                        let mut all_generics = Vec::new();

                        self.enter_scope();
                        for gp in generic_params {
                            all_generics.push(gp.lexeme.to_string())
                        }

                        for gp in method_generics {
                            all_generics.push(gp.lexeme.to_string())
                        }

                        let mut param_types = Vec::new();
                        for (_, type_node) in parameters {
                            param_types.push(self.lower_type(*type_node.clone()).unwrap_or(Type::VOID));
                        }

                        let rt = self.lower_type(*return_type.clone()).unwrap_or(Type::VOID);

                        self.leave_scope();

                        self.scope.define(namespaced_name, Symbol::Function {
                            params: param_types,
                            annotations: Vec::new(),
                            return_type: rt,
                            generic_params: all_generics
                        }).ok();
                    }
                }
            }
        }

        for node in &nodes {
            if let ASTNode::FunctionDeclaration { name, parameters, return_type, generic_params, .. } = node {
                if self.scope.resolve(name.lexeme).is_some() { continue; }

                let mut param_types = Vec::new();
                for (_, type_node) in parameters {
                    match self.lower_type(*type_node.clone()) {
                        Ok(t) => param_types.push(t),
                        Err(_) => {}, // caught in pass 2
                    }
                }

                let rt = match self.lower_type(*return_type.clone()) {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(e);
                        Type::VOID
                    }
                };

                let mut gps = Vec::new();
                for gp in generic_params {
                    gps.push(gp.lexeme.to_string());
                }

                let symbol = Symbol::Function { 
                    params: param_types,
                    annotations: Vec::new(),
                    return_type: rt,
                    generic_params: gps
                };

                if let Err(msg) = self.scope.define(name.lexeme.to_string(), symbol) {
                    errors.push(self.make_error(msg, name));
                }
            }
        }

        for node in nodes {
            match node {
                ASTNode::FunctionDeclaration { ref name, .. } => {
                    if functions.iter().any(|f| f.name == name.lexeme) { continue; }

                    match self.lower_function(node) {
                        Ok(function) => functions.push(function),
                        Err(e) => errors.push(e),
                    }
                },

                ASTNode::StructDeclaration { name, methods, constants, generic_params, .. } => {
                    let struct_name = name.lexeme;

                    for constant in constants {
                        if let ASTNode::VariableDeclaration { name: c_name, initializer, .. } = constant {
                            let full_name = format!("{}.{}", struct_name, c_name.lexeme);
                            if globals.iter().any(|(n, _, _)| n == &full_name) { continue; }

                            match self.lower_expression(*initializer.clone()) {
                                Ok(init_expr) => globals.push((full_name, init_expr.ty.clone(), init_expr)),
                                Err(e) => errors.push(e),
                            }
                        }
                    }
                            
                    for method in methods {
                        let m_name = if let ASTNode::FunctionDeclaration { 
                            name: ref n, .. 
                        } = method { n.lexeme } else { "" };

                        let full_name = format!("{}::{}", struct_name, m_name);

                        if functions.iter().any(|f| f.name == full_name) {
                            continue; 
                        }

                        self.enter_scope();
                        for gp in &generic_params {
                            self.scope.define(gp.lexeme.to_string(), Symbol::Struct {
                                fields: Vec::new() 
                            }).ok();
                        }
                        
                        let lowered_res = self.lower_function(method);

                        self.leave_scope();

                        match lowered_res {
                            Ok(mut ir) => {
                                ir.name = full_name;
                                functions.push(ir);
                            }

                            Err(e) => errors.push(e)
                        }
                    }
                },

                ASTNode::VariableDeclaration { is_const: true, .. } => {},

                _ => errors.push(self.make_generic_error(
                    "executable code is not allowed at the top level".to_string()
                ))
            }
        }

        let has_main = functions.iter().any(|f| f.name == "main");
        if !has_main && errors.is_empty() {
            errors.push(HydraError::GENERIC(Box::new(GenericError {
                code: "E001",
                message: "program is missing a 'main' function entry point".to_string(),
                token: self.dummy_token(),
                help: None
            })))
        }

        if !errors.is_empty() {
            Err(errors)
        } else {
            Ok(Program { 
                functions,
                structs,
                globals,
            })
        }
    }

    fn lower_statement(&mut self, node: ASTNode) -> Result<Stmt, HydraError<'static>> {
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

    fn lower_expression(&mut self, node: ASTNode) -> Result<Expr, HydraError<'static>> {
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
                            Type::REF(t) | Type::CONST_REF(t) => *t.clone(),
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
                
                // 1. Resolve Struct Type from Object
                let struct_name = match &lhs.ty {
                    Type::STRUCT(name) => name.clone(),
                    Type::REF(inner) | Type::CONST_REF(inner) => {
                         if let Type::STRUCT(name) = inner.as_ref() {
                             name.clone()
                         } else {
                             return Err(self.make_error(format!("type '{}' has no methods", lhs.ty), &method));
                         }
                    },

                    _ => {
                        return Err(self.make_error(format!("type '{}' has no methods", lhs.ty), &method));
                    }
                };

                let namespaced_name = format!("{}::{}", struct_name, method_name);
                
                // 2. Resolve Function Symbol
                let symbol = self.scope.resolve(&namespaced_name)
                    .ok_or(self.make_error(format!("method '{}' not found on type '{}'", method_name, struct_name), &method))?;

                let (param_types, return_type) = match symbol {
                    Symbol::Function { params, return_type, .. } => (params.clone(), return_type.clone()),
                    _ => return Err(self.make_error(format!("'{}' is not a function", namespaced_name), &method)),
                };
                
                // 3. Prepare Arguments & Handle Auto-Ref for 'self'
                let mut args = Vec::new();
                
                let expected_self_ty = param_types.first().ok_or(
                    self.make_error(format!("method '{}' expects at least 1 argument (self)", method_name), &method)
                )?;

                let self_arg = match (expected_self_ty, &lhs.ty) {
                    // If method expects reference but object is a value -> Auto-Ref (Address Of)
                    (Type::REF(_), Type::STRUCT(_)) | (Type::CONST_REF(_), Type::STRUCT(_)) => {
                        let ty = lhs.ty.clone();

                        Expr {
                            kind: ExprKind::Unary { op: UnaryOp::ADDR_OF, operand: Box::new(lhs) },
                            ty: Type::REF(Box::new(ty))
                        }
                    },

                    // Otherwise pass as is (matches or strict mismatch caught in loop below)
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

    fn lower_function(&mut self, node: ASTNode) -> Result<Function, HydraError<'static>> {
        if let ASTNode::FunctionDeclaration { 
            name,
            annotations,
            generic_params, 
            parameters, 
            return_type: rt, 
            body, 
            is_extern 
        } = node 
        {
            let is_intrinsic = annotations.iter().any(|a| a.name == "intrinsic");

            self.enter_scope();

            for gp in &generic_params {
                self.scope.define(gp.lexeme.to_string(), Symbol::Struct { fields: Vec::new() }).ok();
            }

            let return_type = self.lower_type(*rt)?;
            let prev_return_type = self.current_return_type.replace(return_type.clone());

            let mut ir_params = Vec::new();
            for (param_name, param_type_node) in &parameters {
                let ty = self.lower_type(*param_type_node.clone())?;

                ir_params.push((param_name.lexeme.to_string(), ty.clone()));

                self.scope.define(param_name.lexeme.to_string(), Symbol::Variable { ty, is_mutable: true })
                    .map_err(|msg| self.make_generic_error(msg))?;
            }

            let mut stmts = Vec::new();
            for stmt_node in body { 
                stmts.push(self.lower_statement(stmt_node)?); 
            }

            self.leave_scope();
            self.current_return_type = prev_return_type;

            Ok(Function { 
                name: name.lexeme.to_string(), 
                params: ir_params, 
                return_type, 
                body: Block { stmts },
                is_extern,
                is_intrinsic,
                generic_params: generic_params.iter().map(|t| t.lexeme.to_string()).collect(),
            })
        } else {
            Err(self.make_generic_error("expected function".to_string()))
        }
    }

    fn lower_for_loop(&mut self, variable: Token, start: ASTNode, 
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

    fn lower_foreach_loop(&mut self, item: Token, iterable: ASTNode, body: Vec<ASTNode>) -> Result<Stmt, HydraError<'static>> {
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

    fn lower_type(&mut self, node: ASTNode) -> Result<Type, HydraError<'static>> {
        match node {
            ASTNode::Reference { inner } => {
                let inner_type = self.lower_type(*inner)?;
                Ok(Type::REF(Box::new(inner_type)))
            }

            ASTNode::ConstReference { inner } => {
                let inner_type = self.lower_type(*inner)?;
                Ok(Type::CONST_REF(Box::new(inner_type)))
            }

            ASTNode::Pointer { inner } => {
                let inner_type = self.lower_type(*inner)?;
                Ok(Type::POINTER(Box::new(inner_type)))
            }

            ASTNode::GenericType { base, args } => {
                // just resolve for now
                // monorphized based on args
                // once proof of concept
                self.lower_type(*base)
            }

            ASTNode::TypeIdentifier { type_token } => {
                match type_token.lexeme {
                    "i8" => Ok(Type::I8), 
                    "i16" => Ok(Type::I16), 
                    "i32" => Ok(Type::I32), 
                    "i64" => Ok(Type::I64),
                    "isize" => Ok(Type::ISIZE), 
                    "u8" => Ok(Type::U8), 
                    "u16" => Ok(Type::U16), 
                    "u32" => Ok(Type::U32),
                    "u64" => Ok(Type::U64), 
                    "usize" => Ok(Type::USIZE), 
                    "f32" => Ok(Type::F32), 
                    "f64" => Ok(Type::F64),
                    "char" => Ok(Type::CHAR), 
                    "bool" => Ok(Type::BOOL), 
                    "void" => Ok(Type::VOID),

                    name => {
                        if let Some(Symbol::Struct { .. }) = self.scope.resolve(name) {
                            Ok(Type::STRUCT(name.to_string()))
                        } else {
                            Err(self.make_error(
                                format!("unknown type: {}", name),
                                &type_token,
                            ))
                        }
                    }
                }
            },

            ASTNode::ArrayType { element_type, size, .. } => {
                let inner = self.lower_type(*element_type)?;

                let size_token = self.get_token_from_node(&size);
                match size_token.token_type {
                    TokenType::IntLiteral(n) => Ok(Type::ARRAY(Box::new(inner), n as usize)),

                    TokenType::ANYSIZE => Ok(Type::INFERRED_ARRAY(Box::new(inner))),

                    _ => Err(self.make_error("array size must be int or 'anysize'".to_string(), &size_token))
                }
            },

            _ => Err(self.make_generic_error(format!("invalid type: {:?}", node))),
        }
    }

    fn enter_scope(&mut self) {
        let parent = mem::replace(&mut self.scope, Scope::new());
        self.scope = Scope::new_child(parent);
    }

    fn leave_scope(&mut self) {
        let current_scope = mem::replace(&mut self.scope, Scope::new());
        let parent = current_scope.parent().expect("popped global scope");

        self.scope = parent;
    }

    fn dummy_token(&self) -> Token<'static> {
        Token { 
            token_type: TokenType::EOF, 
            lexeme: "", 
            line: 0, 
            column: 0 
        }
    }

    fn make_error(&self, msg: String, token: &Token) -> HydraError<'static> {
        HydraError::GENERIC(Box::new(GenericError {
            code: "E000", 
            message: msg, 
            help: None,
            token: Token { 
                token_type: token.token_type.clone(), 
                lexeme: "", 
                line: token.line, 
                column: token.column 
            }
        }))
    }

    fn make_generic_error(&self, msg: String) -> HydraError<'static> {
        HydraError::GENERIC(Box::new(GenericError { 
            code: "E000", 
            message: msg, 
            token: self.dummy_token(), 
            help: None 
        }))
    }

    fn check_and_promote_int_literal(&self, lit_val: i64, target_ty: &Type) -> bool {
        match target_ty {
            Type::I8  => lit_val >= (i8::MIN as i64) && lit_val <= (i8::MAX as i64),
            Type::U8  => lit_val >= 0 && lit_val <= (u8::MAX as i64),
            Type::I16 => lit_val >= (i16::MIN as i64) && lit_val <= (i16::MAX as i64),
            Type::U16 => lit_val >= 0 && lit_val <= (u16::MAX as i64),
            Type::I32 => true,
            Type::U32 => lit_val >= 0,
            Type::I64 | Type::ISIZE => true,
            Type::U64 | Type::USIZE => lit_val >= 0,
            Type::F32 | Type::F64 => true,
            Type::BOOL => lit_val == 0 || lit_val == 1,

            _ => false, 
        }
    }

    fn check_type_compatibility(&self, target: &Type, source: &Type) -> bool {
        if target == source { 
            return true; 
        }

        match (target, source) {
            (Type::INFERRED_ARRAY(target_inner), Type::ARRAY(source_inner, _)) => target_inner == source_inner,
            (Type::REF(t_inner), Type::REF(s_inner)) if t_inner == s_inner => true,
            (Type::CONST_REF(t_inner), Type::REF(s_inner)) if t_inner == s_inner => true,

            _ => false,
        }
    }

    fn get_token_from_node<'a>(&self, node: &ASTNode<'a>) -> Token<'a> {
        match node {
            ASTNode::VariableExpression { name } => name.clone(),
            ASTNode::Expression { token } | ASTNode::Primtive { token } => token.clone(),
            ASTNode::BinaryExpression { operator, .. } => operator.clone(),
            ASTNode::FunctionCallExpression { name, .. } => name.clone(),
            ASTNode::VariableDeclaration { name, .. } => name.clone(),
            ASTNode::AssignmentExpression { operator, .. } => operator.clone(),
            ASTNode::MemberExpression { property, .. } => property.clone(),
            ASTNode::UnaryExpression { operator, .. } => operator.clone(),
            ASTNode::PostfixUnaryExpression { operator, .. } => operator.clone(),
            ASTNode::TypeIdentifier { type_token } => type_token.clone(),
            ASTNode::ReturnStatement { value } => self.get_token_from_node(value),

            _ => self.dummy_token(),
        }
    }

    fn get_binary_op_from_token(&self, token: &TokenType) -> Option<BinaryOp> {
        match token {
            TokenType::PlusEqual => Some(BinaryOp::ADD),
            TokenType::MinusEqual => Some(BinaryOp::SUB),
            TokenType::StarEqual => Some(BinaryOp::MUL),
            TokenType::ForwardSlashEqual => Some(BinaryOp::DIV),
            TokenType::ModuloEqual => Some(BinaryOp::MOD),

            _ => None
        }
    }

    fn get_type_size(&self, ty: &Type) -> Result<i64, HydraError<'static>> {
        match ty {
            Type::I8 | Type::U8 | Type::BOOL | Type::CHAR => Ok(1),
            Type::I16 | Type::U16 => Ok(2),
            Type::I32 | Type::U32 | Type::F32 => Ok(4),
            Type::I64 | Type::U64 | Type::F64 | Type::USIZE | Type::ISIZE => Ok(8),
            
            // Pointers and References are always 8 bytes (on 64-bit systems)
            Type::POINTER(_) | Type::REF(_) | Type::CONST_REF(_) => Ok(8),
            
            Type::ARRAY(inner, len) => {
                let inner_size = self.get_type_size(inner)?;
                Ok(inner_size * (*len as i64))
            },
            
            Type::STRUCT(name) => {
                if let Some(Symbol::Struct { fields }) = self.scope.resolve(name) {
                    let mut total_size = 0;

                    for (_, field_ty, _) in fields {
                        total_size += self.get_type_size(&field_ty)?;
                    }

                    Ok(total_size)
                } else {
                    Err(self.make_generic_error(format!("cannot determine size of undefined struct '{}'", name)))
                }
            },
            
            Type::VOID => Ok(0),
            _ => Err(self.make_generic_error(format!("cannot determine size of type '{}'", ty))),
        }
    }

    fn lower_call_logic(&mut self, call_name: String, token: Token, 
        arguments: Vec<ASTNode>, generic_args: Vec<Type>) 
        -> Result<Expr, HydraError<'static>> 
    {
        if call_name == "println" {
            let mut args = Vec::new();

            for arg in arguments {
                args.push(self.lower_expression(arg)?);
            }
            
            return Ok(Expr {
                kind: ExprKind::Call { 
                    callee: "println".to_string(), 
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
