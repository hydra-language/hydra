use std::collections::HashMap;

use errors::error::Span;
use ir::{
    Constant,
    context::DefID,
    hir::{HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRLocal, HIRStmt, HIRTypeInfo},
    types::Type,
};

use mir::{AggregateKind, Operand, Rvalue, StatementKind, builder::MIRBuilder};

fn build_optional(initializer: HIRExpr) -> mir::MIRFunction {
    let ty = Type::OPTIONAL(Box::new(Type::I32));
    let function = HIRFunction {
        name: "main".into(),
        symbol: "main".into(),
        display_name: None,
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
                init: Some(initializer),
                has_type_annotation: true,
            }],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    };

    let type_info = HIRTypeInfo::default();
    let mut drop_cache = HashMap::new();
    MIRBuilder::new(&type_info, &mut drop_cache).build_function(function)
}

#[test]
fn none_lowers_to_absent_optional_aggregate() {
    let mir = build_optional(HIRExpr {
        kind: HIRExprKind::NONE,
        ty: Type::OPTIONAL(Box::new(Type::I32)),
        span: Span::default(),
    });

    assert!(
        mir.basic_blocks.iter().flat_map(|block| &block.statements).any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign(_, Rvalue::Aggregate(
                    AggregateKind::Optional(Type::I32), operands
                )) if matches!(operands.as_slice(), [Operand::Const(Constant::Bool(false))])
            )
        }),
        "None should lower to an optional aggregate with a false discriminant",
    );
}

#[test]
fn present_value_lowers_to_present_optional_aggregate() {
    let payload = HIRExpr {
        kind: HIRExprKind::IntLiteral(42),
        ty: Type::I32,
        span: Span::default(),
    };

    let mir = build_optional(HIRExpr {
        kind: HIRExprKind::OptionalPresent(Box::new(payload)),
        ty: Type::OPTIONAL(Box::new(Type::I32)),
        span: Span::default(),
    });

    assert!(
        mir.basic_blocks.iter().flat_map(|block| &block.statements).any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign(_, Rvalue::Aggregate(
                    AggregateKind::Optional(Type::I32), operands
                )) if matches!(
                    operands.as_slice(),
                    [
                        Operand::Const(Constant::Bool(true)),
                        Operand::Const(Constant::Int(42, Type::I32))
                    ]
                )
            )
        }),
        "a present optional should carry both the discriminant and its payload",
    );
}
