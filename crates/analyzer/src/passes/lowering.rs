use ir::{context::DefKind, hir::{HIRBlock, HIRFunction}};
use parser::FunctionDecl;
use ir::types::Type as IRType;

use crate::Analyzer;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_function(&mut self, decl: &FunctionDecl, parent_path: Option<String>, inherited_generics: &[String]) 
        -> Option<HIRFunction> 
    {

        let (def_id, full_path) = if let Some(parent_path) = parent_path {
            // extension/associated function.
            //
            // parent_path is the impl-registry key, e.g.
            //     engine::math::ops::Math
            //
            // the method's DefID was created by Resolver::resolve_item
            // and recorded against method.id.
            let def_id = self.name_resolver.get_resolution(decl.id)?;

            let mut full_path: Vec<String> = parent_path
                .split("::")
                .map(|part| part.to_string())
                .collect();

            full_path.push(decl.name.lexeme.to_string());

            (def_id, full_path)
        } else {
            // ordinary module-level function.
            let mut full_path = self.current_module.clone();
            full_path.push(decl.name.lexeme.to_string());

            let def_id = *self.global_symbols.get(&full_path)?;

            (def_id, full_path)
        };

        let mut info = self.context.get_def(def_id)?.clone();

        // this matters for MIR/codegen: calls derive the symbol name
        // from SymbolInfo::absolute_path.
        info.absolute_path = full_path.clone();

        let ret_type = if let Some(rt_node) = &decl.return_type {
            self.lower_type(rt_node).unwrap_or(IRType::VOID)
        } else {
            IRType::VOID
        };
        self.current_return_type = Some(ret_type.clone());

        let mut ir_params = Vec::new();

        for (param_index, (param_token, param_type_node)) in decl.parameters.iter().enumerate() {
            let p_ty = match self.lower_type(param_type_node) {
                Ok(ty) => ty,

                Err(error) => {
                    self.errors.push(error);
                    return None;
                }
            };

            let Some(param_def_id) = self.name_resolver.get_parameter(decl.id, param_index) else 
            {
                self.errors.push(self.error(
                    "S002",
                    format!(
                        "resolution failed for parameter `{}`",
                        param_token.lexeme
                    ),
                    param_token.span,
                ));

                return None;
            };

            let Some(mut p_info) = self.context.get_def(param_def_id).cloned() else 
            {
                self.errors.push(self.error(
                    "S002",
                    format!(
                        "missing definition for parameter `{}`",
                        param_token.lexeme
                    ),
                    param_token.span,
                ));

                return None;
            };

            p_info.kind = DefKind::Variable {
                ty: p_ty.clone(),
                is_mutable: true,
            };

            self.context.update_def(
                param_def_id,
                p_info,
            );

            ir_params.push((
                param_def_id,
                p_ty,
            ));
        }

        if let DefKind::Function { ref mut return_type, ref mut params, .. } = info.kind {
            *return_type = ret_type.clone();
            *params = ir_params.iter().map(|(_, ty)| ty.clone()).collect();
        }

        self.context.update_def(def_id, info);

        let is_inline = decl.annotations.iter().any(|a| a.name == "inline");

        let mut ir_body = Vec::new();
        if let Some(body_block) = &decl.body {
            match self.lower_block(body_block) {
                Ok(block) => ir_body = block.stmts,
                Err(e) => self.errors.push(e), 
            }
        }

        self.current_return_type = None;

        let fn_name = full_path.join("::");

        let mut generic_params = inherited_generics.to_vec();

        for param in &decl.generic_params {
            let name = param.name.lexeme.clone();

            if !generic_params.contains(&name) {
                generic_params.push(name);
            }
        }

        Some(HIRFunction {
            name: fn_name,
            def_id,
            params: ir_params,
            return_type: ret_type,
            body: HIRBlock { stmts: ir_body, span: decl.name.span },
            is_extern: decl.is_extern,
            is_inline,
            generic_params
        })
    }
}
