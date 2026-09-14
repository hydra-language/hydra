use errors::error::Span;

use ir::{
    Constant,
    context::{
        DefID,
        HIRContext,
    },
    types::Type,
};

use mir::{
    BasicBlock,
    LocalDecl,
    LocalID,
    MIRFunction,
    Operand,
    Place,
    ProjectionElem,
    Rvalue,
    Statement,
    StatementKind,
    Terminator,
    bounds::BoundsChecker,
};


fn local(
    ty: Type,
) -> LocalDecl {
    LocalDecl {
        ty,
        is_mutable:
            false,
        debug_def_id:
            None,
    }
}


fn place(
    local: usize,
) -> Place {
    Place {
        local:
            LocalID(
                local
            ),

        projection:
            vec![],
    }
}


fn string_index(
    index: i64,
) -> MIRFunction {
    let string_ty =
        Type::CONST_REF(
            Box::new(
                Type::SLICE(
                    Box::new(
                        Type::U8
                    ),
                ),
            ),
        );

    MIRFunction {
        name:
            "main"
                .to_string(),

        def_id:
            DefID(0),

        return_type:
            Type::VOID,

        arg_count:
            0,

        locals:
            vec![
                local(
                    Type::VOID
                ),

                local(
                    string_ty
                ),

                local(
                    Type::USIZE
                ),

                local(
                    Type::U8
                ),
            ],

        basic_blocks:
            vec![
                BasicBlock {
                    statements:
                        vec![
                            Statement {
                                kind:
                                    StatementKind::Assign(
                                        place(
                                            1
                                        ),

                                        Rvalue::Use(
                                            Operand::Const(
                                                Constant::String(
                                                    "🦀"
                                                        .to_string(),
                                                ),
                                            ),
                                        ),
                                    ),

                                span:
                                    Span::default(),
                            },

                            Statement {
                                kind:
                                    StatementKind::Assign(
                                        place(
                                            2
                                        ),

                                        Rvalue::Use(
                                            Operand::Const(
                                                Constant::Int(
                                                    index,
                                                    Type::USIZE,
                                                ),
                                            ),
                                        ),
                                    ),

                                span:
                                    Span::default(),
                            },

                            Statement {
                                kind:
                                    StatementKind::Assign(
                                        place(
                                            3
                                        ),

                                        Rvalue::Use(
                                            Operand::Copy(
                                                Place {
                                                    local:
                                                        LocalID(
                                                            1
                                                        ),

                                                    projection:
                                                        vec![
                                                            ProjectionElem::Deref,

                                                            ProjectionElem::Index(
                                                                LocalID(
                                                                    2
                                                                )
                                                            ),
                                                        ],
                                                }
                                            ),
                                        ),
                                    ),

                                span:
                                    Span::default(),
                            },
                        ],

                    terminator:
                        Terminator::Return,
                },
            ],

        is_inline:
            false,
    }
}


#[test]
fn unicode_string_bounds_use_utf8_byte_length() {
    let context =
        HIRContext::default();

    let mir =
        string_index(
            3
        );

    BoundsChecker::new(
        &mir,
        &context,
    )
    .check()
    .expect(
        "byte index 3 must be valid for four-byte UTF-8 crab literal",
    );
}


#[test]
fn unicode_string_rejects_index_at_byte_length() {
    let context =
        HIRContext::default();

    let mir =
        string_index(
            4
        );

    let errors =
        BoundsChecker::new(
            &mir,
            &context,
        )
        .check()
        .expect_err(
            "byte index 4 must be out of bounds for four-byte UTF-8 crab literal",
        );

    assert!(
        errors
            .iter()
            .any(|error| {
                error.code
                    == "S008"
            }),
        "expected compile-time bounds error, got {errors:#?}",
    );
}
