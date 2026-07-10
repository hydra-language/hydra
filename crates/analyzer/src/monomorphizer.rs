use std::collections::HashMap;

use ir::context::{DefID, DefKind, HIRContext, SymbolInfo};
use ir::hir::{HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRProgram, HIRStmt};
use ir::types::Type;

pub struct Monomorphizer<'a> {
    pub context: &'a mut HIRContext,

    original_functions: HashMap<DefID, HIRFunction>,
    instantiation_cache: HashMap<(DefID, Vec<Type>), DefID>,
    worklist: Vec<(DefID, Vec<Type>, DefID)>,
    specialized_functions: Vec<HIRFunction>,

    struct_worklist: Vec<(DefID, Vec<Type>)>,
    instantiated_structs: HashMap<(DefID, Vec<Type>), (DefID, String)>,
}

impl<'a> Monomorphizer<'a> {

    // Helper to detect if a function belongs to a generic struct
    // (e.g. `Shape::new` belongs to the generic `Shape`)
    fn is_function_generic(func: &HIRFunction, context: &HIRContext) -> bool {
        if !func.generic_params.is_empty() { return true; }

        let parts: Vec<&str> = func.name.split("::").collect();
        if parts.len() > 1 {
            // Extract everything EXCEPT the method name (e.g., "math::Shape::new" -> "math::Shape")
            let struct_name = parts[..parts.len() - 1].join("::");

            if let Some(def_id) = context.find_struct_by_name(&struct_name) {
                let info = context.get_def(def_id).unwrap();

                if let DefKind::Struct { generic_params, .. } = &info.kind {
                    return !generic_params.is_empty();
                }
            }
        }

        false
    }

    pub fn new(context: &'a mut HIRContext, program: HIRProgram) -> Self {
        let mut original_functions = HashMap::new();
        let mut specialized_functions = Vec::new();
        let mut worklist = Vec::new();

        for func in program.functions {
            // FIX 1: Use the helper to skip un-monomorphized extension methods
            if !Self::is_function_generic(&func, context) {
                worklist.push((func.def_id, vec![], func.def_id));
                specialized_functions.push(func.clone());
            }
            original_functions.insert(func.def_id, func);
        }

        Self {
            context,
            original_functions,
            instantiation_cache: HashMap::new(),
            worklist,
            specialized_functions,
            struct_worklist: Vec::new(),
            instantiated_structs: HashMap::new(),
        }
    }

    pub fn run(mut self) -> HIRProgram {
        while let Some((generic_def_id, type_args, specialized_def_id)) = self.worklist.pop() {
            self.process_function(generic_def_id, type_args, specialized_def_id);
        }

        HIRProgram {
            functions: self.specialized_functions,
            structs: vec![], 
            globals: vec![],
        }
    }

