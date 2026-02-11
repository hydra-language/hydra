pub mod scope;

use std::mem;
use errors::{HydraError, generic::GenericError, type_mismatch::{self, type_mismatch}};
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use ir::{Function, Program, expr::{BinaryOp, Expr, ExprKind, UnaryOp}, stmt::{Block, Stmt}, types::Type};
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
        let mut functions = Vec::new();
        let mut errors = Vec::new();
        
        for node in &nodes {
            if let ASTNode::FunctionDeclaration { name, parameters, return_type, .. } = node {
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
                        Type::VOID      // just here to make sure analysis continues but the error
                        // is collected
                    }
                };

                let symbol = Symbol::Function { 
                    params: param_types,
                    return_type: rt,
                };

                if let Err(msg) = self.scope.define(name.lexeme.to_string(), symbol) {
                    errors.push(self.make_error(msg, name));
                }
            }
        }

        for node in nodes {
            match node {
                ASTNode::FunctionDeclaration { .. } => {
                    match self.lower_function(node) {
                        Ok(function) => functions.push(function),
                        Err(e) => errors.push(e),
                    }
                },

                ASTNode::VariableDeclaration { is_const: true, .. } => {
                    // Globals (future)
                },

                _ => errors.push(self.make_generic_error(
                    "executable code is not allowed at the top level".to_string()
                ))
            }
        }

        let has_main = functions.iter().any(|f| f.name == "main"); // && f.ret_ty == Type::Void
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
            Ok(Program { functions })
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

                self.scope.define(name.lexeme.to_string(), Symbol::Variable {
                    ty: val.ty.clone(),
                    is_mutable: !is_const
                }).map_err(|msg| self.make_error(msg, &name))?;

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
                    ASTNode::VariableExpression { name } => {
                        let var_name = name.lexeme.to_string();

                        let symbol = self.scope.resolve(&var_name)
                            .ok_or(self.make_error(
                                format!("cannot assign to undefined variable '{}'", var_name),
                                &name
                            )
                        )?;

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

                        if !self.check_type_compatibility(&expected_ty, &rhs.ty) {
                            return Err(self.make_error(
                                format!("type mismatch: cannot assign '{}' to variable of type '{}'", rhs.ty, expected_ty),
                                &operator
                            ));
                        }

                        if let ExprKind::INT_LITERAL(l) = rhs.kind {
                             if self.check_and_promote_int_literal(l, &expected_ty) {
                                 rhs.ty = expected_ty.clone();
                             }
                        }

                        Ok(Stmt::Assign {
                            target: ir::stmt::AssignmentTarget::Variable(var_name),
                            value: rhs
                        })
                    },

                    ASTNode::ArrayAccess { array, index, token } => {
                        let arr_expr = self.lower_expression(*array)?;
                        let idx_expr = self.lower_expression(*index)?;

                        match &arr_expr.ty {
                            Type::ARRAY(inner, size) => {
                                if let ExprKind::INT_LITERAL(idx_val) = idx_expr.kind {
                                    if idx_val < 0 || idx_val >= (*size as i64) {
                                        return Err(self.make_error(
                                            format!(
                                                "index out of bounds: the len is {} but the index is {}", 
                                                size, idx_val
                                            ),
                                            &token
                                        ));
                                    }
                                }

                                Ok(Expr {
                                    kind: ExprKind::ArrayAccess { 
                                        array: Box::new(arr_expr), 
                                        index: Box::new(idx_expr) 
                                    },
                                    ty: *inner.clone(),
                                })
                            },

                            Type::INFERRED_ARRAY(inner) => {
                                // for now we can not check bounds at compile time
                                Ok(Expr {
                                    kind: ExprKind::ArrayAccess { 
                                        array: Box::new(arr_expr), 
                                        index: Box::new(idx_expr) 
                                    },
                                    ty: *inner.clone(),
                                })
                            },

                            _ => return Err(self.make_error(
                                format!("type '{}' cannot be indexed", arr_expr.ty), 
                                &token
                            ))
                        }

                        if !idx_expr.ty.is_numeric() {
                            return Err(self.make_error(
                                format!("array index must be numeric, found {}", idx_expr.ty), 
                                &token
                            ));
                        }

                        if !self.check_type_compatibility(inner_ty, &rhs.ty) {
                             return Err(self.make_error(
                                format!("type mismatch: expected {}, found {}", inner_ty, rhs.ty),
                                &operator
                            ));
                        }

                        if let ExprKind::INT_LITERAL(l) = rhs.kind {
                             if self.check_and_promote_int_literal(l, inner_ty) {
                                 rhs.ty = *inner_ty.clone();
                             }
                        }

                        Ok(Stmt::Assign {
                            target: ir::stmt::AssignmentTarget::ArrayAccess {
                                array: arr_expr,
                                index: idx_expr,
                            },
                            value: rhs
                        })
                    },

                    _ => return Err(self.make_generic_error(
                        "assignment target must be a variable or array_element".to_string()
                    ))
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
                    return Err(self.make_error(
                        "return statement outside of function body".to_string(), 
                        &token
                    ));
                }

                Ok(Stmt::Return(Some(val)))
            }

           ASTNode::FunctionCallExpression { .. } | ASTNode::BinaryExpression { .. } | 
            ASTNode::VariableExpression { .. } | ASTNode::Expression { .. } => 
            {
                let expr = self.lower_expression(node)?;
                Ok(Stmt::Expr(expr))
            }, 

            _ => Err(self.make_generic_error(format!("statement type {:?} is not yet supported", node)))
        }
    }

    fn lower_expression(&mut self, node: ASTNode) -> Result<Expr, HydraError<'static>> {
        match node {
            ASTNode::UnaryExpression { operator, right } => {
                let rhs = self.lower_expression(*right)?;

                match operator.token_type {
                    TokenType::Minus => {
                        if !rhs.ty.is_numeric() {
                            return Err(self.make_error(
                                format!("cannot apply negation to type '{}'", rhs.ty),
                                &operator,
                            ));
                        }

                        Ok(Expr {
                            kind: ExprKind::Unary { op: UnaryOp::NEG, operand: Box::new(rhs.clone()) },
                            ty: rhs.ty
                        })
                    },

                    TokenType::ExclamationMark => {
                        if rhs.ty != Type::BOOL {
                            return Err(self.make_error(
                                format!("cannot apply logical not to type '{}'", rhs.ty),
                                &operator
                            ));
                        }

                        Ok(Expr {
                            kind: ExprKind::Unary { op: UnaryOp::NOT, operand: Box::new(rhs) },
                            ty: Type::BOOL,
                        })
                    },

                    _ => Err(self.make_error(
                            format!("unknown unary operator: {}", operator.lexeme),
                            &operator
                    ))
                }
            }

            ASTNode::Expression { token } => {
                match token.token_type {
                    TokenType::IntLiteral(val) => {
                        let ty = if val >= (i32::MIN as i64) && val <= (i32::MAX as i64) {
                            Type::I32
                        } else {
                            Type::I64
                        };

                        Ok(Expr {
                            kind: ExprKind::INT_LITERAL(val),
                            ty
                        })
                    },

                    TokenType::StringLiteral(ref s) => Ok(Expr {
                        kind: ExprKind::STRING_LITERAL(s.clone()),
                        ty: Type::ARRAY(Box::new(Type::U8), s.len())
                    }),

                    TokenType::CharLiteral(c) => Ok(Expr {
                        kind: ExprKind::CHAR_LITERAL(c),
                        ty: Type::CHAR,
                    }),

                    TokenType::BoolLiteral(b) => Ok(Expr {
                        kind: ExprKind::INT_LITERAL(if b { 1 } else { 0 }),
                        ty: Type::BOOL,
                    }),

                    _ => Err(self.make_error(format!("unexpected literal: {:?}", token), &token))
                }
            },

            ASTNode::ArrayInitializer { elements, token: _ } => {
                let mut ir_elements = Vec::new();
                for elem in &elements {
                    ir_elements.push(self.lower_expression(elem.clone())?);
                }

                let element_type = if let Some(first) = ir_elements.first() {
                    first.ty.clone()
                } else {
                    Type::VOID
                };

                Ok(Expr {
                    kind: ExprKind::ArrayInit { elements: ir_elements },
                    ty: Type::ARRAY(Box::new(element_type), elements.len())
                })
            },

            ASTNode::ArrayAccess { array, index, token } => {
                let arr = self.lower_expression(*array)?;
                let index = self.lower_expression(*index)?;

                if !index.ty.is_numeric() {
                    return Err(self.make_error(
                        format!("array index must be an integer, found {}", index.ty),
                        &token
                    ));
                }

                match &arr.ty.clone() {
                    Type::ARRAY(inner, _) | Type::INFERRED_ARRAY(inner) => {
                        Ok(Expr {
                            kind: ExprKind::ArrayAccess {
                                array: Box::new(arr),
                                index: Box::new(index),
                            },
                            ty: *inner.clone()
                        })
                    },

                    _ => Err(self.make_error(
                        format!("type '{}' cannot be indexed", arr.ty),
                        &token
                    ))
                }
            }

            ASTNode::VariableExpression { name, .. } => {
                let symbol = self.scope.resolve(name.lexeme)
                    .ok_or(self.make_error(format!("undefined variable: {}", name.lexeme), &name))?;

                match symbol {
                    Symbol::Variable { ty, .. } => Ok(Expr {
                        kind: ExprKind::VariableReference { name: name.lexeme.to_string() },
                        ty: ty.clone(),
                    }),

                    _ => Err(self.make_error(format!("'{}' is a function, not a variable", name.lexeme), &name))
                }
            },

            ASTNode::BinaryExpression { left, operator, right } => {
                let lhs = self.lower_expression(*left)?;
                let rhs = self.lower_expression(*right)?;

                if lhs.ty != rhs.ty {
                    return Err(self.make_error(
                        format!("type mismatch: {} and {} have different types", lhs.ty, rhs.ty),
                        &operator
                    ));
                }

                Ok(Expr {
                    kind: ExprKind::Binary {
                    op: match operator.token_type {
                        TokenType::Plus => BinaryOp::ADD,
                        TokenType::Minus => BinaryOp::SUB,
                        TokenType::Star => BinaryOp::MUL,
                        TokenType::ForwardSlash => BinaryOp::DIV,
                            
                        _ => return Err(self.make_error(format!("unknown binary operator: {}", operator.lexeme), &operator))
                    },
                    lhs: Box::new(lhs.clone()),
                    rhs: Box::new(rhs)
                    },

                    ty: lhs.ty,
                })
            },

            ASTNode::FunctionCallExpression { name, arguments } => {
                if name.lexeme == "println" {
                    let mut args = Vec::new();

                    for arg in arguments {
                        args.push(self.lower_expression(arg)?);
                    }

                    return Ok(Expr {
                        kind: ExprKind::Call { callee: name.lexeme.to_string(), args },
                        ty: Type::VOID,
                    })
                }

                let symbol = self.scope.resolve(name.lexeme)
                    .ok_or(self.make_error(format!("undefined function: {}", name.lexeme), &name))?;

                let (param_types, return_type) = match symbol {
                    Symbol::Function { params, return_type } => (params.clone(), return_type.clone()),
                    _ => return Err(self.make_error(
                        format!("'{}' is a variable, not a function", name.lexeme), 
                        &name
                    )),
                };

                if arguments.len() != param_types.len() {
                    return Err(self.make_error(
                        format!(
                            "function '{}' expects {} args, got {}",
                            name.lexeme, param_types.len(), arguments.len()
                        ), 
                        &name
                    ));
                }

                let mut args = Vec::new();
                for (i, node) in arguments.into_iter().enumerate() {
                    let token = self.get_token_from_node(&node);

                    let mut arg = self.lower_expression(node)?;
                    let expected = &param_types[i];

                    if let ExprKind::INT_LITERAL(l) = arg.kind {
                        if self.check_and_promote_int_literal(l, expected) {
                            arg.ty = expected.clone();
                        }
                    }

                    if !self.check_type_compatibility(expected, &arg.ty) {
                        return Err(self.make_error(
                            format!(
                                "type mismatch: expected {}, found {}",
                                expected, arg.ty
                            ),
                            &token
                        ))
                    }
                    args.push(arg);
                }

                Ok(Expr {
                    kind: ExprKind::Call { callee: name.lexeme.to_string(), args },
                    ty: return_type,
                })
                
            },

            ASTNode::MemberExpression { object, property } => {
                let lhs = self.lower_expression(*object)?;

                if property.lexeme == "len" {
                    match &lhs.ty {
                        Type::ARRAY(_, size) => {
                            Ok(Expr {
                                kind: ExprKind::INT_LITERAL(*size as i64),
                                ty: Type::I32,
                            })
                        },

                        Type::INFERRED_ARRAY(_) => {
                            match &lhs.kind {
                                ExprKind::STRING_LITERAL(s) => {
                                    Ok(Expr {
                                        kind: ExprKind::INT_LITERAL(s.len() as i64),
                                        ty: Type::I32
                                    })
                                },

                                ExprKind::ArrayInit { elements } => {
                                    Ok(Expr {
                                        kind: ExprKind::INT_LITERAL(elements.len() as i64),
                                        ty: Type::I32,
                                    })
                                },

                                _ => {
                                    // for now, we cannot resolve at comptime if something like a
                                    // function param, but later we will have comptime execution to
                                    // be able to resolve these if possible
                                    Err(self.make_error(
                                        format!("length cannot be determined at compile time for 'anysize' type '{}'", lhs.ty),
                                        &property
                                    ))
                                }
                            }
                        },

                        _ => {
                            Err(self.make_error(
                                format!("type '{}' has no property '{}'", lhs.ty, property.lexeme),
                                &property
                            ))
                        }
                    }
                } else {
                    Err(self.make_error(
                        format!("property '{}' does not exist", property.lexeme), 
                        &property
                    ))
                }
            },

            _ => Err(self.make_generic_error(format!("expression not supported yet: {:?}", node))),
        }
    }

    fn lower_function(&mut self, node: ASTNode) -> Result<Function, HydraError<'static>> {
        if let ASTNode::FunctionDeclaration { name, parameters, return_type: rt, body } = node {
            let return_type = self.lower_type(*rt)?;

            let prev_return_type = self.current_return_type.replace(return_type.clone());

            let mut ir_params = Vec::new();

            self.enter_scope();
            for (param_name, param_type_node) in &parameters {
                let ty = self.lower_type(*param_type_node.clone())?;
                ir_params.push((param_name.lexeme.to_string(), ty.clone()));

                self.scope.define(param_name.lexeme.to_string(), Symbol::Variable {
                    ty,
                    is_mutable: true
                }).map_err(|msg| self.make_generic_error(msg))?;
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
            })
        } else {
            Err(self.make_generic_error("expected FunctionDeclaration".to_string()))
        }
    }

    fn lower_type(&mut self, node: ASTNode) -> Result<Type, HydraError<'static>> {
        match node {
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

                    _ => Err(self.make_error(
                        format!("unknown type: {}", type_token.lexeme), 
                        &type_token
                    )),
                }
            },

            ASTNode::ArrayType { element_type, size, .. } => {
                let inner = self.lower_type(*element_type)?;
                
                let size_token = self.get_token_from_node(&size);
                
                match size_token.token_type {
                    TokenType::IntLiteral(n) => Ok(Type::ARRAY(Box::new(inner), n as usize)),
                    TokenType::AnySize => Ok(Type::INFERRED_ARRAY(Box::new(inner))),

                    _ => Err(self.make_error(
                        "array size must be an integer literal or 'anysize'".to_string(), 
                        &size_token
                    ))
                }
            },

            _ => Err(self.make_generic_error(format!("invalid type syntax: {:?}", node))),
        }
    }

    fn enter_scope(&mut self) {
        let parent = mem::replace(&mut self.scope, Scope::new());
        self.scope = Scope::new_child(parent);
    }

    fn leave_scope(&mut self) {
        let current_scope = mem::replace(&mut self.scope, Scope::new());
        let parent = current_scope.parent().expect("compiler bug: popped global scope");
        self.scope = parent;
    }

    fn dummy_token(&self) -> Token<'static> {
        Token {
            token_type: TokenType::EOF,
            lexeme: "",
            line: 0,
            column: 0,
        }
    }

    fn make_error(&self, msg: String, token: &Token) -> HydraError<'static> {
        HydraError::GENERIC(Box::new(GenericError {
            code: "E000",
            message: msg,
            token: Token {
                token_type: token.token_type.clone(), 
                lexeme: "",
                line: token.line,
                column: token.column
            }, 
            help: None
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
            
            // i32 fits in i64/isize naturally
            Type::I64 | Type::ISIZE => true,
            // i32 fits in u64/usize only if positive
            Type::U64 | Type::USIZE => lit_val >= 0,

            Type::BOOL => lit_val == 0 || lit_val == 1,
            
            // Cannot implicit cast int literal to float, etc
            _ => false, 
        }
    }

    fn check_type_compatibility(&self, target: &Type, source: &Type) -> bool {
        if target == source {
            return true;
        }

        match (target, source) {
            (Type::INFERRED_ARRAY(target_inner), Type::ARRAY(source_inner, _)) => {
                target_inner == source_inner
            }

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
}
