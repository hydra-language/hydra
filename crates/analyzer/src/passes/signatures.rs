use ir::{context::{DefID, DefKind}, intrinsic::IntrinsicKind};
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
                            &[]
                        );
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
                        let parent_name = self.get_impl_registry_key(&target_ty);

                        if parent_name.is_empty() {
                            continue;
                        }

                        let extension_generic_params: Vec<String> = extension
                            .generic_params
                            .iter()
                            .map(|p| p.name.lexeme.clone())
                            .collect();

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
                                &extension_generic_params
                            );
                        }

                        self.current_self_type = previous_self_type;
                    }

                    _ => {}
                }
            }
        }
    }

        fn populate_one_function_signature(&mut self, decl: &FunctionDecl, fn_def_id: DefID, 
        absolute_path: Vec<String>, inherited_generics: &[String]) 
    {
        let mut generic_params = inherited_generics.to_vec();
        for param in &decl.generic_params {
            let name = param.name.lexeme.clone();

            if !generic_params.contains(&name) {
                generic_params.push(name);
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
            annotations: decl.annotations.clone(),
            intrinsic,
        };

        self.context.update_def(
            fn_def_id,
            info,
        );
    }
}
