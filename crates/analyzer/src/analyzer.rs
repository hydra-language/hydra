use std::collections::HashMap;

use parser::ast::*;
use parser::module::{ModuleTree, SourceMap};
use ir::types::Type as IRType;
use ir::context::{HIRContext, DefID, DefKind, SymbolInfo};
use ir::hir::HIRProgram;
use errors::error::{HydraError, Span};

use crate::scope::NameResolver;

pub struct Analyzer<'ctx> {
    pub program: &'ctx ModuleTree,
    pub context: &'ctx mut HIRContext,
    pub source_map: &'ctx SourceMap,
    pub name_resolver: NameResolver,
    pub global_symbols: HashMap<Vec<String>, DefID>,
    pub impl_registry: HashMap<String, HashMap<String, DefID>>,
    
    pub current_return_type: Option<IRType>,
    pub current_self_type: Option<IRType>,
    pub current_filepath: String,
    pub current_module: Vec<String>,
    pub current_source: String,
    pub errors: Vec<HydraError>,
}

impl<'ctx> Analyzer<'ctx> {
    
    pub fn new(
        program: &'ctx ModuleTree, 
        context: &'ctx mut HIRContext,
        source_map: &'ctx SourceMap,
        name_resolver: NameResolver,
        global_symbols: HashMap<Vec<String>, DefID>
    ) -> Self 
    {
        Self {
            program,
            context,
            source_map,
            name_resolver,
            global_symbols,
            impl_registry: HashMap::new(),
            current_return_type: None,
            current_self_type: None,
            current_filepath: String::new(),
            current_module: Vec::new(),
            current_source: String::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn error(&self, code: &'static str, message: impl Into<String>, span: Span) -> HydraError {
        let filename = if self.current_filepath.is_empty() {
            "<unknown>.hydra".to_string()
        } else {
            self.current_filepath.clone()
        };

        HydraError::new(code, message, span)
            .with_file(filename, self.current_source.clone())
    }

    pub fn analyze(mut self) -> Result<HIRProgram, Vec<HydraError>> {
        let mut functions = Vec::new();
        let mut structs = Vec::new();
        let globals = Vec::new(); 

        self.populate_struct_definitions();

        self.populate_function_signatures();

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        self.populate_impl_registry();

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        for (filepath, (module_path, items)) in &self.program.parsed_files {
            self.current_module = module_path.clone();
            self.current_filepath = filepath.display().to_string();

            self.current_source = self
                .source_map
                .get_source(filepath)
                .unwrap_or("")
                .to_string();

            for item in items {
                match item {
                    Item::Function(decl) => {
                        let Some(def_id) = self.name_resolver.get_resolution(decl.id)
                            .or_else(|| {
                                let mut path = self.current_module.clone();
                                path.push(decl.name.lexeme.clone());
                                self.global_symbols.get(&path).copied()
                            })
                        else {
                            continue;
                        };

                        let is_intrinsic = matches!(
                            self.context.get_def(def_id).map(|info| &info.kind),
                            Some(DefKind::Function {
                                intrinsic: Some(_),
                                ..
                            })
                        );

                        if is_intrinsic {
                            continue;
                        }

                        if let Some(hir_fn) = self.lower_function(decl, None, &[]) {
                            functions.push(hir_fn);
                        }
                    }

                    Item::Extension(decl) => {
                        let target_ty =
                        self.lower_type(&decl.target_type)
                            .unwrap_or(IRType::VOID);

                        let previous_self_type =
                        self.current_self_type.replace(target_ty.clone());

                        let registry_key =
                        self.get_impl_registry_key(&target_ty);

                        let extension_generic_params: Vec<String> = decl
                            .generic_params
                            .iter()
                            .map(|p| p.name.lexeme.clone())
                            .collect();

                        for method in &decl.methods {
                            if let Some(hir_fn) = self.lower_function(method, Some(registry_key.clone()), &extension_generic_params) 
                            {
                                functions.push(hir_fn);
                            }
                        }

                        self.current_self_type = previous_self_type;
                    }

                    Item::Struct(_) => {}

                    _ => {}
                }
            }
        }

        let has_main = functions.iter().any(|f| f.name == "main");
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

    pub(crate) fn can_access_struct_field(&self, struct_info: &SymbolInfo, is_pub: bool) -> bool {
        if is_pub {
            return true;
        }

        let Some((_, defining_module)) = struct_info.absolute_path.split_last() else {
            return self.current_module.is_empty();
        };

        defining_module == self.current_module.as_slice()
    }
}
