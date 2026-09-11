use super::super::Analyzer;

use crate::utils;

use errors::error::{
    HydraError,
    Span,
};
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
                let mut arr = self.lower_expr_with_type(array, None)?;

                let idx_expr = self.lower_expr_with_type(
                    index,
                    Some(&IRType::USIZE),
                )?;

                if !idx_expr.ty.is_numeric() {
                    return Err(self.error(
                        "S001",
                        format!(
                            "index must be numeric, found {}",
                            idx_expr.ty
                        ),
                        span,
                    ));
                }

                //
                // indexing auto-dereferences references:
                //
                //     &[T]     -> [T]
                //     &mut [T] -> [T]
                //
                // do this in HIR rather than merely inspecting the inner type,
                // because MIR needs the Deref projection too.
                //
                loop {
                    let inner = match &arr.ty {
                        IRType::REF(inner) | IRType::CONST_REF(inner) => {
                            Some(inner.as_ref().clone())
                        }

                        _ => None,
                    };

                    let Some(inner_ty) = inner else {
                        break;
                    };

                    arr = HIRExpr {
                        kind: HIRExprKind::Dereference {
                            target: Box::new(arr),
                        },
                        ty: inner_ty,
                        span,
                    };
                }

                match arr.ty.clone() {
                    IRType::ARRAY(inner, size) => {
                        if let HIRExprKind::IntLiteral(idx_val) = &idx_expr.kind {
                            if *idx_val < 0 || *idx_val >= size as i64 {
                                return Err(self.error(
                                    "S008",
                                    format!(
                                        "index out of bounds: len is {} but index is {}",
                                        size,
                                        idx_val
                                    ),
                                    span,
                                ));
                            }
                        }

                        Ok(HIRExpr {
                            kind: HIRExprKind::ArrayAccess {
                                array: Box::new(arr),
                                index: Box::new(idx_expr),
                            },
                            ty: *inner,
                            span,
                        })
                    }

                    IRType::INFERRED_ARRAY(inner) | IRType::SLICE(inner) => {
                        Ok(HIRExpr {
                            kind: HIRExprKind::ArrayAccess {
                                array: Box::new(arr),
                                index: Box::new(idx_expr),
                            },
                            ty: *inner,
                            span,
                        })
                    }

                    _ => Err(self.error(
                        "S003",
                        format!(
                            "type '{}' cannot be indexed",
                            arr.ty
                        ),
                        span,
                    )),
                }
            }

            ASTExpr::StructInitializer { name, fields, .. } => {
                let name_id = utils::get_expr_id(name);
                let def_id = self.name_resolver.get_resolution(name_id)
                    .ok_or_else(|| self.error("S002", "undefined struct", span))?;

                let info = self.context.get_def(def_id).unwrap();
                let absolute_struct_name = info.absolute_path.join("::");

                let def_fields = match &info.kind {
                    DefKind::Struct { fields, .. } => fields.clone(),
                    _ => return Err(self.error("S002", "not a struct", span)),
                }; 

                let mut lowered_values = Vec::new();

                for (def_name, def_type, is_const) in &def_fields { 
                    if *is_const { continue; }

                    if let Some((_, value_node)) = fields.iter().find(|(f_token, _)| f_token.lexeme == *def_name) {
                        let mut val = self.lower_expr_with_type(value_node, Some(def_type))?;
                        val = self.coerce_primitive(val, def_type);

                        if !self.check_type_compatibility(def_type, &val.ty) {
                            return Err(self.error("S001", format!("field '{}' expected {}, found {}", def_name, def_type, val.ty), span));
                        }
                        lowered_values.push(val);
                    } else {
                        return Err(self.error("S005", format!("missing field '{}'", def_name), span));
                    }
                }

                Ok(HIRExpr {
                    kind: HIRExprKind::StructInit { def_id, values: lowered_values },
                    ty: IRType::STRUCT(TypeRef::new(def_id, absolute_struct_name)),
                    span,
                })
            },

            _ => unreachable!("lower_aggregate_expr() called with non-aggregate expression")
        }
    }
}
