use super::super::Analyzer;

use errors::error::{
    HydraError,
    Span,
};
use ir::context::DefKind;
use ir::hir::{
    HIRExpr,
    HIRExprKind,
};
use ir::types::Type as IRType;
use parser::ast::Expr as ASTExpr;

use std::collections::HashMap;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_call_expr(&mut self, node: &ASTExpr, expected: Option<&IRType>, span: Span) 
        -> Result<HIRExpr, HydraError> 
    {
        let ASTExpr::FunctionCall { .. } = node else {
            unreachable!();
        };

        match node {
            ASTExpr::FunctionCall { callee, arguments, generic_args, owner_generic_args, .. } => {

                //
                // a call whose callee is a member expression:
                //
                //     self.ptr::as_ptr()
                //
                // is instance-method dispatch on the projected receiver.
                //
                // plain local receivers such as:
                //
                //     ptr::as_ptr()
                //
                // arrive as Path expressions and are handled by the existing
                // variable-path dispatch below.
                //
                if let ASTExpr::Member { object, property, .. } = &**callee {
                    if !owner_generic_args.is_empty() {
                        return Err(self.error(
                            "S004",
                            "owner generic arguments can only qualify a type",
                            span,
                        ));
                    }

                    let lhs_expr = self.lower_expr_with_type(object, None)?;

                    return self.lower_instance_method_call(
                        lhs_expr,
                        &property.lexeme,
                        arguments,
                        generic_args,
                        expected,
                        span,
                    );
                }

                let call_name_debug = match &**callee {
                    ASTExpr::Variable { name, .. } => name.lexeme.to_string(),
                    ASTExpr::Path { segments, .. } => segments.iter().map(|s| s.lexeme.as_str()).collect::<Vec<_>>().join("::"),
                    _ => "".to_string()
                };

                if call_name_debug == "print" || call_name_debug == "println" {
                    let mut args = Vec::new();
                    for arg in arguments { args.push(self.lower_expr(arg)?); }
                    return Ok(HIRExpr {
                        kind: HIRExprKind::BuiltinCall { name: call_name_debug, args },
                        ty: IRType::VOID,
                        span
                    });
                }

                // fetch the ID of the callee expression, not the outer FunctionCall
                let callee_id = crate::utils::get_expr_id(callee);
                let def_id = self.name_resolver.get_resolution(callee_id)
                    .ok_or_else(|| self.error("S002", format!("undefined function `{}`", call_name_debug), span))?;    

                let info = self.context.get_def(def_id).cloned().ok_or_else(|| 
                    {
                        self.error("S002", format!("missing definition for `{}`", call_name_debug), span)
                    }
                )?;

                // a path such as:
                //
                //     math::multiply(...)
                //
                // may have resolved to the local value `math`.
                //
                // in that case the final path segment is a method name,
                // not part of the receiver's definition path.
                if matches!(info.kind, DefKind::Variable { .. } | DefKind::Constant { .. }) {
                    if let ASTExpr::Path { segments, .. } = &**callee {
                        if segments.len() >= 2 {
                            let method_name = &segments.last().unwrap().lexeme;

                            // lower_expr(Path) is safe here:
                            //
                            // the resolver associated this path's NodeID
                            // with the receiver's DefID, so this produces
                            // VarRef(math), not a reference to multiply.
                            let lhs_expr = self.lower_expr_with_type(callee, None)?;
                            return self.lower_instance_method_call(
                                lhs_expr,
                                method_name,
                                arguments,
                                generic_args,
                                expected,
                                span,
                            );
                        }
                    }
                }

                let actual_def_id = if let DefKind::Struct { .. } = info.kind {
                    if let ASTExpr::Path { segments, .. } = &**callee {
                        let method_name = &segments.last().unwrap().lexeme;
                        let struct_name = info.absolute_path.join("::");

                        if let Some(type_methods) = self.impl_registry.get(&struct_name) {
                            if let Some(&m_def_id) = type_methods.get(method_name) {
                                m_def_id
                            } else {
                                return Err(self.error("S005", format!("struct `{}` has no associated function `{}`", struct_name, method_name), span));
                            }
                        } else {
                            return Err(self.error("S005", format!("struct `{}` has no associated function `{}`", struct_name, method_name), span));
                        }
                    } else {
                        return Err(self.error("S003", "target is a struct, not a function", span));
                    }
                } else {
                    def_id
                };

                let actual_info = self.context.get_def(actual_def_id).unwrap();

                let (param_types, return_type, function_gp, intrinsic, owner_generic_count) = match &actual_info.kind {
                    DefKind::Function { params, return_type, intrinsic, generic_params, owner_generic_count, .. } =>
                    {
                        (params.clone(), return_type.clone(), generic_params.clone(), *intrinsic, *owner_generic_count)
                    }

                    _ => {
                        return Err(self.error("S003", "target is not a function", span));
                    }
                };

                let callable_name = if call_name_debug.is_empty() {
                    actual_info.name.clone()
                } else {
                    call_name_debug.clone()
                };

                self.check_call_arity(
                    &format!("function `{}`", callable_name),
                    param_types.len(),
                    arguments.len(),
                    span,
                )?;

                if owner_generic_count > function_gp.len() {
                    return Err(self.error(
                        "S006",
                        "invalid generic metadata for function",
                        span,
                    ));
                }

                let owner_gp = &function_gp[..owner_generic_count];
                let callable_gp = &function_gp[owner_generic_count..];

                //
                // Foo::<A, B>::method()
                //
                // if an owner fishtail is explicitly present, it describes
                // the complete owner type.
                //
                if !owner_generic_args.is_empty() && owner_generic_args.len() != owner_gp.len() {
                    return Err(self.error(
                        "S004",
                        format!("type expected {} generic argument{}, found {}",
                            owner_gp.len(),
                            if owner_gp.len() == 1 {
                                ""
                            } else {
                                "s"
                            },
                            owner_generic_args.len(),
                        ),
                        span,
                    ));
                }


                //
                // Foo::method::<U>()
                //
                // these can only bind the function's own generic parameters.
                //
                if generic_args.len() > callable_gp.len() {
                    return Err(self.error(
                        "S004",
                        format!("function expected at most {} generic argument{}, found {}",
                            callable_gp.len(),
                            if callable_gp.len() == 1 {
                                ""
                            } else {
                                "s"
                            },
                            generic_args.len(),
                        ),
                        span,
                    ));
                }

                //
                // start with explicitly supplied generic arguments:
                //
                //     Foo::<i32>::new()
                //
                let mut lowered_generics = Vec::new();

                for node in generic_args {
                    lowered_generics.push(self.lower_type(node)?);
                }

                if lowered_generics.len() > function_gp.len() {
                    return Err(self.error(
                        "S004",
                        format!("function expected at most {} generic argument{}, found {}",
                            function_gp.len(),
                            if function_gp.len() == 1 {
                                ""
                            } else {
                                "s"
                            },
                            lowered_generics.len(),
                        ),
                        span,
                    ));
                }

                let mut substitutions = HashMap::<String, IRType>::new();

                //
                // owner:
                //     Foo::<i32>::bar()
                //
                for (name, node) in owner_gp.iter().zip(owner_generic_args.iter()) {
                    substitutions.insert(name.clone(), self.lower_type(node)?);
                }

                //
                // function:
                //     Foo::bar::<u64>()
                //
                for (name, node) in callable_gp.iter().zip(generic_args.iter()) {
                    substitutions.insert(name.clone(), self.lower_type(node)?);
                }

                //
                // lower value arguments and infer generics from them.
                //
                //     fn identity<T>(value: T) -> T
                //     identity(5)
                //
                // infers:
                //
                //     T = i32
                //
                let mut args = Vec::new();

                for (i, node) in arguments.iter().enumerate() {
                    let declared_param = param_types.get(i);

                    let substituted_param = declared_param.map(
                        |ty| ty.substitute(&substitutions)
                    );

                    let argument_expected = substituted_param.as_ref().filter(|&ty| !ty.contains_generic());

                    let mut arg = self.lower_expr_with_type(node, argument_expected)?;
                    if let Some(target) = argument_expected {
                        arg = self.coerce_primitive(arg, target);
                    }

                    if let Some(param_ty) = declared_param {
                        Self::infer_generic_bindings(param_ty, &arg.ty, &mut substitutions);
                    }

                    args.push(arg);
                }

                //
                // infer generics from the expected result type.
                //
                // this is what makes:
                //
                //     const ptr: NonNull<i32> = NonNull::dangling();
                //
                // infer:
                //
                //     NonNull<T> == NonNull<i32>
                //     T = i32
                //
                if let Some(expected_ty) = expected {
                    Self::infer_generic_bindings(&return_type, expected_ty, &mut substitutions);
                }

                //
                // store generic arguments in declaration order so the
                // monomorphizer sees exactly the specialization it expects.
                //
                let resolved_generic_args: Vec<IRType> = function_gp.iter().map(|name| {
                    substitutions.get(name).cloned().unwrap_or_else(|| {
                        IRType::GENERIC(
                            name.clone()
                        )
                    })
                }).collect();

                //
                // the HIR expression itself also needs its concrete inferred
                // result type now, because semantic checking happens before
                // monomorphization.
                //
                let resolved_return_type = return_type.substitute(&substitutions);

                if let Some(kind) = intrinsic {
                    return Ok(HIRExpr {
                        kind: HIRExprKind::IntrinsicCall {
                            callee: actual_def_id,
                            kind,
                            args,
                            type_args:
                            resolved_generic_args,
                        },

                        ty: resolved_return_type,
                        span,
                    });
                }

                Ok(HIRExpr {
                    kind: HIRExprKind::Call {
                        callee: actual_def_id,
                        args,
                        generic_args:
                        resolved_generic_args,
                    },

                    ty: resolved_return_type,
                    span,
                })
            }

            _ => unreachable!()
        }
    }

    pub(crate) fn infer_generic_bindings(pattern: &IRType, actual: &IRType, bindings: &mut HashMap<String, IRType>)
    {
        match (pattern, actual) {
            //
            // T = concrete type
            //
            (IRType::GENERIC(name), actual) => {
                bindings.entry(name.clone()).or_insert_with(|| { actual.clone() });
            }

            //
            // Foo<T> = Foo<i32>
            //
            (
                IRType::GENERIC_INSTANCE(
                    pattern_base,
                    pattern_args,
                ),
                IRType::GENERIC_INSTANCE(
                    actual_base,
                    actual_args,
                ),
            ) if pattern_base == actual_base && pattern_args.len() == actual_args.len() => 
            {
                for (pattern, actual) in pattern_args.iter().zip(actual_args.iter())
                {
                    Self::infer_generic_bindings(pattern, actual, bindings);
                }
            }

            (IRType::POINTER(pattern), IRType::POINTER(actual)) | 
            (IRType::CONST_POINTER(pattern), IRType::CONST_POINTER(actual)) | 
            (IRType::REF(pattern), IRType::REF(actual)) | 
            (IRType::CONST_REF(pattern), IRType::CONST_REF(actual)) | 
            (IRType::SLICE(pattern), IRType::SLICE(actual)) => 
            {
                Self::infer_generic_bindings(pattern, actual, bindings);
            }

            (IRType::ARRAY(pattern, pattern_len), IRType::ARRAY(actual, actual_len)) 
                if pattern_len == actual_len => 
            {
                Self::infer_generic_bindings(pattern, actual, bindings);
            }

            _ => {}
        }
    }

    pub(crate) fn check_call_arity(&self, callable: &str, expected: usize, found: usize, span: Span) -> Result<(), HydraError> 
    {
        if expected == found {
            return Ok(());
        }

        Err(self.error(
            "S018",
            format!(
                "{} expected {} argument{}, found {}",
                callable,
                expected,
                if expected == 1 { "" } else { "s" },
                found,
            ),
            span,
        ))
    }
}
