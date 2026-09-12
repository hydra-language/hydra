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
    AggregateKind,
    BasicBlock,
    bounds::BoundsChecker,
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
};

fn span(line: usize) -> Span {
    Span {
        line,
        column: 1,
        length: 1,
    }
}

fn place(local: usize) -> Place {
    Place {
        local: LocalID(local),
        projection: vec![],
    }
}

fn local(ty: Type) -> LocalDecl {
    LocalDecl {
        ty,
        is_mutable: false,
        debug_def_id: None,
    }
}

fn known_slice_access(index: i64) -> MIRFunction {
    let slice_ty = Type::CONST_REF(Box::new(
        Type::SLICE(Box::new(Type::I32))
    ));

    MIRFunction {
        name: "main".to_string(),
        def_id: DefID(0),
        return_type: Type::VOID,
        arg_count: 0,

        locals: vec![
            local(Type::VOID),

            // _1 = source binding `x`
            local(slice_ty.clone()),

            // _2 = hidden backing array
            local(Type::ARRAY(
                Box::new(Type::I32),
                3,
            )),

            // _3 = temporary fat slice
            local(slice_ty),

            // _4 = materialized index
            local(Type::USIZE),

            // _5 = indexed value
            local(Type::I32),
        ],

        basic_blocks: vec![
            BasicBlock {
                statements: vec![
                    Statement {
                        kind: StatementKind::Assign(
                            place(2),
                            Rvalue::Aggregate(
                                AggregateKind::Array(
                                    Type::I32
                                ),
                                vec![
                                    Operand::Const(
                                        Constant::Int(
                                            10,
                                            Type::I32,
                                        )
                                    ),
                                    Operand::Const(
                                        Constant::Int(
                                            20,
                                            Type::I32,
                                        )
                                    ),
                                    Operand::Const(
                                        Constant::Int(
                                            30,
                                            Type::I32,
                                        )
                                    ),
                                ],
                            ),
                        ),
                        span: span(2),
                    },

                    Statement {
                        kind: StatementKind::Assign(
                            place(3),
                            Rvalue::SliceRef {
                                is_mut: false,
                                place: place(2),
                                len: 3,
                                element_ty: Type::I32,
                            },
                        ),
                        span: span(2),
                    },

                    Statement {
                        kind: StatementKind::Assign(
                            place(1),
                            Rvalue::Use(
                                Operand::Copy(
                                    place(3)
                                )
                            ),
                        ),
                        span: span(2),
                    },

                    Statement {
                        kind: StatementKind::Assign(
                            place(4),
                            Rvalue::Use(
                                Operand::Const(
                                    Constant::Int(
                                        index,
                                        Type::USIZE,
                                    )
                                )
                            ),
                        ),
                        span: span(3),
                    },

                    Statement {
                        kind: StatementKind::Assign(
                            place(5),
                            Rvalue::Use(
                                Operand::Copy(
                                    Place {
                                        local: LocalID(1),
                                        projection: vec![
                                            ProjectionElem::Deref,
                                            ProjectionElem::Index(
                                                LocalID(4)
                                            ),
                                        ],
                                    }
                                )
                            ),
                        ),
                        span: span(3),
                    },
                ],

                terminator: Terminator::Return,
            },
        ],

        is_inline: false,
    }
}

#[test]
fn rejects_known_out_of_bounds_slice_index() {
    let context = HIRContext::default();
    let function = known_slice_access(3);

    let errors = BoundsChecker::new(
        &function,
        &context,
    )
    .check()
    .expect_err(
        "index equal to slice length must be rejected"
    );

    assert_eq!(
        errors.len(),
        1,
    );

    assert_eq!(
        errors[0].code,
        "S008",
    );

    assert_eq!(
        errors[0].message,
        "index out of bounds: len is 3 but index is 3",
    );
}

#[test]
fn accepts_known_in_bounds_slice_index() {
    let context = HIRContext::default();
    let function = known_slice_access(2);

    BoundsChecker::new(
        &function,
        &context,
    )
    .check()
    .expect(
        "index below slice length must be accepted"
    );
}

#[test]
fn leaves_unknown_slice_index_for_runtime_checking() {
    let context = HIRContext::default();

    let slice_ty = Type::CONST_REF(Box::new(
        Type::SLICE(Box::new(Type::I32))
    ));

    let function = MIRFunction {
        name: "get".to_string(),
        def_id: DefID(0),
        return_type: Type::I32,
        arg_count: 2,

        locals: vec![
            local(Type::I32),
            local(slice_ty),
            local(Type::USIZE),
        ],

        basic_blocks: vec![
            BasicBlock {
                statements: vec![
                    Statement {
                        kind: StatementKind::Assign(
                            place(0),
                            Rvalue::Use(
                                Operand::Copy(
                                    Place {
                                        local: LocalID(1),
                                        projection: vec![
                                            ProjectionElem::Deref,
                                            ProjectionElem::Index(
                                                LocalID(2)
                                            ),
                                        ],
                                    }
                                )
                            ),
                        ),
                        span: span(2),
                    },
                ],

                terminator: Terminator::Return,
            },
        ],

        is_inline: false,
    };

    BoundsChecker::new(
        &function,
        &context,
    )
    .check()
    .expect(
        "unknown runtime bounds must not be rejected statically"
    );
}
