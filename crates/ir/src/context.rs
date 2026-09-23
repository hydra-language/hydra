use std::collections::{HashMap, HashSet};
use errors::error::Span;
use parser::Annotation;
use crate::intrinsic::IntrinsicKind;
use crate::types::Type;
use crate::Constant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefID(pub usize);

impl std::fmt::Display for DefID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DefID({})", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeID(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitBound {
    pub target_type: Type,
    pub trait_def_id: DefID,
}

#[derive(Debug, Clone)]
pub enum DefKind {

    Variable {
        ty: Type,
        is_mutable: bool,
    },

    Function {
        params: Vec<Type>,
        annotations: Vec<Annotation>,
        return_type: Type,

        // flattened order:
        //      [owner_generics, function generics]
        generic_params: Vec<String>,

        // number of entries at the beginning of generic_params
        // inherited from the extension owner
        owner_generic_count: usize,
        
        trait_bounds: Vec<TraitBound>,
        intrinsic: Option<IntrinsicKind>,
    },


    Struct {
        fields: Vec<(String, Type, bool)>,
        generic_params: Vec<String>,
    },

    Trait {
        methods: Vec<DefID>,
    },

    Alias {
        target_path: Vec<String>,
    },

    Constant {
        ty: Type,
        value: Constant,
    },

    GenericParam,
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub span: Span,
    pub absolute_path: Vec<String>,
    pub kind: DefKind,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct TraitImpl {
    pub trait_def_id: DefID,
    pub target_type: Type,
    pub bounds: Vec<TraitBound>,
    pub methods: HashMap<String, DefID>,
}

#[derive(Default, Debug)]
pub struct HIRContext {
    pub definitions: HashMap<DefID, SymbolInfo>,
    pub types: HashMap<TypeID, Type>,
    next_def_id: usize,
    pub trait_impls: Vec<TraitImpl>,
    pub display_names: HashMap<DefID, String>,
}

impl HIRContext {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_def(&mut self, info: SymbolInfo) -> DefID {
        let id = DefID(self.next_def_id);

        self.next_def_id += 1;
        self.definitions.insert(id, info);

        id
    }

    pub fn get_def(&self, id: DefID) -> Option<&SymbolInfo> {
        self.definitions.get(&id)
    }

    pub fn update_def(&mut self, id: DefID, info: SymbolInfo) {
        self.definitions.insert(id, info);
    }

    pub fn get_struct_fields(&self, id: DefID) -> Vec<(String, Type, bool)> {
        if let Some(info) = self.get_def(id) {
            if let DefKind::Struct { fields, .. } = &info.kind {
                return fields.clone();
            }
        }

        panic!("struct fields not found for {}", id);
    }

    pub fn find_struct_by_name(&self, name: &str) -> Option<DefID> {
        self.definitions.iter().find_map(|(id, info)| {
            let path_str = info.absolute_path.join("::");

            // This ensures "Shape" matches even if stored as an absolute path
            if path_str == name || info.name == name || info.absolute_path.last().map(|s| s.as_str()) == Some(name) {
                if let DefKind::Struct { .. } = &info.kind {
                    return Some(*id);
                }
            }
            None
        })
    }

    pub fn find_function_by_name(&self, name: &str) -> Option<DefID> {
        self.definitions.iter().find_map(|(id, info)| {
            if info.absolute_path.join("::") == name {
                if let DefKind::Function { .. } = &info.kind {
                    return Some(*id);
                }
            }
            None
        })
    }

    pub fn register_trait_impl(&mut self, trait_impl: TraitImpl) {
        self.trait_impls.push(trait_impl);
    }

    pub fn set_display_name(&mut self, def_id: DefID, name: String) {
        self.display_names.insert(def_id, name);
    }

    pub fn get_display_name(&self, def_id: DefID) -> Option<&str> {
        self.display_names.get(&def_id).map(String::as_str)
    }

    pub fn get_exact_trait_impl(&self, trait_def_id: DefID, target_type: &Type) -> Option<&TraitImpl> {
        self.trait_impls
            .iter()
            .find(|impl_| impl_.trait_def_id == trait_def_id && &impl_.target_type == target_type)
    }

