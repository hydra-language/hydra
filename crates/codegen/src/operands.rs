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

                ProjectionElem::Field { ty: field_ty, .. } => field_ty.clone(),
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
