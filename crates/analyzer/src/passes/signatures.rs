use ir::{context::{DefID, DefKind, TraitBound}, intrinsic::IntrinsicKind};
use parser::{FunctionDecl, Item};
use ir::types::Type as IRType;

use crate::Analyzer;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn populate_function_signatures(&mut self) {
        // snapshot because lower_type() requires &mut self.
        let files: Vec<_> = self.program
            .parsed_files
            .iter()
            .map(|(filepath, (module_path, items))| {
                (
                    filepath.clone(),
                    module_path.clone(),
                    items.clone(),
                )
            })
            .collect();

        for (filepath, module_path, items) in files {
            self.current_module = module_path.clone();
            self.current_filepath =
                filepath.display().to_string();

            self.current_source = self
                .source_map
                .get_source(&filepath)
                .unwrap_or("")
                .to_string();

            for item in &items {
                match item {
                    // normal module-level function
                    Item::Function(decl) => {
                        let mut full_path = module_path.clone();
                        full_path.push(decl.name.lexeme.clone());

                        let Some(&def_id) = self.global_symbols.get(&full_path) else 
                        {
                            self.errors.push(self.error(
                                "S002",
                                format!(
                                    "missing definition for function `{}`",
                                    full_path.join("::")
                                ),
                                decl.name.span,
                            ));

                            continue;
                        };

                        self.populate_one_function_signature(
                            decl,
                            def_id,
                            full_path,
                            &[],
                            &[]
                        );
                    }

                    Item::Trait(decl) => {
                        let Some(trait_def_id) = self.name_resolver.get_resolution(decl.id) else {
                            self.errors.push(self.error(
                                "S002",
                                format!("missing definition for trait `{}`", decl.name.lexeme),
                                decl.name.span
                            ));

                            continue;
                        };

                        let trait_info = self.context.get_def(trait_def_id).cloned().unwrap();
                        let trait_path = trait_info.absolute_path.clone();

                        //
                        // inside a trait contract, `Self` is not the
                        // trait itself as a concrete runtime type.
                        //
                        // it is the type that eventually implements
                        // this contract.
                        //
                        let previous_self_type = self.current_self_type.replace(IRType::GENERIC("Self".to_string()));

                        let trait_generic_params: Vec<String> = decl.generic_params
                            .iter()
                            .map(|param| param.name.lexeme.clone())
                            .collect();
                        
                        let mut method_ids = Vec::new();
                        for method in &decl.methods {
                            let Some(method_id) = self.name_resolver.get_resolution(method.id) else {
                                self.errors.push(self.error(
                                    "S002",
                                    format!(
                                        "missing definition of trait method `{}::{}`", 
                                        decl.name.lexeme, 
                                        method.name.lexeme
                                    ),
                                    method.name.span
                                ));

                                continue;
                            };

                            if method.name.lexeme == "drop" {
                                let trait_def_id = self
                                    .name_resolver
                                    .get_resolution(decl.id)
                                    .unwrap();

                                if !self.is_drop_trait(trait_def_id) {
                                    self.errors.push(self.error(
                                        "S026",
                                        "`drop` is reserved for the `Drop` trait",
                                        method.name.span,
                                    ).with_help("destructor behavior must be declared through `extension Drop on Type { ... }`")
                                    .with_help("see `core::ops::drop` for trait signature"));
                                }
                            }

                            let mut method_path = trait_path.clone();
                            method_path.push(method.name.lexeme.clone());

                            self.populate_one_function_signature(method, method_id, method_path, &trait_generic_params, &[]);

                            method_ids.push(method_id);
                        }

                        self.current_self_type = previous_self_type;

                        let Some(mut trait_info) = self.context.get_def(trait_def_id).cloned() else {
                            continue;
                        };

                        trait_info.kind = DefKind::Trait {
                            generic_params: decl.generic_params.iter().map(|param| param.name.lexeme.clone()).collect(),
                            methods: method_ids
                        };

                        self.context.update_def(trait_def_id, trait_info);
                    }

                    // extension methods / associated functions
                    Item::Extension(extension) => {
                        let target_ty = match self.lower_type(&extension.target_type) {
                            Ok(ty) => ty,

                            Err(error) => {
                                self.errors.push(error);
                                continue;
                            }
                        };

                        let previous_self_type = self.current_self_type.replace(target_ty.clone());

                        let parent_name = match self.extension_parent_path(&target_ty, extension.target_trait.as_ref()) 
                        {
                            Ok(path) => path,

                            Err(error) => {
                                self.errors.push(error);
                                self.current_self_type = previous_self_type;
                                continue;
                            }
                        };

                        if parent_name.is_empty() {
                            self.current_self_type = previous_self_type;
                            continue;
                        }

                        let extension_generic_params: Vec<String> = extension
                            .generic_params
                            .iter()
                            .map(|p| p.name.lexeme.clone())
                            .collect();

                        let extension_bounds = match self.lower_where_clause_bounds(extension.where_clause.as_ref()) 
                        {
                            Ok(bounds) => bounds,

                            Err(error) => {
                                self.errors.push(error);
                                self.current_self_type = previous_self_type;
                                continue;
                            }
                        };

                        let parent_path: Vec<String> = parent_name
                            .split("::")
                            .map(|part| part.to_string())
                            .collect();

                        for method in &extension.methods {
                            // extension methods aren't in global_symbols.
                            // resolver recorded their actual definition
                            // against the method declaration's NodeID.
                            let Some(method_def_id) = self.name_resolver.get_resolution(method.id) else 
                            {
                                self.errors.push(self.error(
                                    "S002",
                                    format!(
                                        "missing definition for method `{}::{}`",
                                        parent_name,
                                        method.name.lexeme
                                    ),
                                    method.name.span,
                                ));

                                continue;
                            };

                            let mut method_path = parent_path.clone();

                            method_path.push(
                                method.name.lexeme.clone()
                            );

                            self.populate_one_function_signature(
                                method,
                                method_def_id,
                                method_path,
                                &extension_generic_params,
                                &extension_bounds
                            );
                        }

                        self.current_self_type = previous_self_type;
                    }

                    _ => {}
                }
            }
        }
    }

    fn populate_one_function_signature(
        &mut self, 
        decl: &FunctionDecl, 
        fn_def_id: DefID, 
        absolute_path: Vec<String>, 
        inherited_generics: &[String],
        inherited_bounds: &[TraitBound]
    ) 
    {
        let mut generic_params = inherited_generics.to_vec();
        for param in &decl.generic_params {
            let name = param.name.lexeme.clone();

            if !generic_params.contains(&name) {
                generic_params.push(name);
            }
        }

        let mut trait_bounds = inherited_bounds.to_vec();
        match self.lower_where_clause_bounds(decl.where_clause.as_ref()) {
            Ok(bounds) => trait_bounds.extend(bounds),
            Err(error) => {
                self.errors.push(error); 
                return;
            }
        }

        let mut param_types = Vec::new();

        for (param_index, (param_token, param_type)) in decl.parameters.iter().enumerate() {
            let param_ty = match self.lower_type(param_type) {
                Ok(ty) => ty,

                Err(error) => {
                    self.errors.push(error);
                    continue;
                }
            };

            param_types.push(param_ty.clone());

            let Some(param_def_id) = self.name_resolver.get_parameter(
                decl.id,
                param_index,
            ) else {
                self.errors.push(self.error(
                    "S002",
                    format!(
                        "missing definition for parameter `{}`",
                        param_token.lexeme
                    ),
                    param_token.span,
                ));

                continue;
            };

            let Some(mut param_info) = self.context.get_def(param_def_id).cloned() else 
            {
                self.errors.push(self.error(
                    "S002",
                    format!(
                        "missing HIR definition for parameter `{}`",
                        param_token.lexeme
                    ),
                    param_token.span,
                ));

                continue;
            };

            param_info.kind = DefKind::Variable {
                ty: param_ty,
                is_mutable: true,
            };

            self.context.update_def(
                param_def_id,
                param_info,
            );
        }

        let return_type = if let Some(return_node) = &decl.return_type {
            match self.lower_type(return_node) {
                Ok(ty) => ty,

                Err(error) => {
                    self.errors.push(error);
                    IRType::VOID
                }
            }
        } else {
            IRType::VOID
        };

        let Some(mut info) = self.context.get_def(fn_def_id).cloned() else 
        {
            self.errors.push(self.error(
                "S002",
                format!(
                    "missing function definition for `{}`",
                    absolute_path.join("::")
                ),
                decl.name.span,
            ));

            return;
        };

        let has_intrinsic_attr = decl.annotations.iter().any(|a| a.name == "intrinsic");

        let intrinsic = if has_intrinsic_attr {
            match IntrinsicKind::from_path(&absolute_path) {
                Some(kind) => Some(kind),

                None => {
                    self.errors.push(self.error(
                        "S016",
                        format!(
                            "`#[intrinsic]` is not permitted on `{}`",
                            absolute_path.join("::")
                        ),
                        decl.name.span,
                    ).with_help(
                            "compiler intrinsics must be declared in `core::intrinsics`"
                        ));

                    None
                }
            }
        } else {
            None
        };

        //
        // validate the compiler/std intrinsic ABI here.
        //
        if let Some(kind) = intrinsic {
            if let Err(error) = self.validate_intrinsic_signature(
                kind,
                &generic_params,
                &param_types,
                &return_type,
                decl.name.span,
            ) {
                self.errors.push(error);
                return;
            }
        }

        // critical for associated functions/methods:
        //
        // Math::new
        // Math::multiply
        //
        // need canonical paths before any call body is analyzed.
        info.absolute_path = absolute_path;

        info.kind = DefKind::Function {
            params: param_types,
            return_type,
            generic_params,
            owner_generic_count: inherited_generics.len(),
            trait_bounds,
            annotations: decl.annotations.clone(),
            intrinsic,
        };

        self.context.update_def(
            fn_def_id,
            info,
        );
    }
}
