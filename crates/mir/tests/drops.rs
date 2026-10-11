use std::collections::HashMap;

use errors::error::Span;
use ir::{
    context::DefID,
    hir::{HIRBlock, HIRFunction, HIRFunctionRef, HIRLocal, HIRStmt, HIRTypeInfo},
    types::{Type, TypeRef},
};

use mir::{builder::MIRBuilder, StatementKind};

fn function_with_local(ty: Type) -> HIRFunction {
    HIRFunction {
        name: "main".into(),
        symbol: "main".into(),
        display_name: None,
        source_id: 0,
        def_id: DefID(0),
        params: vec![],
        return_type: Type::VOID,
        body: HIRBlock {
            stmts: vec![HIRStmt::VarDecl {
                local: HIRLocal {
                    def_id: DefID(1),
                    name: "value".into(),
                    ty,
                    is_mutable: true,
                    span: Span::default(),
                },
                init: None,
                has_type_annotation: true,
            }],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    }
}

fn drop_ref() -> HIRFunctionRef {
    HIRFunctionRef {
        def_id: DefID(2),
        symbol: "Thing::drop".into(),
        display_symbol: None,
    }
}

fn build(ty: Type, explicit_drop: bool) -> mir::MIRFunction {
    let mut type_info = HIRTypeInfo::default();
    let Type::STRUCT(type_ref) = &ty else { panic!("expected struct type") };

    type_info.structs.insert(type_ref.def_id, ir::hir::HIRStructInfo {
        type_ref: type_ref.clone(),
        fields: vec![],
    });

    if explicit_drop {
        type_info.drop_functions.insert(ty.clone(), drop_ref());
    }

    let mut drop_cache = HashMap::new();
    MIRBuilder::new(&type_info, &mut drop_cache).build_function(function_with_local(ty))
}

#[test]
fn emits_drop_for_type_implementing_core_drop_trait() {
    let ty = Type::STRUCT(TypeRef::new(DefID(10), "Thing".into()));
    let mir = build(ty, true);

    assert!(
        mir.basic_blocks.iter().flat_map(|block| &block.statements).any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Drop { callee, .. } if callee.def_id == DefID(2)
            )
        }),
        "a type with a registered Drop implementation should receive MIR drop glue",
    );
}

#[test]
fn inherent_method_named_drop_does_not_enable_drop_glue() {
    let ty = Type::STRUCT(TypeRef::new(DefID(10), "Thing".into()));
    let mir = build(ty, false);

    assert!(
        mir.basic_blocks.iter().flat_map(|block| &block.statements).all(|statement| {
            !matches!(statement.kind, StatementKind::Drop { .. })
        }),
        "an inherent method named drop must not enable drop glue",
    );
}
