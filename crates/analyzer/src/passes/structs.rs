use ir::context::DefKind;
use parser::Item;

use crate::Analyzer;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn populate_struct_definitions(&mut self) {
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
            self.current_filepath = filepath.display().to_string();

            self.current_source = self
                .source_map
                .get_source(&filepath)
                .unwrap_or("")
                .to_string();

            for item in &items {
                let Item::Struct(decl) = item else {
                    continue;
                };

                let mut full_path = module_path.clone();
                full_path.push(decl.name.lexeme.clone());

                let Some(&def_id) = self.global_symbols.get(&full_path) else {
                    self.errors.push(self.error(
                        "S002",
                        format!(
                            "missing definition for struct `{}`",
                            full_path.join("::")
                        ),
                        decl.name.span,
                    ));

                    continue;
                };

                let mut fields = Vec::new();
                let mut failed = false;

                for (field_name, field_type) in &decl.fields {
                    match self.lower_type(field_type) {
                        Ok(ty) => {
                            fields.push((
                                field_name.lexeme.clone(),
                                ty,
                                false,
                            ));
                        }

                        Err(error) => {
                            self.errors.push(error);
                            failed = true;
                        }
                    }
                }

                if failed {
                    continue;
                }

                let generic_params = decl
                    .generic_params
                    .iter()
                    .map(|param| param.name.lexeme.clone())
                    .collect();

                let Some(mut info) =
                self.context.get_def(def_id).cloned()
                else {
                    self.errors.push(self.error(
                        "S002",
                        format!(
                            "missing HIR definition for struct `{}`",
                            full_path.join("::")
                        ),
                        decl.name.span,
                    ));

                    continue;
                };

                info.absolute_path = full_path;

                info.kind = DefKind::Struct {
                    fields,
                    generic_params,
                };

                self.context.update_def(def_id, info);
            }
        }
    }
}

