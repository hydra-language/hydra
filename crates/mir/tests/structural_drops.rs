use std::collections::HashMap;

use analyzer::traits;
use errors::error::Span;
use ir::{
    context::{DefID, DefKind, HIRContext, SymbolInfo, TraitImpl},
    hir::{HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRStmt},
    types::{Type, TypeRef},
};
use mir::{
    builder::MIRBuilder,
    LocalID,
    Place,
    ProjectionElem,
    StatementKind,
};

fn drop_trait_path() -> Vec<String> {
    traits::DROP_TRAIT_PATH
        .split("::")
        .map(str::to_string)
        .collect()
}

fn insert_drop_trait(context: &mut HIRContext) -> DefID {
    let mut drop_method_path = drop_trait_path();
    drop_method_path.push("drop".to_string());

    let drop_method_def_id = context.insert_def(SymbolInfo {
        name: "drop".to_string(),
        span: Span::default(),
        absolute_path: drop_method_path,
        kind: DefKind::Function {
            params: vec![Type::REF(Box::new(Type::GENERIC("Self".to_string())))],
            annotations: vec![],
            return_type: Type::VOID,
            generic_params: vec![],
            owner_generic_count: 0,
            intrinsic: None,
        },
        is_pub: true,
    });

    context.insert_def(SymbolInfo {
        name: "Drop".to_string(),
        span: Span::default(),
        absolute_path: drop_trait_path(),
        kind: DefKind::Trait {
            methods: vec![drop_method_def_id],
        },
        is_pub: true,
    })
}

fn insert_struct(context: &mut HIRContext, name: &str, fields: Vec<(String, Type, bool)>) -> (DefID, Type) {
    let def_id = context.insert_def(SymbolInfo {
        name: name.to_string(),
        span: Span::default(),
        absolute_path: vec![name.to_string()],
        kind: DefKind::Struct {
            fields,
            generic_params: vec![],
        },
        is_pub: false,
    });

    (
        def_id,
        Type::STRUCT(TypeRef::new(def_id, name.to_string())),
    )
}

fn register_drop_impl(context: &mut HIRContext, drop_trait_def_id: DefID, target_ty: &Type, name: &str) {
    let drop_method_def_id = context.insert_def(SymbolInfo {
        name: "drop".to_string(),
        span: Span::default(),
        absolute_path: vec![
            format!("{name}::<{}>", traits::DROP_TRAIT_PATH),
            "drop".to_string(),
        ],
        kind: DefKind::Function {
            params: vec![Type::REF(Box::new(target_ty.clone()))],
            annotations: vec![],
            return_type: Type::VOID,
            generic_params: vec![],
            owner_generic_count: 0,
            intrinsic: None,
        },
        is_pub: false,
    });

    context.register_trait_impl(TraitImpl {
        trait_def_id: drop_trait_def_id,
        target_type: target_ty.clone(),
        methods: HashMap::from([("drop".to_string(), drop_method_def_id)]),
    });
}

fn build_payload_function(
    context: &mut HIRContext,
    owned_def_id: DefID,
    owned_ty: Type,
    payload_def_id: DefID,
    payload_ty: Type,
) -> (HIRFunction, DefID) {
    let payload_var_def_id = context.insert_def(SymbolInfo {
        name: "payload".to_string(),
        span: Span::default(),
        absolute_path: vec![],
        kind: DefKind::Variable {
            ty: payload_ty.clone(),
            is_mutable: true,
        },
        is_pub: false,
    });

    let main_def_id = context.insert_def(SymbolInfo {
        name: "main".to_string(),
        span: Span::default(),
        absolute_path: vec!["main".to_string()],
        kind: DefKind::Function {
            params: vec![],
            annotations: vec![],
            return_type: Type::VOID,
            generic_params: vec![],
            owner_generic_count: 0,
            intrinsic: None,
        },
        is_pub: false,
    });

    let owned_init = HIRExpr {
        kind: HIRExprKind::StructInit {
            def_id: owned_def_id,
            values: vec![],
        },
        ty: owned_ty,
        span: Span::default(),
    };

    let payload_init = HIRExpr {
        kind: HIRExprKind::StructInit {
            def_id: payload_def_id,
            values: vec![owned_init],
        },
        ty: payload_ty.clone(),
        span: Span::default(),
    };

    (
        HIRFunction {
            name: "main".to_string(),
            def_id: main_def_id,
            params: vec![],
            return_type: Type::VOID,
            body: HIRBlock {
                stmts: vec![HIRStmt::VarDecl {
                    def_id: payload_var_def_id,
                    ty: payload_ty,
                    init: Some(payload_init),
                    has_type_annotation: true,
                    span: Span::default(),
                }],
                span: Span::default(),
            },
            is_extern: false,
            is_inline: false,
            generic_params: vec![],
        },
        payload_var_def_id,
    )
}

