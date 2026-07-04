use super::Analyzer;
use errors::error::{Span, HydraError};
use lexer::TokenType;
use parser::ast::ASTNode;
use ir::types::Type;
use ir::context::DefKind;

impl Analyzer {

    pub(crate) fn lower_type(&mut self, node: ASTNode) -> Result<Type, HydraError> {
        match node {
            // borrowing (&mut var | &var)
            ASTNode::Reference { is_mut, inner } => {
                let inner_type = self.lower_type(*inner)?;

                if is_mut {
                    Ok(Type::REF(Box::new(inner_type)))
                } else {
                    Ok(Type::CONST_REF(Box::new(inner_type)))
                }
            }

            // raw pointers (*mut T | *const T)
            ASTNode::RawPointer { is_mut, inner } => {
                let inner_type = self.lower_type(*inner)?;

                // NOTE: for now, i am mapping both to Type::POINTER. 
                // when i update the IR to enforce strict pointer math, 
                // i will split this into Type::MUT_POINTER and Type::CONST_POINTER
                let _ = is_mut;

                Ok(Type::POINTER(Box::new(inner_type)))
            }

            ASTNode::GenericType { base, args } => {
                let base_ty = self.lower_type(*base)?;

                let mut lowered_args = Vec::new();
                for arg in args {
                    lowered_args.push(self.lower_type(arg)?);
                }

                Ok(Type::GENERIC_INSTANCE(Box::new(base_ty), lowered_args))
            }

            ASTNode::TypeIdentifier { type_token } => {
                let name = type_token.lexeme.to_string();

                if self.current_generics.contains(&name) {
                    return Ok(Type::GENERIC(name));
                }

                match name.as_str() {
                    "i8" => Ok(Type::I8), 
                    "i16" => Ok(Type::I16), 
                    "i32" => Ok(Type::I32), 
                    "i64" => Ok(Type::I64),
                    "isize" => Ok(Type::ISIZE), 
                    "u8" => Ok(Type::U8), 
                    "u16" => Ok(Type::U16), 
                    "u32" => Ok(Type::U32),
                    "u64" => Ok(Type::U64), 
                    "usize" => Ok(Type::USIZE), 
                    "f32" => Ok(Type::F32), 
                    "f64" => Ok(Type::F64),
                    "char" => Ok(Type::CHAR), 
                    "bool" => Ok(Type::BOOL), 
                    "void" => Ok(Type::VOID),

                    _ => {
                        let abs_path = self.scope.resolve_path(&[name], &self.context);
                        Ok(Type::STRUCT(abs_path.join("::")))
                    }
                }
            },

            ASTNode::ArrayType { element_type, size, .. } => {
                let inner = self.lower_type(*element_type)?;
                let size_token = self.get_token_from_node(&size);
                match size_token.token_type {
                    TokenType::IntLiteral(n) => Ok(Type::ARRAY(Box::new(inner), n as usize)),
                    TokenType::ANYSIZE => Ok(Type::INFERRED_ARRAY(Box::new(inner))),
                    _ => Err(self.error("S003", "array size must be an integer or 'anysize'", size_token.span))
                }
            },

            _ => Err(self.error("S006", format!("invalid type: {:?}", node), self.get_token_from_node(&node).span))
        }
    }

    pub(crate) fn get_type_size(&self, ty: &Type) -> Result<i64, HydraError> {
        match ty {
            Type::I8 | Type::U8 | Type::BOOL | Type::CHAR => Ok(1),
            Type::I16 | Type::U16 => Ok(2),
            Type::I32 | Type::U32 | Type::F32 => Ok(4),
            Type::I64 | Type::U64 | Type::F64 | Type::USIZE | Type::ISIZE => Ok(8),
            
            Type::POINTER(_) | Type::REF(_) | Type::CONST_REF(_) => Ok(8),
            
            Type::ARRAY(inner, len) => {
                let inner_size = self.get_type_size(inner)?;
                Ok(inner_size * (*len as i64))
            },
            
            Type::STRUCT(name) => {
                if let Some(def_id) = self.scope.resolve(name, &self.context) {
                    if let Some(info) = self.context.get_def(def_id) {
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
            
            Type::VOID => Ok(0),
            _ => Err(self.error("S006", format!("cannot determine size of type '{}'", ty), Span::default())),
        }
    }

    pub(crate) fn check_and_promote_int_literal(&self, lit_val: i64, target_ty: &Type) -> bool {
        match target_ty {
            Type::I8  => lit_val >= (i8::MIN as i64) && lit_val <= (i8::MAX as i64),
            Type::U8  => lit_val >= 0 && lit_val <= (u8::MAX as i64),
            Type::I16 => lit_val >= (i16::MIN as i64) && lit_val <= (i16::MAX as i64),
            Type::U16 => lit_val >= 0 && lit_val <= (u16::MAX as i64),
            Type::I32 => true,
            Type::U32 => lit_val >= 0,
            Type::I64 | Type::ISIZE => true,
            Type::U64 | Type::USIZE => true,
            Type::F32 | Type::F64 => true,
            Type::BOOL => lit_val == 0 || lit_val == 1,
            _ => false, 
        }
    }

    pub(crate) fn check_type_compatibility(&self, target: &Type, source: &Type) -> bool {
        if target == source { 
            return true;
        }

        match (target, source) {
            (Type::INFERRED_ARRAY(target_inner), Type::ARRAY(source_inner, _)) => target_inner == source_inner,
            (Type::REF(t_inner), Type::REF(s_inner)) if t_inner == s_inner => true,
            (Type::CONST_REF(t_inner), Type::REF(s_inner)) if t_inner == s_inner => true,
            _ => false,
        }
    }
}
