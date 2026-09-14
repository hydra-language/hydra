use errors::error::Span;

use ir::{
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
    LocalID,
    StatementKind,
    Terminator,
    builder::MIRBuilder,
};


fn function_def(
    context: &mut HIRContext,
    name: &str,
    return_type: Type,
) -> DefID {
    context.insert_def(
        SymbolInfo {
            name:
                name.to_string(),

            span:
                Span::default(),

            absolute_path:
                vec![
                    name.to_string(),
                ],

            kind:
                DefKind::Function {
                    params:
                        vec![],

                    annotations:
                        vec![],

                    return_type,

                    generic_params:
                        vec![],

                    owner_generic_count:
                        0,

                    intrinsic:
                        None,
                },

            is_pub:
                false,
        }
    )
}


#[test]
fn returning_void_call_does_not_assign_void_value_to_return_place() {
    let mut context =
        HIRContext::new();

    let callee =
        function_def(
            &mut context,
            "grow",
            Type::VOID,
        );

    let caller =
        function_def(
            &mut context,
            "push",
            Type::VOID,
        );


    //
    // Equivalent HIR to:
    //
    //     fn push() -> void {
    //         return grow();
    //     }
    //
    let call =
        HIRExpr {
            kind:
                HIRExprKind::Call {
                    callee,

                    args:
                        vec![],

                    generic_args:
                        vec![],
                },

            ty:
                Type::VOID,

            span:
                Span::default(),
        };


    let return_expr =
        HIRExpr {
            kind:
                HIRExprKind::Return(
                    Some(
                        Box::new(
                            call
                        )
                    )
                ),

            ty:
                Type::VOID,

            span:
                Span::default(),
        };


    let function =
        HIRFunction {
            name:
                "push".to_string(),

            def_id:
                caller,

            params:
                vec![],

            return_type:
                Type::VOID,

            body:
                HIRBlock {
                    stmts:
                        vec![
                            HIRStmt::Expr(
                                return_expr
                            ),
                        ],

                    span:
                        Span::default(),
                },

            is_extern:
                false,

            is_inline:
                false,

            generic_params:
                vec![],
        };


    let mir =
        MIRBuilder::new(
            &context
        )
        .build_function(
            function
        );


    //
    // The call itself still has a void destination because MIR
    // currently represents every call with a destination place.
    //
    let Terminator::Call {
        destination,
        target,
        ..
    } = &mir.basic_blocks[0].terminator
    else {
        panic!(
            "expected void function call terminator, found {:#?}",
            mir.basic_blocks[0].terminator,
        );
    };


    assert_eq!(
        mir.locals[
            destination.local.0
        ].ty,
        Type::VOID,
    );


    //
    // The important invariant:
    //
    //     return grow();
    //
    // must NOT become:
    //
    //     _0 = _void_temp;
    //
    // because void locals have no runtime storage.
    //
    for block in &mir.basic_blocks {
        for statement in &block.statements {
            if let StatementKind::Assign(
                place,
                _,
            ) = &statement.kind
            {
                assert_ne!(
                    place.local,
                    LocalID(0),
                    "void return expression must not be assigned to the return place",
                );
            }
        }
    }


    assert!(
        matches!(
            mir.basic_blocks[
                target.0
            ].terminator,
            Terminator::Return
        ),
        "control must return directly after the void call",
    );
}
