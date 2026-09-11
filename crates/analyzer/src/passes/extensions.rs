use parser::Item;
use ir::types::Type as IRType;

use crate::Analyzer;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn populate_impl_registry(&mut self) {
        for (filepath, (module_path, items)) in &self.program.parsed_files {
            self.current_module = module_path.clone();
            self.current_filepath = filepath.display().to_string();
            self.current_source = self.source_map.get_source(filepath).unwrap_or("").to_string();

            for item in items {
                if let Item::Extension(decl) = item {
                    let target_ty = self.lower_type(&decl.target_type).unwrap_or(IRType::VOID);
                    let registry_key = self.get_impl_registry_key(&target_ty);

                    if registry_key.is_empty() { 
                        continue; 
                    }

                    let type_methods = self.impl_registry.entry(registry_key.clone()).or_default();
                    
                    for method in &decl.methods {
                        if let Some(def_id) = self.name_resolver.get_resolution(method.id) {
                            type_methods.insert(method.name.lexeme.to_string(), def_id);

                            if method.name.lexeme == "drop" {
                                self.context.register_drop_impl(
                                    registry_key.clone(),
                                    def_id,
                                );
                            }
                        }
                    }

                }
            }
        }
    }

    pub(crate) fn get_impl_registry_key(&self, ty: &IRType) -> String {
        match ty {
            IRType::STRUCT(name) => name.symbol.clone(),

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

            //
            // these must remain distinct from both each other
            // and their pointee.
            //
            IRType::POINTER(_) => {
                "ptr_mut".to_string()
            }

            IRType::CONST_POINTER(_) => {
                "ptr_const".to_string()
            }

            IRType::REF(inner) | IRType::CONST_REF(inner) => self.get_impl_registry_key(inner),

            _ => String::new(),
        }
    }
}
