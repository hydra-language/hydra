use super::Analyzer;
use errors::error::{Span, HydraError};
use parser::ast::Type as ASTType;
use ir::types::Type as IRType;
use ir::context::DefKind;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_type(&mut self, node: &ASTType) -> Result<IRType, HydraError> {
        let span = crate::utils::get_type_span(node);

        match node {
            ASTType::Path { id, segments } => {
                let name = segments[0].lexeme.as_str();

                if segments.len() == 1 && name == "Self" {
                    if let Some(self_ty) = &self.current_self_type {
                        return Ok(self_ty.clone());
                    }
                }

                match name {
                    "i8" => return Ok(IRType::I8), 
                    "i16" => return Ok(IRType::I16), 
                    "i32" => return Ok(IRType::I32), 
                    "i64" => return Ok(IRType::I64),
                    "isize" => return Ok(IRType::ISIZE), 
                    "u8" => return Ok(IRType::U8), 
                    "u16" => return Ok(IRType::U16), 
                    "u32" => return Ok(IRType::U32),
                    "u64" => return Ok(IRType::U64), 
                    "usize" => return Ok(IRType::USIZE), 
                    "f32" => return Ok(IRType::F32), 
                    "f64" => return Ok(IRType::F64),
                    "char" => return Ok(IRType::CHAR), 
                    "bool" => return Ok(IRType::BOOL), 
                    "void" => return Ok(IRType::VOID),
                    _ => {}
                }

                let def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", format!("unresolved type `{}`", segments[0].lexeme), span))?;
                
                let info = self.context.get_def(def_id).unwrap();

                match info.kind {
                    DefKind::Struct { .. } => Ok(IRType::STRUCT(info.absolute_path.join("::"))),
                    DefKind::GenericParam => {
                        Ok(IRType::GENERIC(
                            info.name.clone()
                        ))
                    }
                    _ => Err(self.error("T001", format!("`{}` is not a type", info.name), span))
                }
            },

            ASTType::Borrow { is_mut, inner, .. } => {
                let inner_type = self.lower_type(inner)?;
                if *is_mut {
                    Ok(IRType::REF(Box::new(inner_type)))
                } else {
                    Ok(IRType::CONST_REF(Box::new(inner_type)))
                }
            },

            ASTType::RawPointer { is_mut, inner, .. } => {
                let inner_type = self.lower_type(inner)?;

                if *is_mut {
                    Ok(IRType::POINTER(Box::new(inner_type)))
                } else {
                    Ok(IRType::CONST_POINTER(Box::new(inner_type)))
                }
            }

            ASTType::Generic { base, args, .. } => {
                let base_ty = self.lower_type(base)?;
                let mut lowered_args = Vec::new();
                for arg in args {
                    lowered_args.push(self.lower_type(arg)?);
                }
                Ok(IRType::GENERIC_INSTANCE(Box::new(base_ty), lowered_args))
            },

            ASTType::Array { element_type, size, .. } => {
                let inner = self.lower_type(element_type)?;
                
                // quickly extract length from simple literal 
                if let parser::ast::Expr::Literal { token, .. } = &**size {
                    if let lexer::TokenType::IntLiteral(n) = token.token_type {
                        return Ok(IRType::ARRAY(Box::new(inner), n as usize));
                    } else if let lexer::TokenType::ANYSIZE = token.token_type {
                        return Ok(IRType::INFERRED_ARRAY(Box::new(inner)));
                    }
                }
                
                Err(self.error("S003", "array size must be an integer literal or 'anysize'", span))
            },

            ASTType::Slice { element_type, .. } => {
                let inner = self.lower_type(element_type)?;
                Ok(IRType::SLICE(Box::new(inner)))
            }
        }
    }

    pub(crate) fn get_type_size(&self, ty: &IRType) -> Result<i64, HydraError> {
        match ty {
            IRType::I8 | IRType::U8 | IRType::BOOL | IRType::CHAR => Ok(1),
            IRType::I16 | IRType::U16 => Ok(2),
            IRType::I32 | IRType::U32 | IRType::F32 => Ok(4),
            IRType::I64 | IRType::U64 | IRType::F64 | IRType::USIZE | IRType::ISIZE => Ok(8),
            
            IRType::POINTER(_) | IRType::CONST_POINTER(_) | IRType::REF(_) | IRType::CONST_REF(_) => Ok(8),
            
            IRType::ARRAY(inner, len) => {
                let inner_size = self.get_type_size(inner)?;
                Ok(inner_size * (*len as i64))
            },

            IRType::SLICE(_) => Ok(16),
            
            IRType::STRUCT(name) => {
                if let Some(def_id) = self.global_symbols.get(&name.split("::").map(|s| s.to_string()).collect::<Vec<_>>()) {
                    if let Some(info) = self.context.get_def(*def_id) {
                        if let DefKind::Struct { fields, .. } = &info.kind {
                            let mut total_size = 0;
                            for (_, field_ty, _) in fields {
                                total_size += self.get_type_size(field_ty)?;
                            }
                            return Ok(total_size);
                        }
                    }
                }
                Err(self.error("S002", format!("cannot determine size of undefined struct '{}'", name), Span::default()))
            },
            
            IRType::VOID => Ok(0),
            _ => Err(self.error("S006", format!("cannot determine size of type '{}'", ty), Span::default())),
        }
    }

    pub(crate) fn check_and_promote_int_literal(&self, lit_val: i64, target_ty: &IRType) -> bool {
        match target_ty {
            IRType::I8  => lit_val >= (i8::MIN as i64) && lit_val <= (i8::MAX as i64),
            IRType::U8  => lit_val >= 0 && lit_val <= (u8::MAX as i64),
            IRType::I16 => lit_val >= (i16::MIN as i64) && lit_val <= (i16::MAX as i64),
            IRType::U16 => lit_val >= 0 && lit_val <= (u16::MAX as i64),
            IRType::I32 => true,
            IRType::U32 => lit_val >= 0,
            IRType::I64 | IRType::ISIZE => true,
            IRType::U64 | IRType::USIZE => true,
            IRType::F32 | IRType::F64 => true,
            IRType::BOOL => lit_val == 0 || lit_val == 1,
            _ => false, 
        }
    }

    pub(crate) fn check_type_compatibility(&self, target: &IRType, source: &IRType) -> bool {
        if target == source { return true; }

        match (target, source) {
            (IRType::INFERRED_ARRAY(target_inner), IRType::ARRAY(source_inner, _)) => target_inner == source_inner,
            (IRType::SLICE(t_inner), IRType::SLICE(s_inner)) if t_inner == s_inner => true,
            (IRType::REF(t_inner), IRType::REF(s_inner)) if t_inner == s_inner => true,
            (IRType::CONST_REF(t_inner), IRType::REF(s_inner)) if t_inner == s_inner => true,
            _ => false,
        }
    }
}
