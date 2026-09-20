use errors::error::Span;
use ir::{
    Constant,
    context::{
        DefID,
        DefKind,
        HIRContext,
        SymbolInfo,
    },
    hir::{
        HIRBlock,
        HIRExpr,
        HIRExprKind,
        HIRFunction,
        HIRStmt,
    },
    types::Type,
};
use mir::{
    Operand,
    Place,
    ProjectionElem,
    Rvalue,
    StatementKind,
    Terminator,
    builder::MIRBuilder,
};

fn insert_variable(
    context: &mut HIRContext,
    name: &str,
    ty: Type,
) -> DefID {
    context.insert_def(SymbolInfo {
        name: name.to_string(),
        span: Span::default(),
        absolute_path: vec![],
        kind: DefKind::Variable {
            ty,
            is_mutable: false,
        },
        is_pub: false,
    })
}

fn insert_main(
    context: &mut HIRContext,
) -> DefID {
    context.insert_def(SymbolInfo {
        name: "main".to_string(),
        span: Span::default(),
        absolute_path: vec![
            "main".to_string(),
        ],
        kind: DefKind::Function {
            params: vec![],
            annotations: vec![],
            return_type: Type::VOID,
            generic_params: vec![],
            owner_generic_count: 0,
            intrinsic: None,
        },
        is_pub: false,
    })
}

#[test]
fn orelse_branches_on_discriminant_and_extracts_payload() {
    let mut context = HIRContext::new();

    let optional_ty = Type::OPTIONAL(
        Box::new(Type::I32),
    );

    let optional_def = insert_variable(
        &mut context,
        "optional",
        optional_ty.clone(),
    );

    let result_def = insert_variable(
        &mut context,
        "result",
        Type::I32,
    );

    let main_def = insert_main(
        &mut context,
    );

    let function = HIRFunction {
        name: "main".to_string(),
        def_id: main_def,
        params: vec![],
        return_type: Type::VOID,
        body: HIRBlock {
            stmts: vec![
                HIRStmt::VarDecl {
                    def_id: optional_def,
                    ty: optional_ty.clone(),
                    init: Some(HIRExpr {
                        kind: HIRExprKind::NONE,
                        ty: optional_ty.clone(),
                        span: Span::default(),
                    }),
                    has_type_annotation: true,
                    span: Span::default(),
                },

                HIRStmt::VarDecl {
                    def_id: result_def,
                    ty: Type::I32,
                    init: Some(HIRExpr {
                        kind: HIRExprKind::OrElse {
                            optional: Box::new(
                                HIRExpr {
                                    kind: HIRExprKind::VarRef(
                                        optional_def,
                                    ),
                                    ty: optional_ty,
                                    span: Span::default(),
                                },
                            ),

                            fallback: Box::new(
                                HIRExpr {
                                    kind: HIRExprKind::IntLiteral(
                                        7,
                                    ),
                                    ty: Type::I32,
                                    span: Span::default(),
                                },
                            ),
                        },

                        ty: Type::I32,
                        span: Span::default(),
                    }),
                    has_type_annotation: true,
                    span: Span::default(),
                },
            ],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    };

    let mir = MIRBuilder::new(&context)
        .build_function(function);

    let optional_local = mir
        .locals
        .iter()
        .enumerate()
        .find_map(|(index, local)| {
            (local.debug_def_id == Some(optional_def))
                .then_some(mir::LocalID(index))
        })
        .expect("expected optional MIR local");

    let (
        present_target,
        fallback_target,
    ) = mir
        .basic_blocks
        .iter()
        .find_map(|block| {
            let Terminator::SwitchInt {
                discriminant,
                true_target,
                false_target,
            } = &block.terminator
            else {
                return None;
            };

            let Operand::Copy(place) = discriminant else {
                return None;
            };

            if place.local != optional_local
                || place.projection
                    != vec![
                        ProjectionElem::Field(0),
                    ]
            {
                return None;
            }

            Some((
                *true_target,
                *false_target,
            ))
        })
        .expect(
            "orelse should branch on optional discriminant",
        );

    assert!(
        mir.basic_blocks[present_target.0]
            .statements
            .iter()
            .any(|statement| {
                let StatementKind::Assign(
                    _,
                    Rvalue::Use(
                        Operand::Copy(place)
                        | Operand::Move(place),
                    ),
                ) = &statement.kind
                else {
                    return false;
                };

                place.local == optional_local
                    && place.projection
                        == vec![
                            ProjectionElem::Field(1),
                        ]
            }),
        "present branch should extract the optional payload",
    );

    assert!(
        mir.basic_blocks[fallback_target.0]
            .statements
            .iter()
            .any(|statement| {
                matches!(
                    &statement.kind,
                    StatementKind::Assign(
                        _,
                        Rvalue::Use(
                            Operand::Const(
                                Constant::Int(
                                    7,
                                    Type::I32,
                                ),
                            ),
                        ),
                    )
                )
            }),
        "None branch should evaluate and use the fallback",
    );
}
