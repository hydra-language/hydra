use std::collections::HashMap;

use errors::error::HydraError;
use ir::{
    context::{DefID, DefKind, TraitImpl}, items::DROP_TRAIT_PATH, types::Type as IRType,
};
use parser::{ast::Type as ASTType, ExtensionDecl, Item};

use crate::Analyzer;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn populate_impl_registry(&mut self) {
        let files: Vec<_> = self.program.parsed_files.iter().map(|(fpath, (modpath, items))| {
            (fpath.clone(), modpath.clone(), items.clone())
        }).collect();

        for (filepath, module_path, items) in files {
            self.current_module = module_path;
            self.current_filepath = filepath.display().to_string();
            self.current_source = self.source_map.get_source(&filepath).unwrap_or("").to_string();

            for item in &items {
                let Item::Extension(decl) = item else {
                    continue;
                };

                let target_ty = match self.lower_type(&decl.target_type) {
                    Ok(ty) => ty,

                    Err(error) => {
                        self.errors.push(error);
                        continue;
                    }
                };

                if decl.target_trait.is_none() {
                    for method in &decl.methods {
                        if method.name.lexeme == "drop" {
                            self.errors.push(self.error(
                                "S026",
                                "`drop` is reserved for the `Drop` trait",
                                method.name.span,
                            ).with_help("implement `Drop` for this type instead")
                            .with_help("see `core::ops::drop` for trait signature"));
                        }
                    }
                }

                // trait extensions must NOT enter the inherent-method registry
                if let Some(target_trait) = &decl.target_trait {
                    let trait_def_id = match self.resolve_trait_def_id(target_trait) {
                        Ok(def_id) => def_id,

                        Err(error) => {
                            self.errors.push(error);
                            continue;
                        }
                    };

                    self.populate_trait_impl(decl, trait_def_id, target_ty);

                    continue;
                }

                // everything below here is only for inherent extensions
                let registry_key = self.get_impl_registry_key(&target_ty);

                if registry_key.is_empty() {
                    continue;
                }

                let type_methods = self.impl_registry.entry(registry_key.clone()).or_default();

                for method in &decl.methods {
                    if let Some(def_id) = self.name_resolver.get_resolution(method.id) {
                        type_methods.insert(method.name.lexeme.clone(), def_id);
                    }
                }
            }
        }
    }

    fn populate_trait_impl(&mut self, decl: &ExtensionDecl, trait_def_id: DefID, target_ty: IRType) {
        let trait_info = self.context.get_def(trait_def_id).cloned().unwrap();

        let trait_name = if trait_info.absolute_path.is_empty() {
            trait_info.name.clone()
        } else {
            trait_info.absolute_path.join("::")
        };

        if self.context.get_exact_trait_impl(trait_def_id, &target_ty).is_some() {
            self.errors.push(self.error(
                "S023",
                format!("duplicate implementation of trait `{trait_name}` for `{target_ty}`"),
                crate::utils::get_type_span(&decl.target_type),
            ));

            return;
        }

        let DefKind::Trait { methods: trait_method_ids, } = trait_info.kind else {
            return;
        };

        let mut implementation_methods = HashMap::new();

        for method in &decl.methods {
            if let Some(def_id) = self.name_resolver.get_resolution(method.id) {
                implementation_methods.insert(method.name.lexeme.clone(), def_id);
            }
        }

        let error_count = self.errors.len();
        let mut registered_methods = HashMap::new();

        for trait_method_id in trait_method_ids {
            let Some(trait_method) = self.context.get_def(trait_method_id).cloned() else {
                continue;
            };

            let Some(&impl_method_id) = implementation_methods.get(&trait_method.name) else {
                self.errors.push(
                    self.error(
                        "S021",
                        format!(
                            "implementation of trait `{trait_name}` for `{target_ty}` is missing method `{}`",
                            trait_method.name,
                        ),
                        crate::utils::get_type_span(&decl.target_type)
                    ).with_note(format!("`{}` is required by this trait", trait_method.name), trait_method.span)
                );

                continue;
            };

            self.validate_trait_method_signature(&trait_name, trait_method_id, impl_method_id, &target_ty);

            registered_methods.insert(trait_method.name, impl_method_id);
        }

        if self.errors.len() != error_count {
            return;
        }

        self.context.register_trait_impl(TraitImpl {
            trait_def_id,
            target_type: target_ty,
            methods: registered_methods,
        });
    }

    fn validate_trait_method_signature(&mut self, trait_name: &str, trait_method_id: DefID, impl_method_id: DefID, target_ty: &IRType) 
    {
        let trait_method = self.context.get_def(trait_method_id).cloned().unwrap();
        let impl_method = self.context.get_def(impl_method_id).cloned().unwrap();

        let DefKind::Function {
            params: trait_params,
            return_type: trait_return,
            generic_params: trait_generics,
            owner_generic_count: trait_owner_generic_count,
            ..
        } = &trait_method.kind
        else {
            return;
        };

        let DefKind::Function {
            params: impl_params,
            return_type: impl_return,
            generic_params: impl_generics,
            owner_generic_count: impl_owner_generic_count,
            ..
        } = &impl_method.kind
        else {
            return;
        };

        let trait_method_generics = &trait_generics[*trait_owner_generic_count..];
        let impl_method_generics = &impl_generics[*impl_owner_generic_count..];

        if trait_method_generics.len() != impl_method_generics.len() {
            self.errors.push(
                self.error(
                    "S022",
                    format!(
                        "method `{}::{}` has an incompatible generic parameter count",
                        trait_name,
                        trait_method.name,
                    ),
                    impl_method.span,
                ).with_note("trait method declared here", trait_method.span)
            );

            return;
        }

        let method_generic_substitutions: HashMap<String, String> = trait_method_generics
            .iter()
            .cloned()
            .zip(impl_method_generics.iter().cloned())
            .collect();

        let expected_params: Vec<IRType> = trait_params
            .iter()
            .map(|ty| {
                Self::substitute_trait_contract_type(
                    ty,
                    target_ty,
                    &method_generic_substitutions,
                )
            })
            .collect();

        let expected_return = Self::substitute_trait_contract_type(
            trait_return,
            target_ty,
            &method_generic_substitutions,
        );

        if expected_params == *impl_params && expected_return == *impl_return {
            return;
        }

        let expected_signature = Self::format_trait_signature(
            &expected_params,
            &expected_return,
        );

        let found_signature = Self::format_trait_signature(
            impl_params,
            impl_return,
        );

        self.errors.push(
            self.error(
                "S022",
                format!("method `{}::{}` has an incompatible signature",
                    trait_name,
                    trait_method.name,
                ),
                impl_method.span,
            ).with_note("trait method declared here", trait_method.span).with_help(
                format!("expected `{expected_signature}`, found `{found_signature}`"
            )),
        );
    }

    pub fn is_drop_trait(&self, trait_def_id: DefID) -> bool {
        self.context.get_def(trait_def_id).is_some_and(|info| info.absolute_path.join("::") == DROP_TRAIT_PATH )
    }

    fn substitute_trait_contract_type(ty: &IRType, self_ty: &IRType, method_generics: &HashMap<String, String>) -> IRType {
        match ty {
            IRType::GENERIC(name) if name == "Self" => self_ty.clone(),

            IRType::GENERIC(name) => {
                if let Some(mapped) = method_generics.get(name) {
                    IRType::GENERIC(mapped.clone())
                } else {
                    ty.clone()
                }
            }

            IRType::ARRAY(inner, len) => IRType::ARRAY(
                Box::new(Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    method_generics,
                )),
                *len,
            ),

            IRType::INFERRED_ARRAY(inner) => IRType::INFERRED_ARRAY(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    method_generics,
                ),
            )),

            IRType::SLICE(inner) => IRType::SLICE(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    method_generics,
                ),
            )),

            IRType::POINTER(inner) => IRType::POINTER(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    method_generics,
                ),
            )),

            IRType::CONST_POINTER(inner) => IRType::CONST_POINTER(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    method_generics,
                ),
            )),

            IRType::REF(inner) => IRType::REF(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    method_generics,
                ),
            )),

            IRType::CONST_REF(inner) => IRType::CONST_REF(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    method_generics,
                ),
            )),

            IRType::GENERIC_INSTANCE(base, args) => IRType::GENERIC_INSTANCE(
                base.clone(),
                args.iter()
                    .map(|arg| {
                        Self::substitute_trait_contract_type(
                            arg,
                            self_ty,
                            method_generics,
                        )
                    })
                    .collect(),
            ),

            _ => ty.clone(),
        }
    }

    fn format_trait_signature(params: &[IRType], return_type: &IRType) -> String {
        let params = params
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");

        format!("fn({params}) -> {return_type}")
    }

    pub(crate) fn resolve_trait_def_id(&self, trait_ty: &ASTType) -> Result<DefID, HydraError> {
        let span = crate::utils::get_type_span(trait_ty);

        let ASTType::Path { id, .. } = trait_ty else {
            return Err(self.error(
                "S020",
                "expected a trait name after `extension`",
                span,
            ));
        };

        let Some(def_id) = self.name_resolver.get_resolution(*id) else {
            return Err(self.error(
                "S002",
                "missing definition for trait implementation",
                span
            ));
        };

        let Some(info) = self.context.get_def(def_id) else {
            return Err(self.error(
                "S020",
                "missing definition for trait implementation",
                span,
            ));
        };

        if !matches!(info.kind, DefKind::Trait { .. }) {
            return Err(self.error(
                "S020",
                format!("`{}` is not a trait", info.name),
                span,
            ));
        };

        Ok(def_id)
    }

    pub(crate) fn extension_parent_path(&self, target_ty: &IRType, target_trait: Option<&ASTType>) -> Result<String, HydraError> {
        let target_name = match target_ty {
            IRType::GENERIC(name) => name.clone(),
            _ => self.get_impl_registry_key(target_ty),
        };

        if target_name.is_empty() {
            return Ok(String::new());
        }

        let Some(trait_ty) = target_trait else {
            return Ok(target_name);
        };

        let trait_def_id = self.resolve_trait_def_id(trait_ty)?;
        let trait_info = self.context.get_def(trait_def_id).unwrap();

        let trait_name = if trait_info.absolute_path.is_empty() {
            trait_info.name.clone()
        } else {
            trait_info.absolute_path.join("::")
        };

        Ok(format!("{target_name}::<{trait_name}>"))
    }

    pub(crate) fn get_impl_registry_key(&self, ty: &IRType) -> String {
        match ty {
            IRType::STRUCT(name) => name.symbol.clone(),

            IRType::GENERIC_INSTANCE(base, _) => base.symbol.clone(),

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
