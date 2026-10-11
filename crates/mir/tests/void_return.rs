use std::collections::HashMap;

use errors::error::Span;
use ir::{
    context::DefID,
    hir::{
        HIRBlock, HIRExpr, HIRExprKind, HIRFunction,
        HIRFunctionRef, HIRStmt, HIRTypeInfo,
    },
    types::Type,
};
use mir::{builder::MIRBuilder, LocalID, StatementKind, Terminator};

#[test]
fn returning_void_call_does_not_assign_void_value_to_return_place() {
    let call = HIRExpr {
        kind: HIRExprKind::Call {
            callee: HIRFunctionRef {
                def_id: DefID(1),
                symbol: "grow".into(),
                display_symbol: None,
            },
            args: vec![],
            generic_args: vec![],
            trait_dispatch: None,
        },
        ty: Type::VOID,
        span: Span::default(),
    };

    let return_expr = HIRExpr {
        kind: HIRExprKind::Return(Some(Box::new(call))),
        ty: Type::VOID,
        span: Span::default(),
    };

    let function = HIRFunction {
        name: "push".into(),
        symbol: "push".into(),
        display_name: None,
        def_id: DefID(2),
        source_id: 0,
        params: vec![],
        return_type: Type::VOID,
        body: HIRBlock {
            stmts: vec![HIRStmt::Expr(return_expr)],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    };

    let type_info = HIRTypeInfo::default();
    let mut drop_cache = HashMap::new();
    let mir = MIRBuilder::new(&type_info, &mut drop_cache).build_function(function);

    let Terminator::Call { destination, target, .. } = &mir.basic_blocks[0].terminator else {
        panic!("expected void function call terminator");
    };

    assert_eq!(mir.locals[destination.local.0].ty, Type::VOID);

    for statement in mir.basic_blocks.iter().flat_map(|block| &block.statements) {
        if let StatementKind::Assign(place, _) = &statement.kind {
            assert_ne!(
                place.local,
                LocalID(0),
                "void return expression must not be assigned to the return place",
            );
        }
    }

    assert!(
        matches!(mir.basic_blocks[target.0].terminator, Terminator::Return),
        "control must return directly after the void call",
    );
}
