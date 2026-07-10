use std::collections::HashMap;

use crate::scope::Scope;
use crate::fold::const_fold_hir;
use errors::error::{HydraError, Span};

use parser::ast::ASTNode;
use parser::program::Program as ASTProgram;

use ir::types::Type;
use ir::context::{HIRContext, DefID, DefKind, SymbolInfo};
use ir::hir::{HIRBlock, HIRFunction, HIRProgram};

#[derive(Default)]
pub struct Analyzer {
    pub scope: Scope,
    pub context: HIRContext,
    pub global_symbols: HashMap<Vec<String>, DefID>,
    pub impl_registry: HashMap<String, HashMap<String, DefID>>,
    pub(crate) current_return_type: Option<Type>,
    pub(crate) current_module: Vec<String>,
    pub(crate) current_struct: Option<String>,
    pub(crate) current_source: String,
    pub(crate) current_generics: Vec<String>,
}

impl Analyzer {

    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn error(&self, code: &'static str, message: impl Into<String>, span: Span) -> HydraError {
        let filename = if self.current_module.is_empty() {
            "main.hydra".to_string()
        } else {
            format!("{}.hydra", self.current_module.join("/"))
        };
        HydraError::new(code, message, span)
            .with_file(filename, self.current_source.clone())
    }

    pub fn analyze(&mut self, program: &ASTProgram) -> Result<HIRProgram, Vec<HydraError>> {
        let mut functions: Vec<HIRFunction> = Vec::new();
        let mut errors = Vec::new();
        let mut structs = Vec::new();
        let mut globals = Vec::new();

        for (module_name, module) in &program.modules {
            let path_prefix = module_name.clone(); 

            self.current_module = path_prefix.clone();
            self.current_source = module.0.to_string();

            for node in &module.1 {
                self.register_global_item(&path_prefix, node, &mut errors);
            }
        }

        if !errors.is_empty() { return Err(errors); }

        let mut root_module_name = String::new();
        for (name, module) in &program.modules {
            for node in &module.1 {
                if let ASTNode::FunctionDeclaration { name: fn_name, .. } = node {
                    if fn_name.lexeme == "main" {
                        root_module_name = name.join("::");
                        break;
                    }
                }
            }
        }

        if root_module_name.is_empty() {
            let mut keys: Vec<_> = program.modules.keys().collect();
            keys.sort();
            if !keys.is_empty() {
                root_module_name = keys[0].join("::");
            }
        }

        // --- PASS 1: Global Signatures ---
        for (module_name, module) in &program.modules {
            let module_name_str = module_name.join("::");
            let path_prefix = self.get_path_from_module_name(&module_name_str, &root_module_name);
            self.scope = Scope::new(path_prefix.clone());
            self.current_module = path_prefix.clone();
            self.current_source = module.0.to_string();

            for node in &module.1 {
                if let ASTNode::IncludeStatement { path, symbols, alias } = node {
                    // Extract the base path (e.g., "std::random")
                    let (base_path, end_span) = match &**path {
                        ASTNode::VariableExpression { name } => {
                            (vec![name.lexeme.to_string()], name.span)
                        },
                        ASTNode::PathExpression { segments } => {
                            let strings = segments.iter().map(|t| t.lexeme.to_string()).collect();
                            (strings, segments.last().unwrap().span)
                        },
                        _ => continue,
                    };

                    if let Some(syms) = symbols {
                        // Handle: include std::random::{Random, seed};
                        for sym in syms {
                            let local_name = sym.lexeme.to_string();

                            // Build the absolute path to the specific symbol
                            let mut full_target_path = base_path.clone();
                            full_target_path.push(local_name.clone());

                            let info = SymbolInfo {
                                name: local_name.clone(),
                                span: sym.span,
                                absolute_path: full_target_path.clone(),
                                kind: DefKind::Alias { target_path: full_target_path },
                                is_pub: false,
                            };

                            let def_id = self.context.insert_def(info);
                            self.scope.define(local_name, def_id).ok();
                        }
                    } else {
                        // Handle: include std::random; OR include std::random as rng;
                        let local_name = alias.as_ref()
                            .map(|t| t.lexeme.to_string())
                            .unwrap_or_else(|| base_path.last().unwrap().clone());

                        let info = SymbolInfo {
                            name: local_name.clone(),
                            span: alias.as_ref().map(|t| t.span).unwrap_or(end_span),
                            absolute_path: base_path.clone(),
                            kind: DefKind::Alias { target_path: base_path },
                            is_pub: false,
                        };

                        let def_id = self.context.insert_def(info);
                        self.scope.define(local_name, def_id).ok();
                    }
                }
            }

            for node in &module.1 {
                self.register_global_item(&path_prefix, node, &mut errors);
            }
        }

        // --- PASS 2: Deep Lowering ---
        for (module_name, module) in &program.modules {
            let module_name_str = module_name.join("::");
            let path_prefix = self.get_path_from_module_name(&module_name_str, &root_module_name);
            
            self.current_module = path_prefix.clone();
            self.current_source = module.0.to_string();
            self.scope = Scope::new(path_prefix.clone());

            // 0. Re-inject globals
            for (path, &def_id) in &self.global_symbols {
                if path.len() == 1 || path.starts_with(&path_prefix) {
                    let local_name = path.last().unwrap().clone();
                    self.scope.define(local_name, def_id).ok();
                }
            }

            // 1. Register Local Includes/Aliases
            for node in &module.1 {
                if let ASTNode::IncludeStatement { path, symbols, alias } = node {
                    // Extract the base path (e.g., "std::random")
                    let (base_path, end_span) = match &**path {
                        ASTNode::VariableExpression { name } => {
                            (vec![name.lexeme.to_string()], name.span)
                        },
                        ASTNode::PathExpression { segments } => {
                            let strings = segments.iter().map(|t| t.lexeme.to_string()).collect();
                            (strings, segments.last().unwrap().span)
                        },
                        _ => continue,
                    };

                    if let Some(syms) = symbols {
                        // Handle: include std::random::{Random, seed};
                        for sym in syms {
                            let local_name = sym.lexeme.to_string();

                            // Build the absolute path to the specific symbol
                            let mut full_target_path = base_path.clone();
                            full_target_path.push(local_name.clone());

                            if let Some(&target_def_id) = self.global_symbols.get(&full_target_path) {
                                let target_info = self.context.get_def(target_def_id).unwrap();

                                if !target_info.is_pub && self.current_module != base_path {
                                    errors.push(self.error(
                                        "S020",
                                        format!("'{}' is private and cannot be included", local_name),
                                        sym.span
                                    ).with_help(format!("declare it as 'pub' in '{}'", base_path.join("::"))));
                                    
                                    continue;
                                }
                            } else {
                                errors.push(self.error("S021", format!("unresolved import `{}`", local_name), sym.span));
                                continue;
                            }

                            let info = SymbolInfo {
                                name: local_name.clone(),
                                span: sym.span,
                                absolute_path: full_target_path.clone(),
                                kind: DefKind::Alias { target_path: full_target_path },
                                is_pub: false
                            };

                            let def_id = self.context.insert_def(info);
                            self.scope.define(local_name, def_id).ok();
                        }
                    } else {
                        // Handle: include std::random; OR include std::random as rng;
                        let local_name = alias.as_ref()
                            .map(|t| t.lexeme.to_string())
                            .unwrap_or_else(|| base_path.last().unwrap().clone());

                        let info = SymbolInfo {
                            name: local_name.clone(),
                            span: alias.as_ref().map(|t| t.span).unwrap_or(end_span),
                            absolute_path: base_path.clone(),
                            kind: DefKind::Alias { target_path: base_path },
                            is_pub: false,
                        };

                        let def_id = self.context.insert_def(info);
                        self.scope.define(local_name, def_id).ok();
                    }
                }            
            }

            // 2. Lower Bodies
            for node in &module.1 {
                match node {
                    ASTNode::FunctionDeclaration { name, parameters, return_type, body, is_extern, generic_params, annotations, .. } => {
                        self.enter_scope();

                        let param_names: Vec<String> = generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                        self.current_generics.extend(param_names.clone());

                        let rt = match self.lower_type(*return_type.clone()) {
                            Ok(t) => t,
                            Err(e) => { errors.push(e); Type::VOID }
                        };

                        self.current_return_type = Some(rt.clone());

                        let mut ir_params = Vec::new();
                        for (p_name, p_type_node) in parameters {
                            let p_ty = match self.lower_type(*p_type_node.clone()) {
                                Ok(t) => t,
                                Err(e) => { errors.push(e); Type::VOID }
                            };
                            
                            let info = SymbolInfo {
                                name: p_name.lexeme.to_string(),
                                span: p_name.span,
                                absolute_path: vec![p_name.lexeme.to_string()],
                                kind: DefKind::Variable { ty: p_ty.clone(), is_mutable: true },
                                is_pub: false,
                            };
                            let def_id = self.context.insert_def(info);
                            self.scope.define(p_name.lexeme.to_string(), def_id).ok();
                            
                            ir_params.push((def_id, p_ty)); // HIR utilizes (DefId, Type)
                        }

                        let mut ir_body = Vec::new();
                        for stmt in body {
                            match self.lower_statement(stmt.clone()) {
                                Ok(s) => ir_body.push(s),
                                Err(e) => errors.push(e),
                            }
                        }

                        let mut full_path = self.current_module.clone();
                        full_path.push(name.lexeme.to_string());
                        
                        let fn_def_id = *self.global_symbols.get(&full_path).unwrap();
                        let is_intrinsic = annotations.iter().any(|a| a.name == "intrinsic");
                        let is_inline = annotations.iter().any(|a| a.name == "inline");

                        functions.push(HIRFunction {
                            name: full_path.join("::"), 
                            def_id: fn_def_id,
                            params: ir_params,
                            return_type: rt,
                            body: HIRBlock { stmts: ir_body, span: name.span },
                            is_extern: *is_extern,
                            is_intrinsic,
                            is_inline,
                            generic_params: generic_params.iter().map(|t| t.lexeme.to_string()).collect(),
                        });

                        self.current_generics.truncate(self.current_generics.len() - param_names.len());
                        self.leave_scope();
                        self.current_return_type = None;
                    }

                    ASTNode::StructDeclaration { name, fields, constants, generic_params, is_pub, .. } => {
                        self.current_struct = Some(name.lexeme.to_string());

                        let param_names: Vec<String> = generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                        self.current_generics.extend(param_names.clone());

                        let mut struct_path = self.current_module.clone();
                        struct_path.push(name.lexeme.to_string());

                        let mut ir_fields = Vec::new();
                        let mut symbol_fields = Vec::new();

                        for (f_name, f_type) in fields {
                            match self.lower_type(*f_type.clone()) {
                                Ok(ty) => {
                                    ir_fields.push((f_name.lexeme.to_string(), ty.clone()));
                                    symbol_fields.push((f_name.lexeme.to_string(), ty, false));
                                }
                                Err(e) => errors.push(e),
                            }
                        }
                        structs.push((struct_path.join("::"), ir_fields));

                        for constant in constants {
                            if let ASTNode::VariableDeclaration { name: c_name, initializer, type_annotation, is_const, .. } = constant {
                                let mut const_path = struct_path.clone();
                                const_path.push(c_name.lexeme.to_string());
                                let expected_ty = type_annotation.as_ref()
                                    .and_then(|ann| self.lower_type(*ann.clone()).ok());

                                match self.lower_expr_with_type(*initializer.clone(), expected_ty.as_ref()) {
                                    Ok(mut init_expr) => {
                                        if let Some(ref target) = expected_ty {
                                            init_expr = self.coerce_primitive(init_expr, target);
                                        }
                                        let final_ty = expected_ty.unwrap_or(init_expr.ty.clone());

                                        if *is_const {
                                            if let Some(folded) = const_fold_hir(&init_expr, &self.context) {
                                                if let Some(&def_id) = self.global_symbols.get(&const_path) {
                                                    let mut info = self.context.get_def(def_id).unwrap().clone();
                                                    if let DefKind::Constant { ref mut value, .. } = info.kind {
                                                        *value = folded;
                                                    }
                                                    self.context.update_def(def_id, info);
                                                }
                                            }
                                        }

                                        globals.push((const_path.join("::"), final_ty.clone(), init_expr));
                                        symbol_fields.push((c_name.lexeme.to_string(), final_ty, *is_const));
                                    }
                                    Err(e) => errors.push(e),
                                }
                            }
                        }

                        // Update the Pass 1 dummy struct with the actual loaded fields
                        let info = SymbolInfo {
                            name: name.lexeme.to_string(),
                            span: name.span,
                            absolute_path: struct_path.clone(),
                            kind: DefKind::Struct { 
                                fields: symbol_fields, 
                                generic_params: generic_params.iter().map(|t| t.lexeme.to_string()).collect() 
                            },
                            is_pub: *is_pub, // Preserving visibility across the update
                        };

                        let def_id = *self.global_symbols.get(&struct_path).unwrap();
                        self.context.update_def(def_id, info);

                        self.current_struct = None;
                        self.current_generics.truncate(self.current_generics.len() - param_names.len());
                    }

                    ASTNode::VariableDeclaration { name, initializer, type_annotation, .. } => {
                        let mut const_path = self.current_module.clone();
                        const_path.push(name.lexeme.to_string());
                        
                        let expected_ty = type_annotation.as_ref()
                            .and_then(|ann| self.lower_type(*ann.clone()).ok());
                        match self.lower_expr_with_type(*initializer.clone(), expected_ty.as_ref()) {
                            Ok(mut init_expr) => {
                                if let Some(ref target) = expected_ty {
                                    init_expr = self.coerce_primitive(init_expr, target);
                                }
                                let final_ty = expected_ty.unwrap_or(init_expr.ty.clone());
                                globals.push((const_path.join("::"), final_ty, init_expr));
                            }
                            Err(e) => errors.push(e),
                        }
                    }

                    ASTNode::ExtensionDeclaration { target, constants, methods, generic_params, .. } => {
                        let param_names: Vec<String> = generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                        self.current_generics.extend(param_names.clone());

                        let target_type = self.lower_type(*target.clone()).unwrap_or(Type::VOID);
                        let target_path = match &target_type {
                            Type::STRUCT(full_path) => full_path.split("::").map(|s| s.to_string()).collect(),

                            Type::GENERIC_INSTANCE(base, _) => {
                                if let Type::STRUCT(full_path) = &**base {
                                    full_path.split("::").map(|s| s.to_string()).collect()
                                } else {
                                    self.current_module.clone()
                                }
                            },

                            _ => {
                                let target_name = match &**target {
                                    ASTNode::TypeIdentifier { type_token } => type_token.lexeme.to_string(),

                                    ASTNode::GenericType { base, .. } => {
                                        if let ASTNode::TypeIdentifier { type_token } = &**base {
                                            type_token.lexeme.to_string()
                                        } else { String::new() }
                                    }

                                    _ => String::new(),
                                };

                                let mut p = self.current_module.clone();
                                if !target_name.is_empty() {
                                    p.push(target_name); 
                                }

                                p
                            }
                        };
                        
                        let struct_name = match &target_type {
                            Type::STRUCT(name) => name.clone(),
                            Type::GENERIC_INSTANCE(base, _) => {
                                if let Type::STRUCT(name) = base.as_ref() { name.clone() }
                                else { String::new() }
                            }
                            _ => String::new(),
                        };
                        let type_methods = self.impl_registry.get(&struct_name).cloned().unwrap_or_default(); 

                        for method in methods {
                            if let ASTNode::FunctionDeclaration { 
                                name: m_name, 
                                parameters, 
                                return_type, 
                                body, 
                                generic_params: m_generic_params, 
                                annotations, is_extern, .. 
                            } = method 
                            {
                                self.enter_scope();

                                let m_param_names: Vec<String> = m_generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                                self.current_generics.extend(m_param_names.clone());

                                let rt = match self.lower_type(*return_type.clone()) {
                                    Ok(t) => t,
                                    Err(e) => { errors.push(e); Type::VOID }
                                };

                                self.current_return_type = Some(rt.clone());

                                let mut ir_params = Vec::new();
                                for (p_name, p_type_node) in parameters {
                                    let p_ty = match self.lower_type(*p_type_node.clone()) {
                                        Ok(t) => t,
                                        Err(e) => { errors.push(e); Type::VOID }
                                    };
                                    
                                    let info = SymbolInfo {
                                        name: p_name.lexeme.to_string(),
                                        span: p_name.span,
                                        absolute_path: vec![p_name.lexeme.to_string()],
                                        kind: DefKind::Variable { ty: p_ty.clone(), is_mutable: true },
                                        is_pub: false,
                                    };
                                    let def_id = self.context.insert_def(info);
                                    self.scope.define(p_name.lexeme.to_string(), def_id).ok();
                                    
                                    ir_params.push((def_id, p_ty));
                                }

                                let mut ir_body = Vec::new();
                                for stmt in body {
                                    match self.lower_statement(stmt.clone()) {
                                        Ok(s) => ir_body.push(s),
                                        Err(e) => errors.push(e),
                                    }
                                }

                                let mut m_path = target_path.clone();
                                m_path.push(m_name.lexeme.to_string());

                                let is_intrinsic = annotations.iter().any(|a| a.name == "intrinsic");
                                let is_inline = annotations.iter().any(|a| a.name == "inline");
                                let m_def_id = *type_methods.get(m_name.lexeme).unwrap(); // Fetch DefId assigned in Pass 1

                                functions.push(HIRFunction {
                                    name: m_path.join("::"), 
                                    def_id: m_def_id,
                                    params: ir_params,
                                    return_type: rt.clone(),
                                    body: HIRBlock { stmts: ir_body, span: m_name.span },
                                    is_extern: *is_extern,
                                    is_intrinsic,
                                    is_inline,
                                    generic_params: generic_params.iter().map(|t| t.lexeme.to_string()).collect(),
                                });

                                self.leave_scope();
                                self.current_return_type = None;
                                self.current_generics.truncate(self.current_generics.len() - m_param_names.len());
                            }
                        }

                        if !target_path.is_empty() {
                            for constant in constants {
                                if let ASTNode::VariableDeclaration { name: c_name, initializer, type_annotation, is_const, .. } = constant {
                                    let mut c_path = target_path.clone();
                                    c_path.push(c_name.lexeme.to_string());

                                    let expected_ty = type_annotation.as_ref()
                                        .and_then(|ann| self.lower_type(*ann.clone()).ok());
                                    match self.lower_expr_with_type(*initializer.clone(), expected_ty.as_ref()) {
                                        Ok(mut init_expr) => {
                                            if let Some(ref target_ty) = expected_ty {
                                                init_expr = self.coerce_primitive(init_expr, target_ty);
                                            }
                                            let final_ty = expected_ty.unwrap_or(init_expr.ty.clone());

                                            // Fold and patch the dummy placeholder value from Pass 1
                                            if let Some(folded) = const_fold_hir(&init_expr, &self.context) {
                                                if let Some(&def_id) = self.global_symbols.get(&c_path) {
                                                    let mut info = self.context.get_def(def_id).unwrap().clone();
                                                    if let DefKind::Constant { ref mut value, .. } = info.kind {
                                                        *value = folded;
                                                    }
                                                    self.context.update_def(def_id, info);
                                                }
                                            }

                                            globals.push((c_path.join("::"), final_ty.clone(), init_expr));
                                        }
                                        Err(e) => errors.push(e),
                                    }
                                }
                            }
                        }

                        self.current_generics.truncate(self.current_generics.len() - param_names.len());
                    }

                    ASTNode::IncludeStatement { .. } => {} 
                    node => {
                        let span = self.get_token_from_node(node).span;
                        errors.push(self.error("S017", "executable code is not allowed at the top level", span));
                    }
                }
            }
        }

        let has_main = functions.iter().any(|f| f.name == "main");
        if !has_main && errors.is_empty() {
            errors.push(HydraError::new(
                "S015", 
                "program is missing an entry point",
                Span::default()
            ).with_help("consider adding `fn main() -> void`"));
        }

        if !errors.is_empty() { Err(errors) } else { Ok(HIRProgram { functions, structs, globals }) }
    }

