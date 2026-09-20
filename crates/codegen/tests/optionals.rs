use codegen::CodeGen;
use errors::error::Span;
use inkwell::context::Context;
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
    BasicBlockID,
    LocalDecl,
    LocalID,
    MIRFunction,
    MIRProgram,
    Operand,
    Place,
    ProjectionElem,
    Rvalue,
    Statement,
    StatementKind,
    Terminator,
};

#[test]
fn optional_discriminant_and_payload_projections_codegen() {
    let context = Context::create();
    let hir_context = HIRContext::new();

    let optional_ty = Type::OPTIONAL(
        Box::new(Type::I32),
    );

    let optional_place = Place {
        local: LocalID(1),
        projection: vec![],
    };

    let discriminant_place = Place {
        local: LocalID(1),
        projection: vec![
            ProjectionElem::Field(0),
        ],
    };

    let payload_place = Place {
        local: LocalID(1),
        projection: vec![
            ProjectionElem::Field(1),
        ],
    };

    let function = MIRFunction {
        name: "main".to_string(),
        def_id: DefID(0),
        display_name: None,
        return_type: Type::VOID,
        arg_count: 0,
        locals: vec![
            LocalDecl {
                ty: Type::VOID,
                is_mutable: true,
                debug_def_id: None,
            },

            LocalDecl {
                ty: optional_ty,
                is_mutable: false,
                debug_def_id: None,
            },

            LocalDecl {
                ty: Type::I32,
                is_mutable: false,
                debug_def_id: None,
            },
        ],

        basic_blocks: vec![
            BasicBlock {
                statements: vec![
                    Statement {
                        kind: StatementKind::Assign(
                            optional_place,
                            Rvalue::Aggregate(
                                AggregateKind::Optional(
                                    Type::I32,
                                ),
                                vec![
                                    Operand::Const(
                                        Constant::Bool(true),
                                    ),

                                    Operand::Const(
                                        Constant::Int(
                                            42,
                                            Type::I32,
                                        ),
                                    ),
                                ],
                            ),
                        ),
                        span: Span::default(),
                    },
                ],

                terminator: Terminator::SwitchInt {
                    discriminant: Operand::Copy(
                        discriminant_place,
                    ),
                    true_target: BasicBlockID(1),
                    false_target: BasicBlockID(2),
                },
            },

            BasicBlock {
                statements: vec![
                    Statement {
                        kind: StatementKind::Assign(
                            Place {
                                local: LocalID(2),
                                projection: vec![],
                            },
                            Rvalue::Use(
                                Operand::Copy(
                                    payload_place,
                                ),
                            ),
                        ),
                        span: Span::default(),
                    },
                ],

                terminator: Terminator::Goto {
                    target: BasicBlockID(3),
                },
            },

            BasicBlock {
                statements: vec![
                    Statement {
                        kind: StatementKind::Assign(
                            Place {
                                local: LocalID(2),
                                projection: vec![],
                            },
                            Rvalue::Use(
                                Operand::Const(
                                    Constant::Int(
                                        7,
                                        Type::I32,
                                    ),
                                ),
                            ),
                        ),
                        span: Span::default(),
                    },
                ],

                terminator: Terminator::Goto {
                    target: BasicBlockID(3),
                },
            },

            BasicBlock {
                statements: vec![],
                terminator: Terminator::Return,
            },
        ],

        is_inline: false,
    };

    let program = MIRProgram {
        functions: vec![
            function,
        ],
    };

    let mut codegen = CodeGen::new(
        &context,
        &hir_context,
        "optional_projection_test",
    );

    codegen
        .generate(&program)
        .expect(
            "optional projections should lower to LLVM",
        );

    codegen
        .module
        .verify()
        .expect(
            "generated LLVM for optional projections should verify",
        );
}
