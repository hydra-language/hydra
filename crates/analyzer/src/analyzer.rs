use std::collections::HashMap;

use lexer::NameID;
use parser::ast::*;
use parser::module::{ModuleTree, SourceMap};

use ir::types::Type as IRType;
use ir::context::{
    DefID, DefKind, SemanticContext, SymbolInfo, TraitBound,
};
use ir::hir::{HIRFunctionRef, HIRLocal, HIRProgram};

use errors::error::{HydraError, Span};

use crate::scope::NameResolver;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ImplRegistryKey {
    Nominal(DefID),
    Slice,
    Array,

    I8, I16, I32, I64, Isize,
    U8, U16, U32, U64, Usize,
    F32, F64,

    Bool,
    Char,
    String,

    MutPtr,
    ConstPtr,
}

impl ImplRegistryKey {

    pub(crate) fn of(ty: &IRType) -> Option<Self> {
        match ty {
            IRType::STRUCT(type_ref) |
            IRType::GENERIC_INSTANCE(type_ref, _) => Some(Self::Nominal(type_ref.def_id)),

            IRType::SLICE(_) => Some(Self::Slice),
            IRType::ARRAY(_, _) | IRType::INFERRED_ARRAY(_) => Some(Self::Array),

            IRType::I8 => Some(Self::I8),
            IRType::I16 => Some(Self::I16),
            IRType::I32 => Some(Self::I32),
            IRType::I64 => Some(Self::I64),
            IRType::ISIZE => Some(Self::Isize),

            IRType::U8 => Some(Self::U8),
            IRType::U16 => Some(Self::U16),
            IRType::U32 => Some(Self::U32),
            IRType::U64 => Some(Self::U64),
            IRType::USIZE => Some(Self::Usize),

            IRType::F32 => Some(Self::F32),
            IRType::F64 => Some(Self::F64),

            IRType::BOOL => Some(Self::Bool),
            IRType::CHAR => Some(Self::Char),
            IRType::STRING => Some(Self::String),

            IRType::POINTER(_) => Some(Self::MutPtr),
            IRType::CONST_POINTER(_) => Some(Self::ConstPtr),

            IRType::REF(inner) |
            IRType::CONST_REF(inner) => Self::of(inner),

            _ => None,
        }
    }
}

pub struct AnalyzedProgram {
    pub hir: HIRProgram,
    pub context: SemanticContext,
}

pub struct Analyzer<'ctx> {
    pub program: &'ctx ModuleTree,
    pub context: SemanticContext,
    pub source_map: &'ctx SourceMap,
    pub name_resolver: NameResolver,
    pub global_symbols: HashMap<Vec<String>, DefID>,

    pub(crate) impl_registry: HashMap<ImplRegistryKey, HashMap<NameID, DefID>>,
    pub(crate) trait_methods_by_name: HashMap<(DefID, NameID), DefID>,

    pub current_return_type: Option<IRType>,
    pub current_self_type: Option<IRType>,
    pub current_filepath: String,
    pub current_module: Vec<String>,
    pub current_source: String,
    pub current_trait_bounds: Vec<TraitBound>,

    pub errors: Vec<HydraError>,
}

impl<'ctx> Analyzer<'ctx> {
    