    fn get_path_from_module_name(&self, name: &str, root_module_name: &str) -> Vec<String> {
        if name == root_module_name { return vec![]; }
        name.split("::").map(|s| s.to_string()).collect()
    }


    fn register_global_item(&mut self, prefix: &[String], node: &ASTNode, errors: &mut Vec<HydraError>) {
        match node {
            ASTNode::FunctionDeclaration { name, parameters, return_type, generic_params, annotations, is_pub, .. } => {
                let mut path = prefix.to_vec();
                path.push(name.lexeme.to_string());

                let param_names: Vec<String> = generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                self.current_generics.extend(param_names.clone());

                let param_types: Vec<Type> = parameters.iter()
                    .map(|(_, ty_node)| self.lower_type(*ty_node.clone()).unwrap_or(Type::VOID))
                    .collect();
                let ret_type = self.lower_type(*return_type.clone()).unwrap_or(Type::VOID);

                self.current_generics.truncate(self.current_generics.len() - param_names.len());

                let info = SymbolInfo {
                    name: name.lexeme.to_string(),
                    span: name.span,
                    absolute_path: path.clone(),
                    kind: DefKind::Function {
                        params: param_types,
                        annotations: annotations.clone(),
                        return_type: ret_type,
                        generic_params: generic_params.iter().map(|t| t.lexeme.to_string()).collect(),
                    },
                    is_pub: *is_pub,
                };

                let def_id = self.context.insert_def(info);
                if let Err(msg) = self.scope.define(name.lexeme.to_string(), def_id) {
                    errors.push(self.error("S018", msg, name.span));
                }

                self.global_symbols.insert(path, def_id);
            }

            ASTNode::StructDeclaration { name, constants, generic_params, is_pub, .. } => {
                let mut struct_path = prefix.to_vec();
                struct_path.push(name.lexeme.to_string());

                let param_names: Vec<String> = generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                self.current_generics.extend(param_names.clone());

                let info = SymbolInfo {
                    name: name.lexeme.to_string(),
                    span: name.span,
                    absolute_path: struct_path.clone(),
                    kind: DefKind::Struct { fields: vec![], generic_params: vec![] },
                    is_pub: *is_pub,
                };

                let def_id = self.context.insert_def(info);
                if let Err(msg) = self.scope.define(name.lexeme.to_string(), def_id) {
                    errors.push(self.error("S018", msg, name.span));
                }
                self.global_symbols.insert(struct_path.clone(), def_id);

                for constant in constants {
                    if let ASTNode::VariableDeclaration { name: c_name, type_annotation, is_pub: c_is_pub, .. } = constant {
                        let mut c_path = struct_path.clone();
                        c_path.push(c_name.lexeme.to_string());

                        let expected_ty = type_annotation.as_ref()
                            .and_then(|ann| self.lower_type(*ann.clone()).ok()).unwrap_or(Type::VOID);

                        let c_info = SymbolInfo {
                            name: c_name.lexeme.to_string(),
                            span: c_name.span,
                            absolute_path: c_path.clone(),
                            kind: DefKind::Constant {
                                ty: expected_ty.clone(),
                                value: ir::Constant::Float(0.0, expected_ty),
                            },
                            is_pub: *c_is_pub,
                        };

                        let c_def_id = self.context.insert_def(c_info);
                        self.global_symbols.insert(c_path, c_def_id);
                    }
                }

                self.current_generics.truncate(self.current_generics.len() - param_names.len());
            }

            ASTNode::VariableDeclaration { name, type_annotation, is_const, is_pub, .. } => {
                let mut path = prefix.to_vec();
                path.push(name.lexeme.to_string());

                let ty = type_annotation.as_ref()
                    .and_then(|ann| self.lower_type(*ann.clone()).ok())
                    .unwrap_or(Type::VOID);

                let kind = if *is_const {
                    DefKind::Constant {
                        ty: ty.clone(),
                        value: ir::Constant::Float(0.0, ty.clone()),
                    }
                } else {
                    DefKind::Variable { ty: ty.clone(), is_mutable: true }
                };

                let info = SymbolInfo {
                    name: name.lexeme.to_string(),
                    span: name.span,
                    absolute_path: path.clone(),
                    kind,
                    is_pub: *is_pub,
                };

                let def_id = self.context.insert_def(info);
                self.global_symbols.insert(path, def_id);
            }

            ASTNode::ExtensionDeclaration { target, methods, constants, generic_params, .. } => {
                let param_names: Vec<String> = generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                self.current_generics.extend(param_names.clone());

                let target_type = self.lower_type(*target.clone()).unwrap_or(Type::VOID);
                let target_path = match &target_type {
                    Type::STRUCT(full_path) => full_path.split("::").map(|s| s.to_string()).collect(),

                    Type::GENERIC_INSTANCE(base, _) => {
                        if let Type::STRUCT(full_path) = &**base {
                            full_path.split("::").map(|s| s.to_string()).collect()
                        } else {
                            prefix.to_vec()
                        }
                    },

                    _ => {
                        let target_name = match &**target {
                            ASTNode::TypeIdentifier { type_token } => type_token.lexeme.to_string(),

                            ASTNode::GenericType { base, .. } => {
                                if let ASTNode::TypeIdentifier { type_token } = &**base {
                                    type_token.lexeme.to_string()
                                } else { String::new() }
                            }

                            _ => String::new(),
                        };

                        let mut p = prefix.to_vec();
                        if !target_name.is_empty() { p.push(target_name); }

                        p
                    }
                };

                let mut lowered_methods = Vec::new();
                for method in methods {
                    if let ASTNode::FunctionDeclaration { name: m_name, parameters, return_type, generic_params: m_generic_params, annotations, is_pub: m_is_pub, .. } = method {
                        let m_param_names: Vec<String> = m_generic_params.iter().map(|t| t.lexeme.to_string()).collect();
                        self.current_generics.extend(m_param_names.clone());

                        let param_types: Vec<Type> = parameters.iter()
                            .map(|(_, ty_node)| self.lower_type(*ty_node.clone()).unwrap_or(Type::VOID))
                            .collect();
                        let ret_type = self.lower_type(*return_type.clone()).unwrap_or(Type::VOID);

                        self.current_generics.truncate(self.current_generics.len() - m_param_names.len());

                        let mut m_path = target_path.clone();
                        m_path.push(m_name.lexeme.to_string());

                        let m_info = SymbolInfo {
                            name: m_name.lexeme.to_string(),
                            span: m_name.span,
                            absolute_path: m_path,
                            kind: DefKind::Function {
                                params: param_types,
                                annotations: annotations.clone(),
                                return_type: ret_type,
                                generic_params: {
                                    let mut gp = param_names.clone(); // extension's <T>
                                    gp.extend(m_param_names.iter().cloned()); // method's own <U> if any
                                    gp
                                },
                            },
                            is_pub: *m_is_pub,
                        };

                        let m_def_id = self.context.insert_def(m_info);
                        lowered_methods.push((m_name.lexeme.to_string(), m_def_id));
                    }
                }

                let struct_name = match &target_type {
                    Type::STRUCT(name) => name.clone(),
                    Type::GENERIC_INSTANCE(base, _) => {
                        if let Type::STRUCT(name) = base.as_ref() { name.clone() }
                        else { String::new() }
                    }
                    _ => String::new(),
                };

                let type_methods = self.impl_registry.entry(struct_name).or_default();

                for (m_name, m_def_id) in lowered_methods {
                    type_methods.insert(m_name, m_def_id);
                }

                if !target_path.is_empty() {
                    for constant in constants {
                        if let ASTNode::VariableDeclaration { name: c_name, type_annotation, is_pub: c_is_pub, .. } = constant {
                            let mut c_path = target_path.clone();
                            c_path.push(c_name.lexeme.to_string());

                            let expected_ty = type_annotation.as_ref()
                                .and_then(|ann| self.lower_type(*ann.clone()).ok()).unwrap_or(Type::VOID);

                            let c_info = SymbolInfo {
                                name: c_name.lexeme.to_string(),
                                span: c_name.span,
                                absolute_path: c_path.clone(),
                                kind: DefKind::Constant {
                                    ty: expected_ty.clone(),
                                    value: ir::Constant::Float(0.0, expected_ty),
                                },
                                is_pub: *c_is_pub,
                            };

                            let c_def_id = self.context.insert_def(c_info);
                            self.global_symbols.insert(c_path, c_def_id);
                        }
                    }
                }

                self.current_generics.truncate(self.current_generics.len() - param_names.len());
            }
            _ => {}
        }
    }
}
