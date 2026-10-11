use std::collections::HashMap;

use errors::error::Span;
use ir::{
    context::DefID,
    hir::{
        HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRFunctionRef,
        HIRLocal, HIRStmt, HIRStructInfo, HIRTypeInfo,
    },
    types::{Type, TypeRef},
};
use mir::{builder::MIRBuilder, LocalID, Place, ProjectionElem, StatementKind};

fn struct_type(id: usize, name: &str) -> Type {
    Type::STRUCT(TypeRef::new(DefID(id), name.into()))
}

fn drop_function(id: usize, symbol: &str) -> HIRFunctionRef {
    HIRFunctionRef {
        def_id: DefID(id),
        symbol: symbol.into(),
        display_symbol: None,
    }
}

fn register_struct(info: &mut HIRTypeInfo, ty: &Type, fields: Vec<Type>) {
    let Type::STRUCT(type_ref) = ty else { panic!("expected struct") };
    info.structs.insert(type_ref.def_id, HIRStructInfo {
        type_ref: type_ref.clone(),
        fields,
    });
}

fn build_payload(explicit_payload_drop: bool) -> mir::MIRFunction {
    let owned_ty = struct_type(1, "Owned");
    let payload_ty = struct_type(2, "Payload");
    let mut info = HIRTypeInfo::default();

    register_struct(&mut info, &owned_ty, vec![]);
    register_struct(&mut info, &payload_ty, vec![owned_ty.clone()]);

    info.drop_functions.insert(owned_ty.clone(), drop_function(3, "Owned::drop"));
    if explicit_payload_drop {
        info.drop_functions.insert(payload_ty.clone(), drop_function(4, "Payload::drop"));
    }

    let owned_init = HIRExpr {
        kind: HIRExprKind::StructInit { def_id: DefID(1), values: vec![] },
        ty: owned_ty,
        span: Span::default(),
    };
    let payload_init = HIRExpr {
        kind: HIRExprKind::StructInit { def_id: DefID(2), values: vec![owned_init] },
        ty: payload_ty.clone(),
        span: Span::default(),
    };

    let function = HIRFunction {
        name: "main".into(),
        symbol: "main".into(),
        display_name: None,
        source_id: 0,
        def_id: DefID(5),
        params: vec![],
        return_type: Type::VOID,
        body: HIRBlock {
            stmts: vec![HIRStmt::VarDecl {
                local: HIRLocal {
                    def_id: DefID(6),
                    name: "payload".into(),
                    ty: payload_ty,
                    is_mutable: true,
                    span: Span::default(),
                },
                init: Some(payload_init),
                has_type_annotation: true,
            }],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    };

    let mut drop_cache = HashMap::new();
    MIRBuilder::new(&info, &mut drop_cache).build_function(function)
}

fn payload_local(mir: &mir::MIRFunction) -> LocalID {
    mir.locals.iter().enumerate()
        .find_map(|(index, local)| {
            (local.debug_def_id == Some(DefID(6))).then_some(LocalID(index))
        })
        .expect("expected payload MIR local")
}

fn drops(mir: &mir::MIRFunction) -> Vec<(Place, DefID)> {
    mir.basic_blocks.iter().flat_map(|block| &block.statements)
        .filter_map(|statement| match &statement.kind {
            StatementKind::Drop { place, callee } => Some((place.clone(), callee.def_id)),
            _ => None,
        })
        .collect()
}

#[test]
fn struct_without_drop_impl_drops_owned_fields() {
    let mir = build_payload(false);
    let local = payload_local(&mir);

    assert_eq!(
        drops(&mir),
        vec![(
            Place {
                local,
                projection: vec![ProjectionElem::Field { index: 0, ty: struct_type(1, "Owned") }],
            },
            DefID(3),
        )],
        "Payload has no Drop implementation, so its owned field should be dropped",
    );
}

#[test]
fn explicit_drop_runs_before_struct_fields_are_dropped() {
    let mir = build_payload(true);
    let local = payload_local(&mir);

    assert_eq!(
        drops(&mir),
        vec![
            (Place { local, projection: vec![] }, DefID(4)),
            (
                Place {
                    local,
                    projection: vec![ProjectionElem::Field { index: 0, ty: struct_type(1, "Owned") }],
                },
                DefID(3),
            ),
        ],
        "explicit Drop must run before compiler-generated field destruction",
    );
}
