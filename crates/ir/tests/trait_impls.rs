use std::collections::HashMap;

use ir::{
    context::{DefID, SemanticContext, TraitImpl},
    types::Type,
};

fn trait_impl(trait_def_id: DefID, target_type: Type) -> TraitImpl {
    TraitImpl {
        trait_def_id,
        trait_args: Vec::new(),
        target_type,
        bounds: Vec::new(),
        methods: HashMap::new(),
    }
}

#[test]
fn trait_impl_index_returns_only_impls_for_requested_trait() {
    let mut context = SemanticContext::new();

    let first_trait = DefID(1);
    let second_trait = DefID(2);

    context.register_trait_impl(trait_impl(first_trait, Type::I32));
    context.register_trait_impl(trait_impl(second_trait, Type::U8));
    context.register_trait_impl(trait_impl(first_trait, Type::I64));

    let targets = context
        .trait_impls_for(first_trait)
        .map(|trait_impl| trait_impl.target_type.clone())
        .collect::<Vec<_>>();

    assert_eq!(targets, vec![Type::I32, Type::I64]);

    let targets = context
        .trait_impls_for(second_trait)
        .map(|trait_impl| trait_impl.target_type.clone())
        .collect::<Vec<_>>();

    assert_eq!(targets, vec![Type::U8]);
}
