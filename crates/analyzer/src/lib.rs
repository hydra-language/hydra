pub mod scope;

use std::mem;
use errors::{HydraError, generic::GenericError};
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use ir::{expr::{Expr, ExprKind, BinaryOp}, types::Type, stmt::{Stmt, Block}, Program, Function};
use scope::Scope;

use crate::scope::Symbol;

pub struct Analyzer {
    scope: Scope,
}

impl Analyzer {

    pub fn new() -> Self {
        Self {
            scope: Scope::new(),
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

                let rt = self.lower_type(*return_type.clone()).unwrap_or(Type::VOID); // need to
                // change. i dont allow implicit void

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
            ASTNode::VariableDeclaration { name, type_annotation, initializer, .. } => {
                let mut val = self.lower_expression(*initializer)?;

                if let Some(ann) = type_annotation {
                    let explicit = self.lower_type(*ann)?;

                    if let ExprKind::INT_LITERAL(l) = val.kind {
                        if !self.check_and_promote_int_literal(l as i32, &explicit) {
                            return Err(self.make_error(
                                format!("literal value {} does not fit into type {}", l, explicit),
                                &name
                            ));
                        }

                        val.ty = explicit.clone();
                    }

                    if val.ty != explicit {
                        return Err(self.make_error(
                            format!("type mismatch: expected {}, found {}", explicit, val.ty),
                            &name
                        ));
                    }
                }
                // Scope errors are strings currently, map them to HydraError
                self.scope.define(name.lexeme.to_string(), Symbol::Variable {
                    ty: val.ty.clone(),
                    is_mutable: true
                }).map_err(|msg| self.make_error(msg, &name))?;

                Ok(Stmt::Var {
                    name: name.lexeme.to_string(),
                    ty: val.ty.clone(),
                    init: val,
                    is_mutable: true
                })
            },

            ASTNode::AssignmentExpression { target, operator, value } => {
                let var_name = match *target {
                    ASTNode::VariableExpression { name } => name.lexeme.to_string(),

                    // this makes no sense. why cant the target be a variable? that only works if
                    // the variable is const
                    _ => return Err(self.make_generic_error(
                            "assignment target must not be a variable".to_string())
                    )
                };

                if self.scope.resolve(&var_name).is_none() {
                    return Err(self.make_generic_error(
                        format!("cannot assign to undefined variable '{}'", var_name))
                    );
                }

                let rhs = self.lower_expression(*value)?;

                // TODO: type checking here

                if operator.token_type == TokenType::Equal {
                    Ok(Stmt::Assign {
                        name: var_name,
                        value: rhs
                    })
                } else {
                    Err(self.make_generic_error(
                        "compound assignment not yet implemented".to_string())
                    )
                }
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
            ASTNode::Expression { token } => {
                match token.token_type {
                    TokenType::IntLiteral(val) => Ok(Expr {
                        kind: ExprKind::INT_LITERAL(val),
                        ty: Type::I32,
                    }),

                    TokenType::StringLiteral(ref s) => Ok(Expr {
                        kind: ExprKind::STRING_LITERAL(s.clone()),
                        ty: Type::I8
                    }),

                    _ => Err(self.make_error(format!("unexpected literal: {:?}", token), &token))
                }
            },

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
                    _ => return Err(self.make_error(format!("'{}' is a variable, not a function", name.lexeme), &name)),
                };

                if arguments.len() != param_types.len() {
                    return Err(self.make_error(format!(
                        "function '{}' expects {} args, got {}",
                        name.lexeme, param_types.len(), arguments.len()), &name));
                }

                let mut args = Vec::new();
                for node in arguments {
                    args.push(self.lower_expression(node)?);
                }

                Ok(Expr {
                    kind: ExprKind::Call { callee: name.lexeme.to_string(), args },
                    ty: return_type,
                })
                
            },

            _ => Err(self.make_generic_error(format!("expression not supported yet: {:?}", node))),
        }
    }

    fn lower_function(&mut self, node: ASTNode) -> Result<Function, HydraError<'static>> {
        if let ASTNode::FunctionDeclaration { name, parameters, return_type: rt, body } = node {
            let return_type = self.lower_type(*rt)?;

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

                    _ => Err(self.make_error(format!("unknown type: {}", type_token.lexeme), &type_token)),
                }
            },

            ASTNode::ArrayType { element_type, .. } => {
                let inner = self.lower_type(*element_type)?;
                Ok(Type::ARRAY(Box::new(inner), 0))
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

    fn check_and_promote_int_literal(&self, lit_val: i32, target_ty: &Type) -> bool {
        match target_ty {
            Type::I8  => lit_val >= (i8::MIN as i32) && lit_val <= (i8::MAX as i32),
            Type::U8  => lit_val >= 0 && lit_val <= (u8::MAX as i32),
            Type::I16 => lit_val >= (i16::MIN as i32) && lit_val <= (i16::MAX as i32),
            Type::U16 => lit_val >= 0 && lit_val <= (u16::MAX as i32),
            Type::I32 => true,
            Type::U32 => lit_val >= 0,
            
            // i32 fits in i64/isize naturally
            Type::I64 | Type::ISIZE => true,
            // i32 fits in u64/usize only if positive
            Type::U64 | Type::USIZE => lit_val >= 0,
            
            // Cannot implicit cast int literal to float/bool/etc
            _ => false, 
        }
    }
}