    pub fn new(
        program: &'ctx ModuleTree,
        context: SemanticContext,
        source_map: &'ctx SourceMap,
        name_resolver: NameResolver,
        global_symbols: HashMap<Vec<String>, DefID>,
    ) -> Self 
    {
        Self {
            program,
            context,
            source_map,
            name_resolver,
            global_symbols,
            impl_registry: HashMap::new(),
            trait_methods_by_name: HashMap::new(),
            current_return_type: None,
            current_self_type: None,
            current_filepath: String::new(),
            current_module: Vec::new(),
            current_source: String::new(),
            current_trait_bounds: Vec::new(),
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

    pub fn analyze(mut self) -> Result<AnalyzedProgram, Vec<HydraError>> {
        let mut functions = Vec::new();
        let structs = Vec::new();
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

        self.validate_builtin_trait_impls();

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
                        let target_ty = self.lower_type(&decl.target_type).unwrap_or(IRType::VOID);
                        let previous_self_type = self.current_self_type.replace(target_ty.clone());

                        let parent_path = match self.extension_parent_path(&target_ty, decl.target_trait.as_ref()) 
                        {
                            Ok(path) => path,

                            Err(error) => {
                                self.errors.push(error);
                                self.current_self_type = previous_self_type;
                                continue;
                            }
                        };

                        let extension_generic_params: Vec<String> = decl
                            .generic_params
                            .iter()
                            .map(|p| p.name.lexeme.clone())
                            .collect();

                        for method in &decl.methods {
                            if let Some(hir_fn) = self.lower_function(method, Some(parent_path.clone()), &extension_generic_params) 
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
            Ok(AnalyzedProgram {
                hir: HIRProgram { 
                    functions, 
                    structs, 
                    globals
                },
                context: self.context,
            })
        } else {
            Err(self.errors)
        }
    }

    pub(crate) fn trait_method_id(&self, trait_def_id: DefID, name_id: NameID) -> Option<DefID> {
        self.trait_methods_by_name.get(&(trait_def_id, name_id)).copied()
    }

    pub(crate) fn hir_local(&self, def_id: DefID, ty: IRType) -> HIRLocal {
        let info = self.context.get_def(def_id).unwrap_or_else(|| {
            panic!("ICE: missing definition for HIR local {}", def_id)
        });

        let is_mutable = match &info.kind {
            DefKind::Variable { is_mutable, .. } => *is_mutable,
            DefKind::Constant { .. } => false,
            _ => false,
        };

        HIRLocal {
            def_id,
            name: info.name.clone(),
            ty,
            is_mutable,
            span: info.span,
        }
    }

    pub(crate) fn hir_function_ref(&self, def_id: DefID) -> HIRFunctionRef {
        let symbol = self.context.get_symbol_name(def_id).unwrap_or_else(|| {
            panic!("ICE: missing backend symbol for {}", def_id)
        });

        HIRFunctionRef {
            def_id,
            symbol: symbol.to_string(),
            display_symbol: self.context.get_display_name(def_id).map(str::to_string),
        }
    }

    pub(crate) fn hir_intrinsic_ref(&self, def_id: DefID) -> HIRFunctionRef {
        let info = self.context.get_def(def_id).unwrap_or_else(|| {
            panic!("ICE: missing intrinsic definition for {}", def_id)
        });

        let display_symbol = self.context.get_display_name(def_id).map(str::to_string).or_else(|| {
            (!info.absolute_path.is_empty()).then(|| info.absolute_path.join("::"))
        });

        HIRFunctionRef {
            def_id,
            symbol: info.name.clone(),
            display_symbol,
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

#[cfg(test)]
mod tests {
    use super::ImplRegistryKey;
    use ir::{context::DefID, types::{Type, TypeRef}};

    #[test]
    fn impl_registry_key_uses_nominal_def_id_not_symbol() {
        let first = Type::STRUCT(TypeRef::new(DefID(7), "first::Thing".into()));
        let renamed = Type::GENERIC_INSTANCE(
            TypeRef::new(DefID(7), "renamed::Thing".into()),
            vec![Type::I32],
        );
        let different = Type::STRUCT(TypeRef::new(DefID(8), "first::Thing".into()));

        assert_eq!(ImplRegistryKey::of(&first), Some(ImplRegistryKey::Nominal(DefID(7))));
        assert_eq!(ImplRegistryKey::of(&renamed), Some(ImplRegistryKey::Nominal(DefID(7))));
        assert_ne!(ImplRegistryKey::of(&first), ImplRegistryKey::of(&different));
    }

    #[test]
    fn impl_registry_key_preserves_receiver_classes() {
        assert_eq!(ImplRegistryKey::of(&Type::ARRAY(Box::new(Type::I32), 4)), Some(ImplRegistryKey::Array));
        assert_eq!(ImplRegistryKey::of(&Type::ARRAY(Box::new(Type::U8), 32)), Some(ImplRegistryKey::Array));
        assert_eq!(ImplRegistryKey::of(&Type::REF(Box::new(Type::I32))), Some(ImplRegistryKey::I32));
        assert_eq!(ImplRegistryKey::of(&Type::CONST_REF(Box::new(Type::I32))), Some(ImplRegistryKey::I32));
        assert_ne!(
            ImplRegistryKey::of(&Type::POINTER(Box::new(Type::I32))),
            ImplRegistryKey::of(&Type::CONST_POINTER(Box::new(Type::I32))),
        );
        assert_eq!(ImplRegistryKey::of(&Type::OPTIONAL(Box::new(Type::I32))), None);
    }
}
