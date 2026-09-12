use codegen::CodeGen;
use errors::error::Span;
use inkwell::context::Context;
use ir::{
    Constant,
    context::{DefID, HIRContext},
    types::Type,
};
use mir::{
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
fn printing_indexed_char_slice_uses_projected_char_type() {
    let context = Context::create();
    let hir_context = HIRContext::new();

    let mut codegen = CodeGen::new(
        &context,
        &hir_context,
        "projected_operand_printing",
    );

    let string_ty = Type::CONST_REF(Box::new(
        Type::SLICE(Box::new(Type::CHAR))
    ));

    let function = MIRFunction {
        name: "main".to_string(),
        def_id: DefID(0),
        return_type: Type::VOID,
        arg_count: 0,
        locals: vec![
            LocalDecl {
                ty: Type::VOID,
                is_mutable: true,
                debug_def_id: None,
            },
            LocalDecl {
                ty: string_ty,
                is_mutable: false,
                debug_def_id: None,
            },
            LocalDecl {
                ty: Type::USIZE,
                is_mutable: false,
                debug_def_id: None,
            },
        ],
        basic_blocks: vec![
            BasicBlock {
                statements: vec![
                    Statement {
                        kind: StatementKind::Assign(
                            Place {
                                local: LocalID(1),
                                projection: vec![],
                            },
                            Rvalue::Use(
                                Operand::Const(
                                    Constant::String("hello".to_string())
                                )
                            ),
                        ),
                        span: Span::default(),
                    },
                    Statement {
                        kind: StatementKind::Assign(
                            Place {
                                local: LocalID(2),
                                projection: vec![],
                            },
                            Rvalue::Use(
                                Operand::Const(
                                    Constant::Int(1, Type::USIZE)
                                )
                            ),
                        ),
                        span: Span::default(),
                    },
                ],
                terminator: Terminator::BuiltinCall {
                    name: "println".to_string(),
                    args: vec![
                        Operand::Const(
                            Constant::String("{}".to_string())
                        ),
                        Operand::Copy(Place {
                            local: LocalID(1),
                            projection: vec![
                                ProjectionElem::Deref,
                                ProjectionElem::Index(LocalID(2)),
                            ],
                        }),
                    ],
                    target: BasicBlockID(1),
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
        functions: vec![function],
    };

    codegen.generate(&program)
        .expect("printing s[index] from &[char] should compile");

    let llvm = codegen.module.print_to_string().to_string();

    assert!(
        llvm.contains("call void @print_chars"),
        "printing a projected char should use char printing:\n{}",
        llvm
    );
}
