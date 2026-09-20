use std::collections::HashMap;

use ir::types::Type;

#[test]
fn optional_type_formats_and_mangles() {
    let ty = Type::OPTIONAL(Box::new(Type::I32));

    assert_eq!(ty.to_string(), "i32?");
    assert_eq!(ty.mangle(), "opt_i32");
}

#[test]
fn optional_type_substitutes_and_tracks_generics() {
    let ty = Type::OPTIONAL(Box::new(Type::GENERIC("T".to_string())));
    assert!(ty.contains_generic());

    let substitutions = HashMap::from([("T".to_string(), Type::I32)]);
    let substituted = ty.substitute(&substitutions);

    assert_eq!(substituted, Type::OPTIONAL(Box::new(Type::I32)));
    assert!(!substituted.contains_generic());
}
