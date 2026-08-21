use std::collections::HashMap;

use parser::ast::*;
use parser::program::Program as ASTProgram;
use ir::context::{HIRContext, DefID, DefKind, SymbolInfo};
use errors::error::{HydraError, Span};

use crate::scope::{NameResolver, Namespace, Scope};

pub struct Resolver<'a, 'ctx> {
    program: &'a ASTProgram<'a>,
    pub context: &'ctx mut HIRContext,
    pub name_resolver: NameResolver,
    
    // global registry for module-level lookups
    pub global_symbols: HashMap<Vec<String>, DefID>,
    
    // scope management
    current_scope: Scope,
    current_module: Vec<String>,
    current_source: String,
    
    pub errors: Vec<HydraError>,
}

impl<'a, 'ctx> Resolver<'a, 'ctx> {

    pub fn new(program: &'a ASTProgram<'a>, context: &'ctx mut HIRContext) -> Self {
        Self {
            program,
            context,
            name_resolver: NameResolver::new(),
            global_symbols: HashMap::new(),
            current_scope: Scope::new(vec![]),
            current_module: vec![],
            current_source: String::new(),
            errors: Vec::new(),
        }
    }

    fn error(&mut self, span: Span, code: &'static str, message: impl Into<String>) {
        let filename = if self.current_module.is_empty() {
            "main.hydra".to_string()
        } else {
            format!("{}.hydra", self.current_module.join("/"))
        };

        self.errors.push(
            HydraError::new(code, message, span).with_file(filename, self.current_source.clone())
        );
    }

    // ========================================================================
    // SCOPE MANAGEMENT
    // ========================================================================

    fn enter_scope(&mut self) {
        let parent = std::mem::replace(&mut self.current_scope, Scope::new(self.current_module.clone()));
        self.current_scope.parent = Some(Box::new(parent));
    }

    fn leave_scope(&mut self) {
        if let Some(parent) = self.current_scope.parent.take() {
            self.current_scope = *parent;
        }
    }

    // ========================================================================
    // RESOLUTION PIPELINE
    // ========================================================================

    pub fn resolve(mut self) -> Result<(NameResolver, HashMap<Vec<String>, DefID>), Vec<HydraError>> {
        // pass 1: populate global scopes (structs, traits, functions)
        self.harvest_globals();

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        // pass 2: walk bodies, resolve usages, and build the side-table
        self.resolve_bodies();

        if !self.errors.is_empty() {
            Err(self.errors)
        } else {
            Ok((self.name_resolver, self.global_symbols))
        }
    }

    // ========================================================================
    // PASS 1: HARVESTING
    // ========================================================================

    fn harvest_globals(&mut self) {
        for (module_path, (source, items)) in &self.program.modules {
            self.current_module = module_path.clone();
            self.current_source = source.to_string();

            for item in items {
                match item {
                    Item::Struct(decl) => {
                        let mut full_path = self.current_module.clone();
                        full_path.push(decl.name.lexeme.to_string());

                        // insert a dummy definition for now. semantic analyzer will fill in the real fields/types.
                        let info = SymbolInfo {
                            name: decl.name.lexeme.to_string(),
                            span: decl.name.span,
                            absolute_path: full_path.clone(),
                            kind: DefKind::Struct { fields: vec![], generic_params: vec![] },
                            is_pub: decl.is_pub,
                        };
                        
                        let def_id = self.context.insert_def(info);
                        self.global_symbols.insert(full_path, def_id);

                        self.name_resolver.record_resolution(decl.id, def_id);
                    }

                    Item::Trait(decl) => {
                        let mut full_path = self.current_module.clone();
                        full_path.push(decl.name.lexeme.to_string());

                        // traits act like structs in the type namespace conceptually for bounds
                        let info = SymbolInfo {
                            name: decl.name.lexeme.to_string(),
                            span: decl.name.span,
                            absolute_path: full_path.clone(),
                            kind: DefKind::Struct { fields: vec![], generic_params: vec![] }, 
                            is_pub: decl.is_pub,
                        };
                        
                        let def_id = self.context.insert_def(info);
                        self.global_symbols.insert(full_path, def_id);
                    }

                    Item::Function(decl) => {
                        let mut full_path = self.current_module.clone();
                        full_path.push(decl.name.lexeme.to_string());

                        let info = SymbolInfo {
                            name: decl.name.lexeme.to_string(),
                            span: decl.name.span,
                            absolute_path: full_path.clone(),
                            kind: DefKind::Function { 
                                params: vec![], 
                                annotations: vec![], 
                                return_type: ir::types::Type::VOID, 
                                generic_params: vec![] 
                            },
                            is_pub: decl.is_pub,
                        };
                        
                        let def_id = self.context.insert_def(info);
                        self.global_symbols.insert(full_path, def_id);
                    }

                    _ => {} // extensions don't introduce top-level names, they attach to existing ones
                }
            }
        }
    }

