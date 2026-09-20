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
    AggregateKind,
    Operand,
    Rvalue,
    StatementKind,
    builder::MIRBuilder,
};

fn insert_main(context: &mut HIRContext) -> DefID {
    context.insert_def(SymbolInfo {
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
    })
}

fn build_function_with_optional(
    context: &mut HIRContext,
    initializer: HIRExpr,
) -> HIRFunction {
    let optional_ty = Type::OPTIONAL(Box::new(Type::I32));

    let local_def_id = context.insert_def(SymbolInfo {
        name: "value".to_string(),
        span: Span::default(),
        absolute_path: vec![],
        kind: DefKind::Variable {
            ty: optional_ty.clone(),
            is_mutable: true,
        },
        is_pub: false,
    });

    let main_def_id = insert_main(context);

    HIRFunction {
        name: "main".to_string(),
        def_id: main_def_id,
        params: vec![],
        return_type: Type::VOID,
        body: HIRBlock {
            stmts: vec![
                HIRStmt::VarDecl {
                    def_id: local_def_id,
                    ty: optional_ty,
                    init: Some(initializer),
                    has_type_annotation: true,
                    span: Span::default(),
                },
            ],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    }
}

#[test]
fn none_lowers_to_absent_optional_aggregate() {
    let mut context = HIRContext::new();

    let optional_ty = Type::OPTIONAL(Box::new(Type::I32));

    let initializer = HIRExpr {
        kind: HIRExprKind::NONE,
        ty: optional_ty,
        span: Span::default(),
    };

    let function = build_function_with_optional(
        &mut context,
        initializer,
    );

    let mir = MIRBuilder::new(&context)
        .build_function(function);

    assert!(
        mir.basic_blocks
            .iter()
            .flat_map(|block| &block.statements)
            .any(|statement| {
                let StatementKind::Assign(
                    _,
                    Rvalue::Aggregate(
                        AggregateKind::Optional(inner),
                        operands,
                    ),
                ) = &statement.kind
                else {
                    return false;
                };

                *inner == Type::I32
                    && matches!(
                        operands.as_slice(),
                        [
                            Operand::Const(
                                Constant::Bool(false)
                            )
                        ]
                    )
            }),
        "None should lower to an optional aggregate with a false discriminant",
    );
}

#[test]
fn present_value_lowers_to_present_optional_aggregate() {
    let mut context = HIRContext::new();

    let payload = HIRExpr {
        kind: HIRExprKind::IntLiteral(42),
        ty: Type::I32,
        span: Span::default(),
    };

    let initializer = HIRExpr {
        kind: HIRExprKind::OptionalPresent(
            Box::new(payload),
        ),
        ty: Type::OPTIONAL(Box::new(Type::I32)),
        span: Span::default(),
    };

    let function = build_function_with_optional(
        &mut context,
        initializer,
    );

    let mir = MIRBuilder::new(&context)
        .build_function(function);

    assert!(
        mir.basic_blocks
            .iter()
            .flat_map(|block| &block.statements)
            .any(|statement| {
                let StatementKind::Assign(
                    _,
                    Rvalue::Aggregate(
                        AggregateKind::Optional(inner),
                        operands,
                    ),
                ) = &statement.kind
                else {
                    return false;
                };

                *inner == Type::I32
                    && matches!(
                        operands.as_slice(),
                        [
                            Operand::Const(
                                Constant::Bool(true)
                            ),
                            Operand::Const(
                                Constant::Int(
                                    42,
                                    Type::I32,
                                )
                            ),
                        ]
                    )
            }),
        "a present optional should carry both a true discriminant and its payload",
    );
}
