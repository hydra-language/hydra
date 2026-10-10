use ir::{Constant, types::Type};

use mir::{
    MIRFunction,
    Operand,
    ProjectionElem,
};

use crate::CodeGen;

impl<'c> CodeGen<'c> {

    pub(crate) fn get_place_type(&self, place: &mir::Place, mir_fn: &MIRFunction) -> Type {
        let mut ty = mir_fn.locals[place.local.0].ty.clone();

        for projection in &place.projection {
            ty = match projection {
                ProjectionElem::Deref => {
                    match ty {
                        Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) | Type::CONST_POINTER(inner) => *inner,
                        other => other,
                    }
                }

                ProjectionElem::Index(_) => {
                    match ty {
                        Type::ARRAY(inner, _) | Type::INFERRED_ARRAY(inner) | Type::SLICE(inner) => *inner,
                        other => other,
                    }
                }

                ProjectionElem::Field(index) => {
                    let mut base_ty = ty;

                    while let Type::REF(inner) | Type::CONST_REF(inner) |
                    Type::POINTER(inner) | Type::CONST_POINTER(inner) = base_ty
                    {
                        base_ty = *inner;
                    }

                    match base_ty {
                        Type::OPTIONAL(inner) => {
                            match *index {
                                0 => Type::BOOL,
                                1 => *inner,
                                _ => Type::VOID,
                            }
                        }

                        Type::STRUCT(type_ref) => {
                            let fields = self.hir_context.get_struct_fields_ref(type_ref.def_id);
                            fields[*index].1.clone()
                        }

                        other => other,
                    }
                }
            };
        }

        ty
    }

    pub(crate) fn get_operand_type(&self, op: &Operand, mir_fn: &MIRFunction) -> Type {
        match op {
            Operand::Const(c) => match c {
                Constant::Int(_, ty) => ty.clone(),
                Constant::Float(_, ty) => ty.clone(),
                Constant::Bool(_) => Type::BOOL,
                Constant::Char(_) => Type::CHAR,

                Constant::String(_) => {
                    Type::CONST_REF(Box::new(Type::STRING))
                }
            },

            Operand::Copy(place) |
            Operand::Move(place) => {
                self.get_place_type(place, mir_fn)
            }
        }
    }
}