    // ========================================================================
    // PASS 2: BODY RESOLUTION
    // ========================================================================

    fn resolve_bodies(&mut self) {
        for (module_path, (source, items)) in &self.program.modules {
            self.current_module = module_path.clone();
            self.current_source = source.to_string();
            
            // clear the scope and rebuild it with globals for this module
            self.current_scope = Scope::new(self.current_module.clone());
            
            // inject local module globals into scope
            for (path, &def_id) in &self.global_symbols {
                if path.len() > 0 && path.starts_with(&self.current_module) && path.len() == self.current_module.len() + 1 {
                    let local_name = path.last().unwrap().clone();
                    
                    // functions go in value namespace, types in type namespace
                    let info = self.context.get_def(def_id).unwrap();
                    let namespace = match info.kind {
                        DefKind::Function { .. } | DefKind::Constant { .. } | DefKind::Variable { .. } => Namespace::Value,
                        _ => Namespace::Type,
                    };

                    self.current_scope.define(namespace, local_name, def_id).ok();
                }
            }

            // walk the items
            for item in items {
                self.resolve_item(item);
            }
        }
    }

    fn resolve_item(&mut self, item: &Item<'a>) {
        match item {
            Item::Struct(decl) => {
                self.enter_scope();

                let mut full_path = self.current_module.clone();
                full_path.push(decl.name.lexeme.to_string());
                if let Some(&struct_def_id) = self.global_symbols.get(&full_path) {
                    self.current_scope.define(Namespace::Type, "Self".to_string(), struct_def_id).ok();
                }
                
                // inject generic parameters into the type namespace
                for param in &decl.generic_params {
                    let info = SymbolInfo {
                        name: param.name.lexeme.to_string(),
                        span: param.name.span,
                        absolute_path: vec![param.name.lexeme.to_string()],
                        kind: DefKind::Alias { target_path: vec![] }, // placeholder for generic
                        is_pub: false,
                    };
                    let def_id = self.context.insert_def(info);
                    self.current_scope.define(Namespace::Type, param.name.lexeme.to_string(), def_id).ok();
                    self.name_resolver.record_resolution(param.id, def_id);
                }

                if let Some(wc) = &decl.where_clause {
                    self.resolve_where_clause(wc);
                }

                for (_, ty) in &decl.fields {
                    self.resolve_type(ty);
                }

                for stmt in &decl.constants {
                    self.resolve_stmt(stmt);
                }

                self.leave_scope();
            }

            Item::Trait(decl) => {
                self.enter_scope();
                
                let mut full_path = self.current_module.clone();
                full_path.push(decl.name.lexeme.to_string());
                if let Some(&trait_def_id) = self.global_symbols.get(&full_path) {
                    self.current_scope.define(Namespace::Type, "Self".to_string(), trait_def_id).ok();
                }
                
                for method in &decl.methods {
                    let info = SymbolInfo {
                        name: method.name.lexeme.to_string(),
                        span: method.name.span,
                        absolute_path: vec![], 
                        kind: DefKind::Function { 
                            params: vec![], 
                            annotations: vec![], 
                            return_type: ir::types::Type::VOID, 
                            generic_params: vec![] 
                        },
                        is_pub: method.is_pub,
                    };

                    let m_def_id = self.context.insert_def(info);
                    self.name_resolver.record_resolution(method.id, m_def_id);

                    self.resolve_item(&Item::Function(method.clone()));
                }

                self.leave_scope();
            }

            Item::Function(decl) => {
                self.enter_scope();

                for param in &decl.generic_params {
                    let info = SymbolInfo {
                        name: param.name.lexeme.to_string(),
                        span: param.name.span,
                        absolute_path: vec![param.name.lexeme.to_string()],
                        kind: DefKind::Alias { target_path: vec![] }, 
                        is_pub: false,
                    };
                    let def_id = self.context.insert_def(info);
                    self.current_scope.define(Namespace::Type, param.name.lexeme.to_string(), def_id).ok();
                    self.name_resolver.record_resolution(param.id, def_id);
                }

                for (name_token, ty) in &decl.parameters {
                    self.resolve_type(ty);
                    
                    // add parameters to the value namespace
                    let info = SymbolInfo {
                        name: name_token.lexeme.to_string(),
                        span: name_token.span,
                        absolute_path: vec![name_token.lexeme.to_string()],
                        kind: DefKind::Variable { ty: ir::types::Type::VOID, is_mutable: true },
                        is_pub: false,
                    };
                    let def_id = self.context.insert_def(info);
                    self.current_scope.define(Namespace::Value, name_token.lexeme.to_string(), def_id).ok();
                }

                if let Some(rt) = &decl.return_type {
                    self.resolve_type(rt);
                }

                if let Some(wc) = &decl.where_clause {
                    self.resolve_where_clause(wc);
                }

                if let Some(body) = &decl.body {
                    self.resolve_block(body);
                }

                self.leave_scope();
            }

            Item::Extension(decl) => {
                self.enter_scope();

                for param in &decl.generic_params {
                    let info = SymbolInfo { 
                        name: param.name.lexeme.to_string(), 
                        span: param.name.span, 
                        absolute_path: vec![], 
                        kind: DefKind::Alias { target_path: vec![] }, 
                        is_pub: false 
                    };

                    let def_id = self.context.insert_def(info);
                    self.current_scope.define(Namespace::Type, param.name.lexeme.to_string(), def_id).ok();
                    self.name_resolver.record_resolution(param.id, def_id);
                }

                if let Some(target_trait) = &decl.target_trait {
                    self.resolve_type(target_trait);
                }

                self.resolve_type(&decl.target_type);

                let target_id_node = crate::utils::get_type_id(&decl.target_type);
                if let Some(target_def_id) = self.name_resolver.get_resolution(target_id_node) {
                    self.current_scope.define(Namespace::Type, "Self".to_string(), target_def_id).ok();
                }

                if let Some(wc) = &decl.where_clause { self.resolve_where_clause(wc); }
                for stmt in &decl.constants { self.resolve_stmt(stmt); }
                for method in &decl.methods { 
                    let info = SymbolInfo {
                        name: method.name.lexeme.to_string(),
                        span: method.name.span,
                        absolute_path: vec![], 
                        kind: DefKind::Function { 
                            params: vec![], 
                            annotations: vec![], 
                            return_type: ir::types::Type::VOID, 
                            generic_params: vec![] 
                        },
                        is_pub: method.is_pub,
                    };

                    let m_def_id = self.context.insert_def(info);
                    self.name_resolver.record_resolution(method.id, m_def_id);

                    self.resolve_item(&Item::Function(method.clone()));
                }

                self.leave_scope();
            }

            _ => {}
        }
    }

