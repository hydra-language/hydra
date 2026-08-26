use std::collections::HashMap;

use ir::intrinsic::IntrinsicKind;
use parser::ast::*;
use parser::module::{ModuleTree, SourceMap};
use ir::types::Type as IRType;
use ir::context::{HIRContext, DefID, DefKind};
use ir::hir::{HIRBlock, HIRFunction, HIRProgram};
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

    fn populate_impl_registry(&mut self) {
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
            annotations: decl.annotations.clone(),
            intrinsic
        };

        self.context.update_def(
            fn_def_id,
            info,
        );
    }

    fn validate_intrinsic_signature(
        &self,
        kind: IntrinsicKind,
        generic_params: &[String],
        params: &[IRType],
        return_type: &IRType,
        span: Span,
    ) -> Result<(), HydraError> 
    {
        match kind {
            IntrinsicKind::SizeOf | IntrinsicKind::AlignOf => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "layout intrinsic requires exactly one type parameter",
                        span,
                    ));
                }

                if !params.is_empty() {
                    return Err(self.error(
                        "S017",
                        "layout intrinsic accepts no value parameters",
                        span,
                    ));
                }

                if *return_type != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "layout intrinsic must return `usize`",
                        span,
                    ));
                }
            }

            IntrinsicKind::PtrRead => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_read requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_read requires exactly one argument",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::CONST_POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_read expects `*const T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_read expects `*const T`",
                            span,
                        ));
                    }
                }

                match return_type {
                    IRType::GENERIC(name) if name == expected => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_read must return `T`",
                            span,
                        ));
                    }
                }
            }

            IntrinsicKind::PtrWrite => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_write requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 2 {
                    return Err(self.error(
                        "S017",
                        "ptr_write requires exactly two arguments",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_write expects first argument to be `*mut T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_write expects first argument to be `*mut T`",
                            span,
                        ));
                    }
                }

                match &params[1] {
                    IRType::GENERIC(name) if name == expected => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_write expects second argument to be `T`",
                            span,
                        ));
                    }
                }

                if *return_type != IRType::VOID {
                    return Err(self.error(
                        "S017",
                        "ptr_write must return `void`",
                        span,
                    ));
                }
            }

            IntrinsicKind::PtrOffset => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_offset requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 2 {
                    return Err(self.error(
                        "S017",
                        "ptr_offset requires exactly two arguments",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_offset expects first argument to be `*mut T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_offset expects first argument to be `*mut T`",
                            span,
                        ));
                    }
                }

                if params[1] != IRType::ISIZE {
                    return Err(self.error(
                        "S017",
                        "ptr_offset expects second argument to be `isize`",
                        span,
                    ));
                }

                match return_type {
                    IRType::POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_offset must return `*mut T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_offset must return `*mut T`",
                            span,
                        ));
                    }
                }
            }

            IntrinsicKind::Alloc => {
                if !generic_params.is_empty() {
                    return Err(self.error(
                        "S017",
                        "alloc accepts no type parameters",
                        span,
                    ));
                }

                if params.len() != 2 {
                    return Err(self.error(
                        "S017",
                        "alloc requires exactly two arguments",
                        span,
                    ));
                }

                if params[0] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "alloc expects `size` to be `usize`",
                        span,
                    ));
                }

                if params[1] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "alloc expects `align` to be `usize`",
                        span,
                    ));
                }

                match return_type {
                    IRType::POINTER(inner)
                    if inner.as_ref() == &IRType::U8 => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "alloc must return `*mut u8`",
                            span,
                        ));
                    }
                }
            }

            IntrinsicKind::Dealloc => {
                if !generic_params.is_empty() {
                    return Err(self.error(
                        "S017",
                        "dealloc accepts no type parameters",
                        span,
                    ));
                }

                if params.len() != 3 {
                    return Err(self.error(
                        "S017",
                        "dealloc requires exactly three arguments",
                        span,
                    ));
                }

                match &params[0] {
                    IRType::POINTER(inner)
                    if inner.as_ref() == &IRType::U8 => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "dealloc expects first argument to be `*mut u8`",
                            span,
                        ));
                    }
                }

                if params[1] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "dealloc expects `size` to be `usize`",
                        span,
                    ));
                }

                if params[2] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "dealloc expects `align` to be `usize`",
                        span,
                    ));
                }

                if *return_type != IRType::VOID {
                    return Err(self.error(
                        "S017",
                        "dealloc must return `void`",
                        span,
                    ));
                }
            }
        }

        Ok(())
    }


    fn populate_struct_definitions(&mut self) {
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

    fn populate_function_signatures(&mut self) {
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

    fn lower_function(&mut self, decl: &FunctionDecl, parent_path: Option<String>, inherited_generics: &[String]) 
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
            IRType::POINTER(inner) | IRType::CONST_POINTER(inner)=> self.get_impl_registry_key(inner),
            IRType::REF(inner) | IRType::CONST_REF(inner) => self.get_impl_registry_key(inner),
            _ => String::new(),
        }
    }
}