fn find_local_for_def(mir: &mir::MIRFunction, def_id: DefID) -> LocalID {
    mir.locals
        .iter()
        .enumerate()
        .find_map(|(index, local)| {
            if local.debug_def_id == Some(def_id) {
                Some(LocalID(index))
            } else {
                None
            }
        })
        .expect("expected MIR local for definition")
}

fn collect_drops(mir: &mir::MIRFunction) -> Vec<Place> {
    mir.basic_blocks
        .iter()
        .flat_map(|block| &block.statements)
        .filter_map(|statement| {
            let StatementKind::Drop(place) = &statement.kind else {
                return None;
            };

            Some(place.clone())
        })
        .collect()
}

#[test]
fn struct_without_drop_impl_drops_owned_fields() {
    let mut context = HIRContext::new();

    let drop_trait_def_id = insert_drop_trait(&mut context);

    let (owned_def_id, owned_ty) = insert_struct(&mut context, "Owned", vec![]);

    register_drop_impl(
        &mut context,
        drop_trait_def_id,
        &owned_ty,
        "Owned",
    );

    let (payload_def_id, payload_ty) = insert_struct(
        &mut context,
        "Payload",
        vec![("data".to_string(), owned_ty.clone(), false)],
    );

    let (function, payload_var_def_id) = build_payload_function(
        &mut context,
        owned_def_id,
        owned_ty,
        payload_def_id,
        payload_ty,
    );

    let mir = MIRBuilder::new(&context).build_function(function);
    let payload_local = find_local_for_def(&mir, payload_var_def_id);

    assert_eq!(
        collect_drops(&mir),
        vec![Place {
            local: payload_local,
            projection: vec![ProjectionElem::Field(0)],
        }],
        "Payload has no explicit Drop implementation, so MIR should directly drop its owned field",
    );
}

#[test]
fn explicit_drop_runs_before_struct_fields_are_dropped() {
    let mut context = HIRContext::new();

    let drop_trait_def_id = insert_drop_trait(&mut context);

    let (owned_def_id, owned_ty) = insert_struct(&mut context, "Owned", vec![]);

    register_drop_impl(
        &mut context,
        drop_trait_def_id,
        &owned_ty,
        "Owned",
    );

    let (payload_def_id, payload_ty) = insert_struct(
        &mut context,
        "Payload",
        vec![("data".to_string(), owned_ty.clone(), false)],
    );

    register_drop_impl(
        &mut context,
        drop_trait_def_id,
        &payload_ty,
        "Payload",
    );

    let (function, payload_var_def_id) = build_payload_function(
        &mut context,
        owned_def_id,
        owned_ty,
        payload_def_id,
        payload_ty,
    );

    let mir = MIRBuilder::new(&context).build_function(function);
    let payload_local = find_local_for_def(&mir, payload_var_def_id);

    assert_eq!(
        collect_drops(&mir),
        vec![
            Place {
                local: payload_local,
                projection: vec![],
            },
            Place {
                local: payload_local,
                projection: vec![ProjectionElem::Field(0)],
            },
        ],
        "explicit Drop should run before compiler-generated field destruction",
    );
}