    // ========================================================================
    // STATEMENTS AND EXPRESSIONS
    // ========================================================================

    fn resolve_block(&mut self, block: &Block<'a>) {
        self.enter_scope();
        for stmt in &block.statements {
            self.resolve_stmt(stmt);
        }
        self.leave_scope();
    }

    fn resolve_stmt(&mut self, stmt: &Stmt<'a>) {
        match stmt {
            Stmt::VariableDecl { id, name, type_annotation, initializer, .. } => {
                if let Some(ty) = type_annotation { 
                   self.resolve_type(ty); 
                }

                self.resolve_expr(initializer);
                let info = SymbolInfo { 
                    name: name.lexeme.to_string(), 
                    span: name.span, 
                    absolute_path: vec![], 
                    kind: DefKind::Variable { 
                        ty: ir::types::Type::VOID, 
                        is_mutable: true }, 
                    is_pub: false 
                };

                let def_id = self.context.insert_def(info);
                self.current_scope.define(Namespace::Value, name.lexeme.to_string(), def_id).ok();

                self.name_resolver.record_resolution(*id, def_id); 
            }

            Stmt::Expr(expr) => self.resolve_expr(expr),
            Stmt::Return { value, .. } => if let Some(v) = value { self.resolve_expr(v); },
            Stmt::Break { condition, .. } | Stmt::Continue { condition, .. } => if let Some(c) = condition { self.resolve_expr(c); },
        }
    }

