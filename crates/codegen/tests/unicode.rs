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
    BasicBlock,
    LocalDecl,
    LocalID,
    MIRFunction,
    MIRProgram,
    Operand,
    Place,
    Rvalue,
    Statement,
    StatementKind,
    Terminator,
};


#[test]
fn unicode_string_literal_uses_utf8_byte_storage() {
    let hir_context =
        HIRContext::default();

    let llvm_context =
        Context::create();

    let mut codegen =
        CodeGen::new(
            &llvm_context,
            &hir_context,
            "unicode_string_test",
        );

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

    let function =
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
                    LocalDecl {
                        ty:
                            Type::VOID,

                        is_mutable:
                            true,

                        debug_def_id:
                            None,
                    },

                    LocalDecl {
                        ty:
                            string_ty,

                        is_mutable:
                            false,

                        debug_def_id:
                            None,
                    },
                ],

            basic_blocks:
                vec![
                    BasicBlock {
                        statements:
                            vec![
                                Statement {
                                    kind:
                                        StatementKind::Assign(
                                            Place {
                                                local:
                                                    LocalID(
                                                        1
                                                    ),

                                                projection:
                                                    vec![],
                                            },

                                            Rvalue::Use(
                                                Operand::Const(
                                                    Constant::String(
                                                        "A🦀"
                                                            .to_string(),
                                                    ),
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
        };

    let program =
        MIRProgram {
            functions:
                vec![
                    function
                ],
        };

    codegen
        .generate(
            &program
        )
        .expect(
            "codegen should succeed",
        );

    let llvm =
        codegen.ir_to_string();

    assert!(
        llvm.contains(
            "[5 x i8]"
        ),
        "`A🦀` must use five UTF-8 bytes of backing storage:\n{llvm}",
    );

    assert!(
        !llvm.contains(
            "[2 x i32]"
        ),
        "string literals must no longer use Unicode-scalar char storage:\n{llvm}",
    );
}
