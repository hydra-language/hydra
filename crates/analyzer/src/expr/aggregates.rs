use super::super::Analyzer;

use std::collections::HashMap;

use crate::utils;
use errors::error::{HydraError, Span };

use ir::context::DefKind;
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::{Type as IRType, TypeRef};

use parser::ast::Expr as ASTExpr;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_aggregate_expr(&mut self, node: &ASTExpr, expected: Option<&IRType>, span: Span) 
        -> Result<HIRExpr, HydraError> 
    {
        match node {
            ASTExpr::ArrayInitializer { elements, .. } => {
                let mut ir_elements = Vec::new();
                let inner_expected = match expected {
                    Some(IRType::ARRAY(inner, _)) => Some(&**inner),
                    _ => None,
                };
                for element in elements {
                    let mut ir_element = self.lower_expr_with_type(element, inner_expected)?;
                    if let Some(target) = inner_expected {
                        ir_element = self.coerce_primitive(ir_element, target);
                    }
                    ir_elements.push(ir_element);
                }

                let element_type = if let Some(target) = inner_expected {
                    target.clone()
                } else {
                    ir_elements.first().map(|e| e.ty.clone()).ok_or_else(|| self.error("S007", "cannot infer type of array", span))?
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::ArrayInit { elements: ir_elements },
                    ty: IRType::ARRAY(Box::new(element_type), elements.len()),
                    span,
                })
            },

            ASTExpr::SliceInitializer { elements, .. } => {
                let expected_slice = match expected {
                    Some(IRType::REF(inner)) => {
                        match inner.as_ref() {
                            IRType::SLICE(element) => {
                                Some((true, element.as_ref()))
                            }

                            _ => None,
                        }
                    }

                    Some(IRType::CONST_REF(inner)) => {
                        match inner.as_ref() {
                            IRType::SLICE(element) => {
                                Some((false, element.as_ref()))
                            }

                            _ => None,
                        }
                    }

                    _ => None,
                };

                let expected_element = expected_slice.map(|(_, element)| element);

                let mut ir_elements = Vec::new();

                for element in elements {
                    let mut value = self.lower_expr_with_type(element, expected_element)?;

                    if let Some(target) = expected_element {
                        value = self.coerce_primitive(value, target);
                    }

                    ir_elements.push(value);
                }

                let (is_mut, element_ty) = if let Some((is_mut, expected_element)) = expected_slice {
                    (is_mut, expected_element.clone())
                } else {
                    (false, ir_elements.first().map(|e| e.ty.clone()).ok_or_else(|| {
                        self.error(
                            "S007",
                            "cannot infer type of empty slice",
                            span,
                        )})?
                    )
                };

                let slice_ty = IRType::SLICE(Box::new(element_ty));

                let ty = if is_mut {
                    IRType::REF(Box::new(slice_ty))
                } else {
                    IRType::CONST_REF(Box::new(slice_ty))
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::SliceInit {
                        elements: ir_elements,
                    },
                    ty,
                    span,
                })
            }

            ASTExpr::ArrayAccess { array, index, .. } => {
                let array = self.lower_expr_with_type(array, None)?;
                self.lower_array_access_from_base(array, index, span)
            }

            ASTExpr::StructInitializer { name, fields, .. } => {
                let name_id = utils::get_expr_id(name);

                let def_id = self.name_resolver.get_resolution(name_id).ok_or_else(|| self.error(
                    "S002",
                    "undefined struct",
                    span,
                ))?;

                let info = self.context.get_def(def_id).unwrap().clone();
                let absolute_struct_name = info.absolute_path.join("::");

                let (def_fields, generic_params) = match &info.kind {
                    DefKind::Struct { fields, generic_params } => {
                        (fields.clone(), generic_params.clone())
                    }

                    _ => {
                        return Err(self.error("S002", "not a struct", span));
                    }
                };

                let type_ref = TypeRef::new(def_id, absolute_struct_name.clone());

                let mut substitutions = HashMap::<String, IRType>::new();

                //
                // if the surrounding context already tells us which generic
                // instantiation this is, seed the generic substitutions from it
                //
                //     const wrapper: Wrapper<i32> = Wrapper {
                //         .value = 42,
                //     };
                //
                // gives:
                //
                //     T = i32
                //
                if let Some(IRType::GENERIC_INSTANCE(expected_base, expected_args)) = expected {
                    if expected_base.def_id == def_id && expected_args.len() == generic_params.len() {
                        for (generic_param, expected_arg) in generic_params.iter().zip(expected_args.iter()) {
                            substitutions.insert(generic_param.clone(), expected_arg.clone());
                        }
                    }
                }

                let mut lowered_values = Vec::new();

                for (def_name, def_type, is_pub) in &def_fields {
                    if !self.can_access_struct_field(&info, *is_pub) {
                        return Err(self.error(
                            "S019",
                            format!(
                                "field `{}` of struct `{}` is private",
                                def_name,
                                absolute_struct_name,
                            ),
                            span,
                        ));
                    }

                    let Some((_, value_node)) = fields.iter().find(|(field_token, _)| field_token.lexeme == *def_name) else 
                    {
                        return Err(self.error(
                            "S005",
                            format!(
                                "missing field '{}'",
                                def_name,
                            ),
                            span,
                        ));
                    };

                    //
                    // substitute any generic arguments we already know
                    //
                    // For Wrapper<i32>:
                    //
                    //     T -> i32
                    //
                    let mut field_type = def_type.substitute(&substitutions);

                    //
                    // only pass a concrete expectation downward. passing an unresolved
                    // T as an expected type doesn't help literals or nested expressions
                    //
                    let field_expected = if field_type.contains_generic() {
                        None
                    } else {
                        Some(&field_type)
                    };

                    let mut val = self.lower_expr_with_type(
                        value_node,
                        field_expected,
                    )?;

                    //
                    // if this struct didn't have an expected type, infer generic
                    // arguments from the field itself
                    //
                    //     Wrapper {
                    //         .value = 42,
                    //     }
                    //
                    // infers:
                    //
                    //     T = i32
                    //
                    Self::infer_generic_bindings(def_type, &val.ty, &mut substitutions);

                    //
                    // inference may have made this field concrete
                    //
                    field_type = def_type.substitute(&substitutions);

                    val = self.coerce_primitive(val, &field_type);

                    if !self.check_type_compatibility(&field_type, &val.ty) {
                        return Err(self.error(
                            "S001",
                            format!(
                                "field '{}' expected {}, found {}",
                                def_name,
                                field_type,
                                val.ty,
                            ),
                            span,
                        ));
                    }

                    lowered_values.push(val);
                }

                let ty = if generic_params.is_empty() {
                    IRType::STRUCT(type_ref)
                } else {
                    let generic_args = generic_params.iter().map(|name| {
                        substitutions.get(name).cloned().unwrap_or_else(|| IRType::GENERIC(name.clone()))
                    }).collect();

                    IRType::GENERIC_INSTANCE(type_ref, generic_args)
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::StructInit {
                        def_id,
                        values: lowered_values,
                    },
                    ty,
                    span,
                })
            }

            _ => unreachable!("lower_aggregate_expr() called with non-aggregate expression")
        }
    }

    pub(crate) fn lower_array_access_from_base(&mut self, mut array: HIRExpr, index: &ASTExpr, span: Span) -> Result<HIRExpr, HydraError> 
    {
        let index_expr = self.lower_expr_with_type(index, Some(&IRType::USIZE))?;

        if !self.is_int_type(&index_expr.ty) {
            return Err(self.error(
                "S001",
                format!(
                    "index must be an integer, found {}",
                    index_expr.ty,
                ),
                span,
            ));
        }

        loop {
            let inner = match &array.ty {
                IRType::REF(inner) | IRType::CONST_REF(inner) => {
                    Some(inner.as_ref().clone())
                }

                _ => None,
            };

            let Some(inner_ty) = inner else {
                break;
            };

            array = HIRExpr {
                kind: HIRExprKind::Dereference {
                    target: Box::new(array),
                },
                ty: inner_ty,
                span,
            };
        }

        match array.ty.clone() {
            IRType::ARRAY(inner, size) => {
                if let HIRExprKind::IntLiteral(index_value) = &index_expr.kind {
                    if *index_value < 0 || *index_value >= size as i64 {
                        return Err(self.error(
                            "S008",
                            format!(
                                "index out of bounds: len is {} but index is {}",
                                size,
                                index_value,
                            ),
                            span,
                        ));
                    }
                }

                Ok(HIRExpr {
                    kind: HIRExprKind::ArrayAccess {
                        array: Box::new(array),
                        index: Box::new(index_expr),
                    },
                    ty: *inner,
                    span,
                })
            }

            IRType::INFERRED_ARRAY(inner) | IRType::SLICE(inner) => {
                Ok(HIRExpr {
                    kind: HIRExprKind::ArrayAccess {
                        array: Box::new(array),
                        index: Box::new(index_expr),
                    },
                    ty: *inner,
                    span,
                })
            }

            _ => {
                Err(self.error(
                    "S003",
                    format!(
                        "type '{}' cannot be indexed",
                        array.ty,
                    ),
                    span,
                ))
            }
        }
    }
}
