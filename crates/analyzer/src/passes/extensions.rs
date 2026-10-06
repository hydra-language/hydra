use std::collections::HashMap;

use errors::error::HydraError;
use ir::{
    context::{DefID, DefKind, TraitBound, TraitImpl}, types::Type as IRType,
};
use crate::{traits, utils};
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
                    let (trait_def_id, trait_args) = match self.resolve_trait_application(target_trait) {
                        Ok(application) => application,

                        Err(error) => {
                            self.errors.push(error);
                            continue;
                        }
                    };

                    let previous_self_type =
                    self.current_self_type.replace(target_ty.clone());

                    let bounds = match self.lower_where_clause_bounds(
                        decl.where_clause.as_ref(),
                    ) {
                        Ok(bounds) => bounds,

                        Err(error) => {
                            self.errors.push(error);
                            self.current_self_type = previous_self_type;
                            continue;
                        }
                    };

                    self.current_self_type = previous_self_type;

                    self.populate_trait_impl(
                        decl,
                        trait_def_id,
                        trait_args,
                        target_ty,
                        bounds,
                    );

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

    fn populate_trait_impl(&mut self, decl: &ExtensionDecl, trait_def_id: DefID, trait_args: Vec<IRType>, target_ty: IRType, bounds: Vec<TraitBound>) 
    {
        let trait_info = self.context.get_def(trait_def_id).cloned().unwrap();

        let trait_name = if trait_info.absolute_path.is_empty() {
            trait_info.name.clone()
        } else {
            trait_info.absolute_path.join("::")
        };

        if self.context.get_exact_trait_impl_with_args(trait_def_id, &trait_args, &target_ty).is_some() {
            self.errors.push(self.error(
                "S023",
                format!("duplicate implementation of trait `{trait_name}` for `{target_ty}`"), 
                utils::get_type_span(&decl.target_type)
            ));

            return;
        }

        let DefKind::Trait { methods: trait_method_ids, generic_params: trait_generic_params, } = trait_info.kind else {
            return;
        };

        let trait_generic_substitutions: HashMap<String, IRType> = trait_generic_params.iter()
            .cloned()
            .zip(trait_args.iter().cloned())
            .collect();

        let mut implementation_methods = HashMap::new();

        for method in &decl.methods {
            if let Some(def_id) = self.name_resolver.get_resolution(method.id) {
                implementation_methods.insert(
                    method.name.lexeme.clone(),
                    def_id,
                );
            }
        }

        let error_count = self.errors.len();
        let mut registered_methods = HashMap::new();

        for trait_method_id in trait_method_ids {
            let Some(trait_method) = self.context.get_def(trait_method_id).cloned() else {
                continue;
            };

            let Some(&impl_method_id) = implementation_methods.get(&trait_method.name) else {
                self.errors.push(self.error(
                    "S021",
                    format!(
                        "implementation of trait `{trait_name}` for `{target_ty}` is missing method `{}`",
                        trait_method.name,
                    ),
                    utils::get_type_span(&decl.target_type),
                ).with_note(format!("`{}` is required by this trait", trait_method.name), trait_method.span));

                continue;
            };

            self.validate_trait_method_signature(
                &trait_name,
                trait_method_id,
                impl_method_id,
                &target_ty,
                &trait_generic_substitutions,
            );

            registered_methods.insert(
                trait_method.name,
                impl_method_id,
            );
        }

        if self.errors.len() != error_count {
            return;
        }

        self.context.register_trait_impl(TraitImpl {
            trait_def_id,
            trait_args,
            target_type: target_ty,
            bounds,
            methods: registered_methods,
        });
    }

    fn validate_trait_method_signature(
        &mut self,
        trait_name: &str,
        trait_method_id: DefID,
        impl_method_id: DefID,
        target_ty: &IRType,
        trait_generic_substitutions: &HashMap<String, IRType>,
    ) {
        let trait_method = self.context.get_def(trait_method_id).cloned().unwrap();
        let impl_method = self.context.get_def(impl_method_id).cloned().unwrap();

        let DefKind::Function {
            params: trait_params,
            return_type: trait_return,
            generic_params: trait_function_generics,
            owner_generic_count: trait_owner_generic_count,
            ..
        } = &trait_method.kind else 
        {
            return;
        };

        let DefKind::Function {
            params: impl_params,
            return_type: impl_return,
            generic_params: impl_function_generics,
            owner_generic_count: impl_owner_generic_count,
            ..
        } = &impl_method.kind else 
        {
            return;
        };

        let trait_method_generics = &trait_function_generics[*trait_owner_generic_count..];
        let impl_method_generics = &impl_function_generics[*impl_owner_generic_count..];

        if trait_method_generics.len() != impl_method_generics.len() {
            self.errors.push(self.error(
                "S022",
                format!(
                    "method `{}::{}` has an incompatible generic parameter count",
                    trait_name,
                    trait_method.name,
                ),
                impl_method.span
            ).with_note("trait method declared here", trait_method.span));

            return;
        }

        let method_generic_substitutions: HashMap<String, String> = trait_method_generics.iter()
            .cloned()
            .zip(impl_method_generics.iter().cloned())
            .collect();

        let expected_params: Vec<IRType> = trait_params.iter().map(|ty| {
            Self::substitute_trait_contract_type(
                ty,
                target_ty,
                trait_generic_substitutions,
                &method_generic_substitutions,
            )
        }).collect();

        let expected_return = Self::substitute_trait_contract_type(
            trait_return,
            target_ty,
            trait_generic_substitutions,
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

        self.errors.push(self.error(
            "S022",
            format!(
                "method `{}::{}` has an incompatible signature",
                trait_name,
                trait_method.name,
            ),
            impl_method.span
        ).with_note("trait method declared here", trait_method.span)
            .with_help(format!("expected `{expected_signature}`, found `{found_signature}`"))
        );
    }

    pub(crate) fn validate_builtin_trait_impls(&mut self) {
        let Some(copy_trait_def_id) = self.context.find_trait_by_path(traits::COPY_TRAIT_PATH) else {
            return;
        };

        let drop_trait_def_id = self.context.find_trait_by_path(traits::DROP_TRAIT_PATH);

        let files: Vec<_> = self.program.parsed_files.iter().map(|(filepath, (module_path, items))| {
                (filepath.clone(), module_path.clone(), items.clone()) 
        }).collect();

        for (filepath, module_path, items) in files {
            self.current_module = module_path;
            self.current_filepath = filepath.display().to_string();
            self.current_source = self.source_map.get_source(&filepath).unwrap_or("").to_string();

            for item in &items {
                let Item::Extension(decl) = item else {
                    continue;
                };

                let Some(target_trait) = &decl.target_trait else {
                    continue;
                };

                let Ok(trait_def_id) = self.resolve_trait_def_id(target_trait) else {
                    continue;
                };

                if trait_def_id != copy_trait_def_id {
                    continue;
                }

                let Ok(target_ty) = self.lower_type(&decl.target_type) else {
                    continue;
                };

                let span = crate::utils::get_type_span(&decl.target_type);

                if let Some(drop_trait_def_id) = drop_trait_def_id {
                    let implements_drop = self.context.trait_impls.iter().any(|trait_impl| {
                        trait_impl.trait_def_id == drop_trait_def_id && 
                        Self::trait_impl_targets_overlap(&target_ty, &trait_impl.target_type)
                    });

                    if implements_drop {
                        self.errors.push(self.error(
                            "S025",
                            format!(
                                "type `{}` cannot implement both `Copy` and `Drop`",
                                target_ty,
                            ),
                            span,
                        ));

                        continue;
                    }
                }

                let Some(fields) = self.copy_target_fields(&target_ty) else {
                    continue;
                };

                for (field_name, field_ty) in fields {
                    if self.type_is_copy_compatible(&field_ty, copy_trait_def_id) {
                        continue;
                    }

                    self.errors.push(self.error(
                        "S025",
                        format!(
                            "type `{}` cannot implement `Copy` because field `{}` has type `{}`, which is not `Copy`",
                            target_ty,
                            field_name,
                            field_ty,
                        ),
                        span,
                    ));
                }
            }
        }
    }

    fn copy_target_fields(&self, target_ty: &IRType) -> Option<Vec<(String, IRType)>> {
        match target_ty {
            IRType::STRUCT(type_ref) => {
                let info = self.context.get_def(type_ref.def_id)?;

                let DefKind::Struct { fields, .. } = &info.kind else {
                    return None;
                };

                Some(fields.iter().map(|(name, ty, _)| (name.clone(), ty.clone())).collect())
            }

            IRType::GENERIC_INSTANCE(base, args) => {
                let info = self.context.get_def(base.def_id)?;

                let DefKind::Struct { fields, generic_params, } = &info.kind else {
                    return None;
                };

                if generic_params.len() != args.len() {
                    return None;
                }

                let substitutions: HashMap<String, IRType> = generic_params
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();

                Some(fields.iter().map(|(name, ty, _)| (name.clone(), ty.substitute(&substitutions))).collect(),
                )
            }

            _ => None,
        }
    }

    fn type_is_copy_compatible(&self, ty: &IRType, copy_trait_def_id: DefID) -> bool {
        match ty {
            IRType::I8 | IRType::I16 | IRType::I32 | IRType::I64 | IRType::ISIZE | 
            IRType::U8 | IRType::U16 | IRType::U32 | IRType::U64 | IRType::USIZE | 
            IRType::F32 | IRType::F64 | IRType::CHAR | IRType::BOOL | IRType::POINTER(_) | 
            IRType::CONST_POINTER(_) | IRType::REF(_) | IRType::CONST_REF(_) | IRType::VOID => true,

            IRType::ARRAY(inner, _) | IRType::OPTIONAL(inner) => {
                self.type_is_copy_compatible(inner, copy_trait_def_id)
            }

            IRType::STRUCT(_) | IRType::GENERIC_INSTANCE(_, _) => {
                self.context.trait_impls.iter().any(|trait_impl| {
                    trait_impl.trait_def_id == copy_trait_def_id &&
                    Self::trait_impl_target_matches(&trait_impl.target_type, ty)
                })
            }

            IRType::GENERIC(_) | IRType::INFERRED_ARRAY(_) | IRType::SLICE(_) => false,
        }
    }

    fn trait_impl_target_matches(pattern: &IRType, actual: &IRType) -> bool {
        let mut substitutions = HashMap::new();

        Self::trait_impl_target_matches_with_substitutions(pattern, actual, &mut substitutions)
    }

    fn trait_impl_target_matches_with_substitutions(pattern: &IRType, actual: &IRType, substitutions: &mut HashMap<String, IRType>) -> bool
    {
        match (pattern, actual) {
            (IRType::GENERIC(name), actual) => {
                if let Some(existing) = substitutions.get(name) {
                    existing == actual
                } else {
                    substitutions.insert(name.clone(), actual.clone());

                    true
                }
            }

            (IRType::GENERIC_INSTANCE(pattern_base, pattern_args), IRType::GENERIC_INSTANCE(actual_base, actual_args)) => 
            {
                pattern_base.def_id == actual_base.def_id &&
                pattern_args.len() == actual_args.len() &&
                pattern_args.iter().zip(actual_args).all(|(pattern, actual)| {
                    Self::trait_impl_target_matches_with_substitutions(pattern, actual, substitutions)
                })
            }

            (IRType::ARRAY(pattern, pattern_len), IRType::ARRAY(actual, actual_len)) => {
                pattern_len == actual_len &&
                Self::trait_impl_target_matches_with_substitutions(pattern, actual, substitutions)
            }

            (IRType::INFERRED_ARRAY(pattern), IRType::INFERRED_ARRAY(actual)) | 
            (IRType::SLICE(pattern), IRType::SLICE(actual)) | (IRType::REF(pattern), IRType::REF(actual)) | 
            (IRType::CONST_REF(pattern), IRType::CONST_REF(actual)) | (IRType::POINTER(pattern), IRType::POINTER(actual)) | 
            (IRType::CONST_POINTER(pattern), IRType::CONST_POINTER(actual)) => {
                Self::trait_impl_target_matches_with_substitutions(pattern, actual, substitutions)
            }

            _ => pattern == actual,
        }
    }

    fn trait_impl_targets_overlap(left: &IRType, right: &IRType) -> bool {
        Self::trait_impl_target_matches(left, right) || Self::trait_impl_target_matches(right, left)
    }

    pub fn is_drop_trait(&self, trait_def_id: DefID) -> bool {
        self.context.get_def(trait_def_id).is_some_and(|info| info.absolute_path.join("::") == traits::DROP_TRAIT_PATH )
    }

    fn substitute_trait_contract_type(
        ty: &IRType,
        self_ty: &IRType,
        trait_generics: &HashMap<String, IRType>,
        method_generics: &HashMap<String, String>,
    ) -> IRType 
    {
        match ty {
            IRType::GENERIC(name) if name == "Self" => {
                self_ty.clone()
            }

            IRType::GENERIC(name) => {
                if let Some(concrete) = trait_generics.get(name) {
                    concrete.clone()
                } else if let Some(mapped) = method_generics.get(name) {
                    IRType::GENERIC(mapped.clone())
                } else {
                    ty.clone()
                }
            }

            IRType::ARRAY(inner, len) => IRType::ARRAY(
                Box::new(Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
                    method_generics,
                )),
                *len,
            ),

            IRType::INFERRED_ARRAY(inner) => IRType::INFERRED_ARRAY(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
                    method_generics,
                ),
            )),

            IRType::SLICE(inner) => IRType::SLICE(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
                    method_generics,
                ),
            )),

            IRType::POINTER(inner) => IRType::POINTER(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
                    method_generics,
                ),
            )),

            IRType::CONST_POINTER(inner) => IRType::CONST_POINTER(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
                    method_generics,
                ),
            )),

            IRType::REF(inner) => IRType::REF(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
                    method_generics,
                ),
            )),

            IRType::CONST_REF(inner) => IRType::CONST_REF(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
                    method_generics,
                ),
            )),

            IRType::OPTIONAL(inner) => IRType::OPTIONAL(Box::new(
                Self::substitute_trait_contract_type(
                    inner,
                    self_ty,
                    trait_generics,
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
                            trait_generics,
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

        let path = match trait_ty {
            ASTType::Path { .. } => trait_ty,

            ASTType::Generic { base, .. } => base.as_ref(),

            _ => {
                return Err(self.error(
                    "S020",
                    "expected a trait name after `extension`",
                    span,
                ));
            }
        };

        let ASTType::Path { id, .. } = path else {
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
                span,
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
        }

        Ok(def_id)
    }

    fn resolve_trait_application(&mut self, trait_ty: &ASTType) -> Result<(DefID, Vec<IRType>), HydraError> {
        let trait_def_id = self.resolve_trait_def_id(trait_ty)?;
        let span = crate::utils::get_type_span(trait_ty);

        let generic_args = match trait_ty {
            ASTType::Path { .. } => Vec::new(),

            ASTType::Generic { args, .. } => {
                let mut lowered = Vec::with_capacity(args.len());

                for arg in args {
                    lowered.push(self.lower_type(arg)?);
                }

                lowered
            }

            _ => unreachable!(),
        };

        let trait_info = self.context.get_def(trait_def_id).expect("resolved trait should exist");

        let DefKind::Trait { generic_params, .. } = &trait_info.kind else {
            unreachable!();
        };

        if generic_args.len() != generic_params.len() {
            return Err(self.error(
                "S020",
                format!(
                    "trait `{}` expects {} generic argument{}, found {}",
                    trait_info.name,
                    generic_params.len(),
                    if generic_params.len() == 1 { "" } else { "s" },
                    generic_args.len(),
                ),
                span,
            ));
        }

        Ok((trait_def_id, generic_args))
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