    fn resolve_expr(&mut self, expr: &Expr<'a>) {
        match expr {
            Expr::Variable { id, name } => {
                if name.lexeme == "print" || name.lexeme == "println" { return; }

                if let Some(def_id) = self.current_scope.resolve(Namespace::Value, name.lexeme, self.context)
                    .or_else(|| self.current_scope.resolve(Namespace::Type, name.lexeme, self.context)) 
                {
                    self.name_resolver.record_resolution(*id, def_id);
                } else {
                    self.error(name.span, "R001", format!("cannot find value `{}` in this scope", name.lexeme));
                }
            }

            Expr::Path { id, segments } => {
                if segments.len() == 1 && (segments[0].lexeme == "print" || segments[0].lexeme == "println") { 
                    return; 
                }

                if let Some(def_id) = self.current_scope.resolve(Namespace::Value, segments[0].lexeme, self.context)
                    .or_else(|| self.current_scope.resolve(Namespace::Type, segments[0].lexeme, self.context))
                {
                    self.name_resolver.record_resolution(*id, def_id);
                } else {
                    self.error(segments[0].span, "R001", format!("cannot find value `{}` in this scope", segments[0].lexeme));
                }
            }

            Expr::FunctionCall { callee, arguments, generic_args, .. } => {
                self.resolve_expr(callee);
                for arg in arguments { self.resolve_expr(arg); }
                for ty in generic_args { self.resolve_type(ty); }
            }

            Expr::MethodCall { object, arguments, generic_args, .. } => {
                self.resolve_expr(object);
                // intentionally DO NOT resolve the method name here. finding methods requires 
                // knowing the type of the object, which is a job for Semantic Analysis.
                for arg in arguments { self.resolve_expr(arg); }
                for ty in generic_args { self.resolve_type(ty); }
            }

            Expr::Member { object, .. } => {
                self.resolve_expr(object);
                // intentionally DO NOT resolve the property name here for the same reason.
            }

            Expr::Binary { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }

            Expr::Unary { right, .. } => self.resolve_expr(right),

            Expr::PostfixUnary { left, .. } => self.resolve_expr(left),

            Expr::Assignment { target, value, .. } => {
                self.resolve_expr(target);
                self.resolve_expr(value);
            }

            Expr::Borrow { right, .. } | Expr::Dereference { right, .. } => self.resolve_expr(right),

            Expr::Cast { value, target, .. } => {
                self.resolve_expr(value);
                self.resolve_type(target);
            }

            Expr::ArrayInitializer { elements, .. } => {
                for el in elements { self.resolve_expr(el); }
            }

            Expr::ArrayAccess { array, index, .. } => {
                self.resolve_expr(array);
                self.resolve_expr(index);
            }

            Expr::StructInitializer { name, fields, .. } => {
                self.resolve_expr(name);
                for (_, expr) in fields {
                    self.resolve_expr(expr);
                }
            }

            Expr::If { condition, then_branch, else_branch, .. } => {
                self.resolve_expr(condition);
                self.resolve_block(then_branch);
                if let Some(eb) = else_branch {
                    self.resolve_block(eb);
                }
            }

            Expr::While { condition, body, .. } => {
                self.resolve_expr(condition);
                self.resolve_block(body);
            }

            Expr::For { id, variable, start, end, body, .. } => {
                self.resolve_expr(start);
                self.resolve_expr(end);

                self.enter_scope();
                let info = SymbolInfo { 
                    name: variable.lexeme.to_string(), 
                    span: variable.span, 
                    absolute_path: vec![], 
                    kind: DefKind::Variable { 
                        ty: ir::types::Type::VOID, 
                        is_mutable: false }, 
                    is_pub: false 
                };

                let def_id = self.context.insert_def(info);
                self.current_scope.define(Namespace::Value, variable.lexeme.to_string(), def_id).ok();

                self.name_resolver.record_resolution(*id, def_id);

                self.resolve_block(body);
                self.leave_scope();
            }

            Expr::ForEach { id, item, iterable, body } => {
                self.resolve_expr(iterable);
                self.enter_scope();

                let item_info = SymbolInfo { 
                    name: item.lexeme.to_string(), 
                    span: item.span, 
                    absolute_path: vec![], 
                    kind: DefKind::Variable { 
                        ty: ir::types::Type::VOID, 
                        is_mutable: false 
                    }, 
                    is_pub: false 
                };

                let item_def = self.context.insert_def(item_info);
                self.current_scope.define(Namespace::Value, item.lexeme.to_string(), item_def).ok();

                self.name_resolver.record_resolution(*id, item_def);

                self.resolve_block(body);
                self.leave_scope();
            }

            _ => {} // literal doesn't need resolution
        }
    }

    fn resolve_type(&mut self, ty: &Type<'a>) {
        match ty {
            Type::Path { id, segments } => {
                let name = segments[0].lexeme;
                if segments.len() == 1 && matches!(
                    name, 
                    "i8" | "i16" | "i32" | "i64" | "isize" | 
                    "u8" | "u16" | "u32" | "u64" | "usize" | 
                    "f32" | "f64" | "char" | "bool" | "void") 
                {
                    return; 
                }

                if let Some(def_id) = self.current_scope.resolve(Namespace::Type, segments[0].lexeme, self.context) {
                    self.name_resolver.record_resolution(*id, def_id);
                } else {
                    self.error(segments[0].span, "R002", format!("cannot find type `{}` in this scope", segments[0].lexeme));
                }
            }
            Type::Borrow { inner, .. } | Type::Slice { element_type: inner, .. } | Type::RawPointer { inner, .. } => self.resolve_type(inner),
            Type::Generic { base, args, .. } => { self.resolve_type(base); for arg in args { self.resolve_type(arg); } }
            Type::Array { element_type, size, .. } => { self.resolve_type(element_type); self.resolve_expr(size); }
        }
    }

    fn resolve_where_clause(&mut self, wc: &WhereClause<'a>) {
        for pred in &wc.predicates {
            self.resolve_type(&pred.target_type);
            for bound in &pred.bound_traits {
                self.resolve_type(bound);
            }
        }
    }
}
