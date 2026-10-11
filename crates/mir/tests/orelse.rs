use std::collections::HashMap;

use errors::error::Span;
use ir::{
    Constant,
    context::DefID,
    hir::{HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRLocal, HIRStmt, HIRTypeInfo},
    types::Type,
};

use mir::{
    Operand, ProjectionElem, Rvalue, StatementKind, Terminator,
    builder::MIRBuilder,
};

fn expr(kind: HIRExprKind, ty: Type) -> HIRExpr {
    HIRExpr { kind, ty, span: Span::default() }
}

fn local(def_id: DefID, name: &str, ty: Type) -> HIRLocal {
    HIRLocal {
        def_id,
        name: name.into(),
        ty,
        is_mutable: false,
        span: Span::default(),
    }
}

#[test]
fn orelse_branches_on_discriminant_and_extracts_payload() {
    let optional_def = DefID(1);
    let result_def = DefID(2);
    let optional_ty = Type::OPTIONAL(Box::new(Type::I32));

    let function = HIRFunction {
        name: "main".into(),
        symbol: "main".into(),
        display_name: None,
        def_id: DefID(0),
        params: vec![],
        return_type: Type::VOID,
        body: HIRBlock {
            stmts: vec![
                HIRStmt::VarDecl {
                    local: local(optional_def, "optional", optional_ty.clone()),
                    init: Some(expr(HIRExprKind::NONE, optional_ty.clone())),
                    has_type_annotation: true,
                },
                HIRStmt::VarDecl {
                    local: local(result_def, "result", Type::I32),
                    init: Some(expr(
                        HIRExprKind::OrElse {
                            optional: Box::new(expr(
                                HIRExprKind::VarRef(optional_def),
                                optional_ty,
                            )),
                            fallback: Box::new(expr(HIRExprKind::IntLiteral(7), Type::I32)),
                        },
                        Type::I32,
                    )),
                    has_type_annotation: true,
                },
            ],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    };

    let type_info = HIRTypeInfo::default();
    let mut drop_cache = HashMap::new();
    let mir = MIRBuilder::new(&type_info, &mut drop_cache).build_function(function);

    let optional_local = mir.locals.iter().enumerate()
        .find_map(|(index, local)| {
            (local.debug_def_id == Some(optional_def)).then_some(mir::LocalID(index))
        })
        .expect("expected optional MIR local");

    let (present_target, fallback_target) = mir.basic_blocks.iter()
        .find_map(|block| {
            let Terminator::SwitchInt {
                discriminant: Operand::Copy(place),
                true_target,
                false_target,
            } = &block.terminator else {
                return None;
            };

            (place.local == optional_local && matches!(
                place.projection.as_slice(),
                [ProjectionElem::Field { index: 0, ty: Type::BOOL }]
            )).then_some((*true_target, *false_target))
        })
        .expect("orelse should branch on optional discriminant");

    assert!(
        mir.basic_blocks[present_target.0].statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign(_, Rvalue::Use(
                    Operand::Copy(place) | Operand::Move(place)
                )) if place.local == optional_local && matches!(
                    place.projection.as_slice(),
                    [ProjectionElem::Field { index: 1, ty: Type::I32 }]
                )
            )
        }),
        "present branch should extract the optional payload",
    );

    assert!(
        mir.basic_blocks[fallback_target.0].statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign(_, Rvalue::Use(
                    Operand::Const(Constant::Int(7, Type::I32))
                ))
            )
        }),
        "None branch should evaluate and use the fallback",
    );
}
