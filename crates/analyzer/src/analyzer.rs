use std::collections::HashMap;

use parser::ast::*;
use parser::program::Program as ASTProgram;
use ir::types::Type as IRType;
use ir::context::{HIRContext, DefID, DefKind};
use ir::hir::{HIRBlock, HIRFunction, HIRProgram};
use errors::error::{HydraError, Span};

use crate::scope::NameResolver;

pub struct Analyzer<'a, 'ctx> {
    pub program: &'a ASTProgram<'a>,
    pub context: &'ctx mut HIRContext,
    pub name_resolver: NameResolver,
    pub global_symbols: HashMap<Vec<String>, DefID>,
    pub impl_registry: HashMap<String, HashMap<String, DefID>>,
    
    pub current_return_type: Option<IRType>,
    pub current_module: Vec<String>,
    pub current_source: String,
    pub errors: Vec<HydraError>,
}

impl<'a, 'ctx> Analyzer<'a, 'ctx> {
    
    pub fn new(
        program: &'a ASTProgram<'a>, 
        context: &'ctx mut HIRContext,
        name_resolver: NameResolver,
        global_symbols: HashMap<Vec<String>, DefID>
    ) -> Self {
        Self {
            program,
            context,
            name_resolver,
            global_symbols,
            impl_registry: HashMap::new(),
            current_return_type: None,
            current_module: Vec::new(),
            current_source: String::new(),
            errors: Vec::new(),
        }
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

    pub fn analyze(mut self) -> Result<HIRProgram, Vec<HydraError>> {
        let mut functions = Vec::new();
        let mut structs = Vec::new();
        let globals = Vec::new(); 

        self.populate_impl_registry();

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        for (module_path, (source, items)) in &self.program.modules {
            self.current_module = module_path.clone();
            self.current_source = source.to_string();

            for item in items {
                match item {
                    Item::Function(decl) => {
                        if let Some(hir_fn) = self.lower_function(decl, None) {
                            functions.push(hir_fn);
                        }
                    }
                    Item::Extension(decl) => {
                        let target_ty = self.lower_type(&decl.target_type).unwrap_or(IRType::VOID);
                        let registry_key = self.get_impl_registry_key(&target_ty);

                        for method in &decl.methods {
                            if let Some(hir_fn) = self.lower_function(method, Some(registry_key.clone())) {
                                functions.push(hir_fn);
                            }
                        }
                    }
                    Item::Struct(_decl) => {
                        // struct fields were registered during the resolve pass.
                        // we can expand this to generate constructor functions if needed later.
                    }
                    _ => {}
                }
            }
        }

        let has_main = functions.iter().any(|f| f.name.ends_with("::main") || f.name == "main");
        if !has_main && self.errors.is_empty() {
            self.errors.push(HydraError::new(
                "S015", 
                "program is missing an entry point",
                Span::default()
            ).with_help("consider adding `fn main() -> void`"));
        }

        if self.errors.is_empty() {
            Ok(HIRProgram { functions, structs, globals })
        } else {
            Err(self.errors)
        }
    }

    fn populate_impl_registry(&mut self) {
    for (module_path, (_, items)) in &self.program.modules {
            self.current_module = module_path.clone();

            for item in items {
                if let Item::Extension(decl) = item {
                    let target_ty = self.lower_type(&decl.target_type).unwrap_or(IRType::VOID);
                    let registry_key = self.get_impl_registry_key(&target_ty);
                    if registry_key.is_empty() { continue; }

                    let type_methods = self.impl_registry.entry(registry_key).or_default();
                    
                    for method in &decl.methods {
                        if let Some(def_id) = self.name_resolver.get_resolution(method.id) {
                            type_methods.insert(method.name.lexeme.to_string(), def_id);
                        }
                    }
                }
            }
        }
    }

    fn lower_function(&mut self, decl: &FunctionDecl<'a>, parent_path: Option<String>) -> Option<HIRFunction> 
    {
        let mut full_path = self.current_module.clone();
        full_path.push(decl.name.lexeme.to_string());

        let def_id = *self.global_symbols.get(&full_path)?;
        let mut info = self.context.get_def(def_id).unwrap().clone();

        let ret_type = if let Some(rt_node) = &decl.return_type {
            self.lower_type(rt_node).unwrap_or(IRType::VOID)
        } else {
            IRType::VOID
        };
        self.current_return_type = Some(ret_type.clone());

        let mut ir_params = Vec::new();
        for (param_token, param_type_node) in &decl.parameters {
            let p_ty = self.lower_type(param_type_node).unwrap_or(IRType::VOID);
            
            // update the parameter's symbol info generated during resolve pass
            if let Some(param_def_id) = self.name_resolver.get_resolution(crate::utils::get_type_id(param_type_node)) {
                let mut p_info = self.context.get_def(param_def_id).unwrap().clone();
                p_info.kind = DefKind::Variable { ty: p_ty.clone(), is_mutable: true };
                self.context.update_def(param_def_id, p_info);
                ir_params.push((param_def_id, p_ty.clone()));
            } else {
                ir_params.push((DefID(0), p_ty.clone()));
            }
        }

        if let DefKind::Function { ref mut return_type, ref mut params, .. } = info.kind {
            *return_type = ret_type.clone();
            *params = ir_params.iter().map(|(_, ty)| ty.clone()).collect();
        }
        self.context.update_def(def_id, info);

        let is_intrinsic = decl.annotations.iter().any(|a| a.name == "intrinsic");
        let is_inline = decl.annotations.iter().any(|a| a.name == "inline");

        let mut ir_body = Vec::new();
        if let Some(body_block) = &decl.body {
            match self.lower_block(body_block) {
                Ok(block) => ir_body = block.stmts,
                Err(e) => self.errors.push(e), 
            }
        }

        self.current_return_type = None;

        let fn_name = if decl.name.lexeme == "main" {
            "main".to_string()
        } else {
            full_path.join("::")
        };

        Some(HIRFunction {
            name: fn_name,
            def_id,
            params: ir_params,
            return_type: ret_type,
            body: HIRBlock { stmts: ir_body, span: decl.name.span },
            is_extern: decl.is_extern,
            is_inline,
            is_intrinsic,
            generic_params: decl.generic_params.iter().map(|p| p.name.lexeme.to_string()).collect(),
        })
    }

    pub(crate) fn get_impl_registry_key(&self, ty: &IRType) -> String {
        match ty {
            IRType::STRUCT(name) => name.clone(),
            IRType::GENERIC_INSTANCE(base, _) => self.get_impl_registry_key(base),
            IRType::SLICE(_) => "slice".to_string(),
            IRType::ARRAY(_, _) | IRType::INFERRED_ARRAY(_) => "array".to_string(),
            IRType::I8 => "i8".to_string(),
            IRType::I16 => "i16".to_string(),
            IRType::I32 => "i32".to_string(),
            IRType::I64 => "i64".to_string(),
            IRType::ISIZE => "isize".to_string(),
            IRType::U8 => "u8".to_string(),
            IRType::U16 => "u16".to_string(),
            IRType::U32 => "u32".to_string(),
            IRType::U64 => "u64".to_string(),
            IRType::USIZE => "usize".to_string(),
            IRType::F32 => "f32".to_string(),
            IRType::F64 => "f64".to_string(),
            IRType::BOOL => "bool".to_string(),
            IRType::CHAR => "char".to_string(),
            IRType::POINTER(inner) => self.get_impl_registry_key(inner),
            IRType::REF(inner) | IRType::CONST_REF(inner) => self.get_impl_registry_key(inner),
            _ => String::new(),
        }
    }
}