    fn process_function(&mut self, generic_def_id: DefID, type_args: Vec<Type>, specialized_def_id: DefID) {
        let generic_func = self.original_functions.get(&generic_def_id).unwrap().clone();

        let mut substitutions = HashMap::new();
        for (i, param_name) in generic_func.generic_params.iter().enumerate() {
            if let Some(concrete_ty) = type_args.get(i) {
                substitutions.insert(param_name.clone(), concrete_ty.clone());
            }
        }

        let mut specialized_body = generic_func.body.clone();
        for stmt in &mut specialized_body.stmts {
            self.substitute_stmt(stmt, &substitutions);
        }

        if let Some(f) = self.specialized_functions.iter_mut().find(|f| f.def_id == specialized_def_id) {
            f.body = specialized_body;
        }

        let specialized_func = self.specialized_functions.iter()
            .find(|f| f.def_id == specialized_def_id)
            .unwrap()
            .clone();

        for (param_def_id, _) in &specialized_func.params {
            if let Some(mut info) = self.context.get_def(*param_def_id).cloned() {
                match &mut info.kind {
                    DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } => {
                        *ty = ty.substitute(&substitutions);
                        // Pass subs into resolve_type
                        *ty = self.resolve_type(&ty.clone(), &substitutions);
                    }
                    _ => {}
                }
                self.context.update_def(*param_def_id, info);
            }
        }
    }

    fn substitute_stmt(&mut self, stmt: &mut HIRStmt, subs: &HashMap<String, Type>) {
        match stmt {
            HIRStmt::Expr(expr) => {
                self.substitute_expr(expr, subs);
            }

            HIRStmt::VarDecl { def_id, init, .. } => {
                let mut resolved_init_ty = None;

                if let Some(init_expr) = init {
                    self.substitute_expr(init_expr, subs);
                    // Grab the cleanly resolved type (e.g. Shape__i32) from the expression!
                    resolved_init_ty = Some(init_expr.ty.clone()); 
                }

                if let Some(mut info) = self.context.get_def(*def_id).cloned() {
                    match &mut info.kind {
                        DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } => {
                            if let Some(init_ty) = resolved_init_ty {
                                *ty = init_ty; // Inherit perfectly!
                            } else {
                                *ty = ty.substitute(subs);
                                *ty = self.resolve_type(&ty.clone(), subs);
                            }
                        }

                        _ => {}
                    }

                    self.context.update_def(*def_id, info);
                }
            }
        }
    }

    fn substitute_expr(&mut self, expr: &mut HIRExpr, subs: &HashMap<String, Type>) {
        let original_type = expr.ty.clone();
        expr.ty = self.resolve_type(&original_type.substitute(subs), subs);

        match &mut expr.kind {
            HIRExprKind::Binary { lhs, rhs, .. } => {
                self.substitute_expr(lhs, subs);
                self.substitute_expr(rhs, subs);
            }
            HIRExprKind::Unary { operand, .. } |
            HIRExprKind::Cast { expr: operand, .. } |
            HIRExprKind::Borrow { target: operand, .. } |
            HIRExprKind::Dereference { target: operand, .. } => {
                self.substitute_expr(operand, subs);
            }
            HIRExprKind::Assign { target, value } => {
                self.substitute_expr(target, subs);
                self.substitute_expr(value, subs);
            }
            
            HIRExprKind::If { cond, then_block, else_block } => {
                self.substitute_expr(cond, subs);
                for s in &mut then_block.stmts { self.substitute_stmt(s, subs); }
                if let Some(eb) = else_block {
                    for s in &mut eb.stmts { self.substitute_stmt(s, subs); }
                }
            }

            HIRExprKind::Loop(block) => {
                for s in &mut block.stmts { self.substitute_stmt(s, subs); }
            }

            HIRExprKind::Block(block) => {
                for s in &mut block.stmts { self.substitute_stmt(s, subs); }
            }

            HIRExprKind::Return(Some(ret_expr)) => {
                self.substitute_expr(ret_expr, subs);
            }
            
            HIRExprKind::StructInit { def_id, values } => {
                for v in values.iter_mut() { self.substitute_expr(v, subs); }

                let info = self.context.get_def(*def_id).unwrap().clone();
                if let DefKind::Struct { generic_params, fields } = &info.kind {
                    if !generic_params.is_empty() {
                        let mut inferred = HashMap::new();
                        for ((_, field_ty, _), val) in fields.iter().zip(values.iter()) {
                            let subbed_field_ty = field_ty.substitute(subs);
                            self.infer_type_args(&subbed_field_ty, &val.ty, &mut inferred);
                        }
                        
                        let concrete_args: Vec<Type> = generic_params.iter()
                            .map(|name| {
                                // Fallback to `subs` if the field inference missed it
                                inferred.get(name)
                                    .or_else(|| subs.get(name))
                                    .cloned()
                                    .unwrap_or(Type::GENERIC(name.clone()))
                            })
                            .collect();

                        let (concrete_def_id, mangled) = self.get_or_create_struct_specialization(*def_id, concrete_args);
                        *def_id = concrete_def_id;
                        expr.ty = Type::STRUCT(mangled);
                    }
                }
            }
            HIRExprKind::ArrayInit { elements } => {
                for e in elements.iter_mut() { self.substitute_expr(e, subs); }
            }
            HIRExprKind::ArrayAccess { array, index } => {
                self.substitute_expr(array, subs);
                self.substitute_expr(index, subs);
            }
            HIRExprKind::FieldAccess { object, field_index } => {
                self.substitute_expr(object, subs);

                let obj_ty = match &object.ty {
                    Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) => inner.as_ref(),
                    other => other,
                };

                if let Type::STRUCT(mangled_name) = obj_ty {
                    if let Some(def_id) = self.context.find_struct_by_name(mangled_name) {
                        let info = self.context.get_def(def_id).unwrap();
                        if let DefKind::Struct { fields, .. } = &info.kind {
                            expr.ty = fields[*field_index].1.clone();
                        }
                    }
                }
            }

            HIRExprKind::BuiltinCall { args, .. } => {
                for arg in args.iter_mut() { self.substitute_expr(arg, subs); }
            }

            HIRExprKind::Call { callee, args, generic_args } => {
                for arg in args.iter_mut() { self.substitute_expr(arg, subs); }

                let callee_info = self.context.get_def(*callee).unwrap().clone();
                if let DefKind::Function { generic_params, params, .. } = &callee_info.kind {
                    if !generic_params.is_empty() {
                        let mut concrete_args: Vec<Type> = generic_args.iter()
                            .map(|g| g.substitute(subs))
                            .collect();

                        if concrete_args.len() < generic_params.len() {
                            let mut inferred: HashMap<String, Type> = HashMap::new();
                            for (param_ty, arg) in params.iter().zip(args.iter()) {
                                self.infer_type_args(param_ty, &arg.ty, &mut inferred);
                            }
                            concrete_args = generic_params.iter()
                                .map(|name| inferred.get(name).cloned().unwrap_or(Type::GENERIC(name.clone())))
                                .collect();
                        }

                        if concrete_args.iter().all(|t| !matches!(t, Type::GENERIC(_))) {
                            let local_subs: HashMap<String, Type> = generic_params.iter().cloned()
                                .zip(concrete_args.iter().cloned())
                                .collect();

                            let specialized_callee = self.get_or_create_specialization(*callee, concrete_args);
                            *callee = specialized_callee;

                            // Pass local_subs into resolve_type
                            expr.ty = self.resolve_type(&original_type.substitute(&local_subs), &local_subs);
                        }
                    }
                }
            }
            _ => {} 
        }
    }

    fn get_or_create_specialization(&mut self, generic_def_id: DefID, type_args: Vec<Type>) -> DefID {
        let cache_key = (generic_def_id, type_args.clone());
        if let Some(&specialized_def_id) = self.instantiation_cache.get(&cache_key) {
            return specialized_def_id;
        }

        let generic_info = self.context.get_def(generic_def_id).unwrap().clone();
        let type_suffixes: Vec<String> = type_args.iter().map(|t| t.mangle()).collect();
        let mangled_name = format!("{}__{}", generic_info.name, type_suffixes.join("_"));

        let mut specialized_info = generic_info.clone();
        specialized_info.name = mangled_name.clone();
        specialized_info.absolute_path = vec![mangled_name.clone()];

        let subs: HashMap<String, Type> = match &generic_info.kind {
            DefKind::Function { generic_params, .. } => generic_params.iter().cloned()
                .zip(type_args.clone())
                .collect(),
            _ => HashMap::new(),
        };

        if let DefKind::Function { params, return_type, annotations, .. } = generic_info.kind {
            specialized_info.kind = DefKind::Function {
                // Pass subs into resolve_type
                params: params.into_iter().map(|ty| self.resolve_type(&ty.substitute(&subs), &subs)).collect(),
                return_type: self.resolve_type(&return_type.substitute(&subs), &subs),
                generic_params: vec![],
                annotations,
            };
        }

        let specialized_def_id = self.context.insert_def(specialized_info);
        self.instantiation_cache.insert(cache_key, specialized_def_id);

        let mut new_func = self.original_functions.get(&generic_def_id).unwrap().clone();
        new_func.name = mangled_name;
        new_func.def_id = specialized_def_id;
        new_func.generic_params.clear();

        new_func.params = new_func.params.into_iter()
            .map(|(id, ty)| (id, self.resolve_type(&ty.substitute(&subs), &subs)))
            .collect();
        new_func.return_type = self.resolve_type(&new_func.return_type.substitute(&subs), &subs);

        new_func.body = HIRBlock { stmts: vec![], span: new_func.body.span };
        self.specialized_functions.push(new_func);
        self.worklist.push((generic_def_id, type_args, specialized_def_id));

        specialized_def_id
    }

    fn get_or_create_struct_specialization(&mut self, generic_def_id: DefID, type_args: Vec<Type>) -> (DefID, String) {
        let cache_key = (generic_def_id, type_args.clone());
        if let Some(entry) = self.instantiated_structs.get(&cache_key) {
            return entry.clone();
        }

        let generic_info = self.context.get_def(generic_def_id).unwrap().clone();
        let type_suffixes: Vec<String> = type_args.iter().map(|t| t.mangle()).collect();
        let mangled_name = format!("{}__{}", generic_info.name, type_suffixes.join("_"));

        let subs: HashMap<String, Type> = match &generic_info.kind {
            DefKind::Struct { generic_params, .. } => generic_params.iter().cloned()
                .zip(type_args.clone())
                .collect(),
            _ => panic!("expected struct"),
        };

        let concrete_fields = match &generic_info.kind {
            DefKind::Struct { fields, .. } => fields.iter()
                // Pass subs into resolve_type
                .map(|(name, ty, is_mut)| (name.clone(), self.resolve_type(&ty.substitute(&subs), &subs), *is_mut))
                .collect::<Vec<_>>(),
            _ => panic!("expected struct"),
        };

        let concrete_info = SymbolInfo {
            name: mangled_name.clone(),
            span: generic_info.span,
            absolute_path: vec![mangled_name.clone()],
            kind: DefKind::Struct {
                fields: concrete_fields,
                generic_params: vec![],
            },
            is_pub: generic_info.is_pub
        };
        
        let concrete_def_id = self.context.insert_def(concrete_info);
        self.instantiated_structs.insert(cache_key, (concrete_def_id, mangled_name.clone()));

        (concrete_def_id, mangled_name)
    }

    // FIX 2: Modified resolve_type to take the current `subs` context map
    fn resolve_type(&mut self, ty: &Type, subs: &HashMap<String, Type>) -> Type {
        match ty {
            Type::GENERIC_INSTANCE(base, args) => {
                let concrete_args: Vec<Type> = args.iter().map(|a| self.resolve_type(a, subs)).collect();

                if let Type::STRUCT(name) = base.as_ref() {
                    if let Some(def_id) = self.context.find_struct_by_name(name) {
                        let info = self.context.get_def(def_id).unwrap();

                        if let DefKind::Struct { generic_params, .. } = &info.kind {
                            if !generic_params.is_empty() {
                                let (_, mangled) = self.get_or_create_struct_specialization(def_id, concrete_args);
                                return Type::STRUCT(mangled);
                            }
                        }
                    }
                }

                Type::GENERIC_INSTANCE(base.clone(), concrete_args)
            }

            Type::STRUCT(name) => {
                if let Some(def_id) = self.context.find_struct_by_name(name) {
                    let info = self.context.get_def(def_id).unwrap();
                    if let DefKind::Struct { generic_params, .. } = &info.kind {
                        if !generic_params.is_empty() {
                            // FIX: Only auto-deduce if we have EVERY substitution!
                            let mut can_specialize = true;
                            let mut concrete_args = Vec::new();

                            for p in generic_params {
                                if let Some(conc) = subs.get(p) {
                                    concrete_args.push(conc.clone());
                                } else {
                                    can_specialize = false;
                                    break;
                                }
                            }

                            if can_specialize {
                                let (_, mangled) = self.get_or_create_struct_specialization(def_id, concrete_args);
                                return Type::STRUCT(mangled);
                            }
                            // If we couldn't find them all, return the unspecialized struct 
                            // and let the expression/call logic figure it out later.
                        }
                    }
                }
                ty.clone()
            }

            Type::POINTER(inner) => Type::POINTER(Box::new(self.resolve_type(inner, subs))),
            Type::REF(inner) => Type::REF(Box::new(self.resolve_type(inner, subs))),
            Type::CONST_REF(inner) => Type::CONST_REF(Box::new(self.resolve_type(inner, subs))),

            other => other.clone(),
        }
    }

    fn infer_type_args(&self, param_ty: &Type, arg_ty: &Type, inferred: &mut HashMap<String, Type>) {
        match (param_ty, arg_ty) {
            (Type::GENERIC(name), concrete) => {
                inferred.entry(name.clone()).or_insert_with(|| concrete.clone());
            }

            (Type::REF(p), Type::REF(a)) | 
            (Type::CONST_REF(p), Type::CONST_REF(a)) | 
            (Type::POINTER(p), Type::POINTER(a)) => 
            {
                self.infer_type_args(p, a, inferred);
            }

            (Type::GENERIC_INSTANCE(_, p_args), Type::GENERIC_INSTANCE(_, a_args)) => {
                for (p, a) in p_args.iter().zip(a_args.iter()) {
                    self.infer_type_args(p, a, inferred);
                }
            }

            _ => {}
        }
    }
}
