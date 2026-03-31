pub mod scope;
pub mod expr;
pub mod stmt;
pub mod types;
pub mod utils;

use errors::{HydraError, generic::GenericError};
use parser::ast::ASTNode;
use ir::{Function, Program, expr::ExprKind, stmt::Block, types::Type};
use scope::Scope;

use crate::scope::Symbol;

#[derive(Default)]
pub struct Analyzer {
    scope: Scope,
    current_return_type: Option<Type>,
}

impl Analyzer {

    pub fn new() -> Self {
        Self::default()
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
            if let ASTNode::StructDeclaration { name, fields, methods, constants, generic_params, is_pub: _ } = node {
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
            if let ASTNode::ExtensionDeclaration { target, constants, methods } = node {
                let target_name = match &**target {
                    ASTNode::TypeIdentifier { type_token } => type_token.lexeme.to_string(),
                    _ => continue,
                };

                for constant in constants {
                    if let ASTNode::VariableDeclaration { name: c_name, type_annotation, .. } = constant {
                        let ty = type_annotation.as_ref()
                            .and_then(|ann| self.lower_type(*ann.clone()).ok())
                            .unwrap_or(Type::VOID);

                        let const_name = format!("{}::{}", target_name, c_name.lexeme);
                        self.scope.define(const_name, Symbol::Variable {
                            ty,
                            is_mutable: false,
                        }).ok();
                    }
                }

                for method in methods {
                    if let ASTNode::FunctionDeclaration { name: m_name, parameters, return_type, generic_params, .. } = method {
                        let namespaced_name = format!("{}::{}", target_name, m_name.lexeme);

                        if self.scope.resolve(&namespaced_name).is_some() { continue; }

                        let mut param_types = Vec::new();
                        for (_, type_node) in parameters {
                            param_types.push(self.lower_type(*type_node.clone()).unwrap_or(Type::VOID));
                        }

                        let rt = self.lower_type(*return_type.clone()).unwrap_or(Type::VOID);

                        self.scope.define(namespaced_name, Symbol::Function {
                            params: param_types,
                            annotations: Vec::new(),
                            return_type: rt,
                            generic_params: generic_params.iter().map(|t| t.lexeme.to_string()).collect(),
                        }).ok();
                    }
                }
            }

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
                        if let ASTNode::VariableDeclaration { name: c_name, initializer, type_annotation, .. } = constant {
                            let full_name = format!("{}.{}", struct_name, c_name.lexeme);

                            if globals.iter().any(|(n, _, _)| n == &full_name) { 
                                continue; 
                            }

                            let expected_ty = type_annotation.as_ref()
                                .and_then(|ann| self.lower_type(*ann.clone()).ok())
                                .unwrap_or(Type::VOID);

                            match self.lower_expression(*initializer.clone()) {
                                Ok(mut init_expr) => {
                                    if let ExprKind::INT_LITERAL(l) = init_expr.kind {
                                        if self.check_and_promote_int_literal(l, &expected_ty) {
                                            init_expr.ty = expected_ty.clone();
                                        }
                                    }
                                    globals.push((full_name, expected_ty, init_expr))
                                },
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

                ASTNode::ExtensionDeclaration { target, constants, methods } => {
                    let target_name = match &*target {
                        ASTNode::TypeIdentifier { type_token } => type_token.lexeme.to_string(),
                        _ => continue,
                    };

                    for constant in constants {
                        if let ASTNode::VariableDeclaration { name: c_name, initializer, type_annotation, .. } = constant {
                            let full_name = format!("{}::{}", target_name, c_name.lexeme);
                            
                            if globals.iter().any(|(n, _, _)| n == &full_name) {
                                continue;
                            }

                            let expected_ty = type_annotation.as_ref()
                                .and_then(|ann| self.lower_type(*ann.clone()).ok())
                                .unwrap_or(Type::VOID);

                            match self.lower_expression(*initializer.clone()) {
                                Ok(mut init_expr) => {
                                    if let ExprKind::INT_LITERAL(l) = init_expr.kind {
                                        if self.check_and_promote_int_literal(l, &expected_ty) {
                                            init_expr.ty = expected_ty.clone();
                                        }
                                    }
                                    globals.push((full_name, expected_ty, init_expr))
                                },
                                Err(e) => errors.push(e)
                            }
                        }
                    }

                    for method in methods {
                        let method_name = if let ASTNode::FunctionDeclaration { name: ref n, .. } = method { 
                            n.lexeme 
                        } else { 
                            "" 
                        };

                        let full_name = format!("{}::{}", target_name, method_name);
                        if functions.iter().any(|f| f.name == full_name) {
                            continue;
                        }

                        let lowered = self.lower_function(method);

                        match lowered {
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

    fn lower_function(&mut self, node: ASTNode) -> Result<Function, HydraError<'static>> {
        if let ASTNode::FunctionDeclaration { 
            name,
            annotations,
            generic_params, 
            parameters, 
            return_type: rt, 
            body, 
            is_extern,
            is_pub
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
}
