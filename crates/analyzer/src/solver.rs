use std::collections::{HashMap, HashSet};

use ir::{context::{DefID, TraitBound, TraitImpl}, types::Type};


pub(crate) struct TraitSolver<'a> {
    trait_impls: &'a [TraitImpl],
    copy_trait: Option<DefID>,
}

impl<'a> TraitSolver<'a> {

    pub fn new(trait_impls: &'a [TraitImpl], copy_trait: Option<DefID>) -> Self {
        Self { trait_impls, copy_trait }
    }

    pub fn type_implements_trait(&self, ty: &Type, trait_def_id: DefID, assumptions: &[TraitBound]) -> bool {
        let mut visiting = HashSet::new();
        self.type_implements_trait_inner(ty, trait_def_id, assumptions, &mut visiting)
    }

    fn type_implements_trait_inner(&self, ty: &Type, trait_def_id: DefID, assumptions: &[TraitBound], visiting: &mut HashSet<(DefID, Type)>) -> bool 
    {
        if assumptions.iter().any(|bound| bound.trait_def_id == trait_def_id && bound.target_type == *ty) {
            return true;
        }

        if Some(trait_def_id) == self.copy_trait && self.builtin_copy_holds(ty, assumptions, visiting) {
            return true;
        }

        let key = (trait_def_id, ty.clone());

        if !visiting.insert(key.clone()) {
            return false;
        }

        let result = self.trait_impls.iter().any(|trait_impl| {
            if trait_impl.trait_def_id != trait_def_id {
                return false;
            }

            let mut substitutions = HashMap::new();

            if !Self::match_trait_impl_target(&trait_impl.target_type, ty, &mut substitutions) {
                return false;
            }

            trait_impl.bounds.iter().all(|bound| {
                let target = bound.target_type.substitute(&substitutions);

                self.type_implements_trait_inner(
                    &target,
                    bound.trait_def_id,
                    assumptions,
                    visiting,
                )
            })
        });

        visiting.remove(&key);
        result
    }

    pub fn trait_impl_is_applicable(&self, trait_impl: &TraitImpl, ty: &Type, assumptions: &[TraitBound]) -> bool {
        let mut substitutions = HashMap::new();

        if !Self::match_trait_impl_target(&trait_impl.target_type, ty, &mut substitutions) {
            return false;
        }

        let mut visiting = HashSet::new();

        trait_impl.bounds.iter().all(|bound| {
            let target = bound.target_type.substitute(&substitutions);

            self.type_implements_trait_inner(
                &target,
                bound.trait_def_id,
                assumptions,
                &mut visiting,
            )
        })
    }

    fn builtin_copy_holds(&self, ty: &Type, assumptions: &[TraitBound], visiting: &mut HashSet<(DefID, Type)>) -> bool {
        let Some(copy_trait) = self.copy_trait else {
            return false;
        };

        match ty {
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::ISIZE |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::USIZE |
            Type::F32 | Type::F64 | Type::BOOL | Type::CHAR => true,

            Type::POINTER(_) | Type::CONST_POINTER(_) |
            Type::REF(_) | Type::CONST_REF(_) => true,

            Type::ARRAY(inner, _) | Type::OPTIONAL(inner) => {
                self.type_implements_trait_inner(
                    inner,
                    copy_trait,
                    assumptions,
                    visiting,
                )
            }

            _ => false,
        }
    }

    fn match_trait_impl_target(pattern: &Type, actual: &Type, substitutions: &mut HashMap<String, Type>) -> bool {
        match (pattern, actual) {
            (Type::GENERIC(name), actual) => {
                if let Some(existing) = substitutions.get(name) {
                    existing == actual
                } else {
                    substitutions.insert(name.clone(), actual.clone());
                    true
                }
            }

            (Type::GENERIC_INSTANCE(pattern_base, pattern_args), Type::GENERIC_INSTANCE(actual_base, actual_args)) => 
            {
                pattern_base.def_id == actual_base.def_id && pattern_args.len() == actual_args.len()
                && pattern_args.iter().zip(actual_args).all(|(pattern, actual)| Self::match_trait_impl_target(pattern, actual, substitutions))
            }

            (Type::ARRAY(pattern, pattern_len), Type::ARRAY(actual, actual_len)) => {
                pattern_len == actual_len && Self::match_trait_impl_target(pattern, actual, substitutions)
            }

            (Type::SLICE(pattern), Type::SLICE(actual)) | (Type::REF(pattern), Type::REF(actual)) | 
            (Type::CONST_REF(pattern), Type::CONST_REF(actual)) | (Type::POINTER(pattern), Type::POINTER(actual)) | 
            (Type::CONST_POINTER(pattern), Type::CONST_POINTER(actual)) | (Type::OPTIONAL(pattern), Type::OPTIONAL(actual)) => 
            {
                Self::match_trait_impl_target(pattern, actual, substitutions)
            }

            _ => pattern == actual,
        }
    }
}