    pub fn find_trait_by_path(&self, path: &str) -> Option<DefID> {
        self.definitions.iter().find_map(|(def_id, info)| {
            if info.absolute_path.join("::") == path && matches!(info.kind, DefKind::Trait { .. }) {
                Some(*def_id)
            } else {
                None
            }
        })
    }

    pub fn get_exact_trait_method(&self, trait_def_id: DefID, target_type: &Type, method_name: &str) -> Option<DefID> {
        self.get_exact_trait_impl(trait_def_id, target_type)
            .and_then(|trait_impl| trait_impl.methods.get(method_name))
            .copied()
    }

    pub fn get_trait_method(&self, trait_def_id: DefID, method_name: &str) -> Option<DefID> {
        let trait_info = self.get_def(trait_def_id)?;

        let DefKind::Trait { methods } = &trait_info.kind else {
            return None;
        };

        methods.iter().copied().find(|method_def_id| {
            self.get_def(*method_def_id)
                .map(|method| method.name.as_str())
            == Some(method_name)
        })
    }

    pub fn trait_impl_is_applicable(&self, trait_impl: &TraitImpl, target_type: &Type, assumptions: &[TraitBound]) -> bool 
    {
        let mut substitutions = HashMap::new();

        if !Self::match_trait_impl_target(&trait_impl.target_type, target_type, &mut substitutions) {
            return false;
        }

        let mut visiting = HashSet::new();

        trait_impl.bounds.iter().all(|bound| {
            let target = bound.target_type.substitute(&substitutions);

            self.type_implements_trait_inner(&target, bound.trait_def_id, assumptions, &mut visiting)
        })
    }

    pub fn type_implements_trait(&self, target_type: &Type, trait_def_id: DefID, assumptions: &[TraitBound]) -> bool 
    {
        let mut visiting = HashSet::new();

        self.type_implements_trait_inner(target_type, trait_def_id, assumptions, &mut visiting)
    }

    fn type_implements_trait_inner(&self, target_type: &Type, trait_def_id: DefID, assumptions: &[TraitBound], visiting: &mut HashSet<(DefID, Type)>) 
        -> bool 
    {
        if assumptions.iter().any(|bound| {
            bound.trait_def_id == trait_def_id
            && bound.target_type == *target_type
        }) 
        {
            return true;
        }

        let key = (trait_def_id, target_type.clone());

        if !visiting.insert(key.clone()) {
            return false;
        }

        let result = self.trait_impls.iter().any(|trait_impl| {
            if trait_impl.trait_def_id != trait_def_id {
                return false;
            }

            let mut substitutions = HashMap::new();

            if !Self::match_trait_impl_target(
                &trait_impl.target_type,
                target_type,
                &mut substitutions,
            ) {
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
                pattern_base.def_id == actual_base.def_id
                && pattern_args.len() == actual_args.len()
                && pattern_args
                    .iter()
                    .zip(actual_args)
                    .all(|(pattern, actual)| {
                        Self::match_trait_impl_target(
                            pattern,
                            actual,
                            substitutions,
                        )
                    })
            }

            (Type::ARRAY(pattern, pattern_len), Type::ARRAY(actual, actual_len)) => {
                pattern_len == actual_len
                && Self::match_trait_impl_target(
                    pattern,
                    actual,
                    substitutions,
                )
            }

            (Type::SLICE(pattern), Type::SLICE(actual)) |
            (Type::REF(pattern), Type::REF(actual)) |
            (Type::CONST_REF(pattern), Type::CONST_REF(actual)) |
            (Type::POINTER(pattern), Type::POINTER(actual)) |
            (Type::CONST_POINTER(pattern), Type::CONST_POINTER(actual)) |
            (Type::OPTIONAL(pattern), Type::OPTIONAL(actual)) => {
                Self::match_trait_impl_target(
                    pattern,
                    actual,
                    substitutions,
                )
            }

            _ => pattern == actual,
        }
    }
}
