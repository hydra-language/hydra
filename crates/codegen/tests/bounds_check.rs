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
fn indexed_slice_emits_runtime_bounds_check() {
    let context = Context::create();
    let hir_context = HIRContext::default();

    let mut codegen = CodeGen::new(
        &context,
        &hir_context,
        "bounds_checks",
    );

    let slice_ty = Type::CONST_REF(Box::new(
        Type::SLICE(Box::new(Type::I32))
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
                ty: slice_ty,
                is_mutable: false,
                debug_def_id: None,
            },
            LocalDecl {
                ty: Type::USIZE,
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
                    Statement {
                        kind: StatementKind::Assign(
                            Place {
                                local: LocalID(3),
                                projection: vec![],
                            },
                            Rvalue::Use(
                                Operand::Copy(Place {
                                    local: LocalID(1),
                                    projection: vec![
                                        ProjectionElem::Deref,
                                        ProjectionElem::Index(LocalID(2)),
                                    ],
                                })
                            ),
                        ),
                        span: Span::default(),
                    },
                ],
                terminator: Terminator::Return,
            },
        ],
        is_inline: false,
    };

    let program = MIRProgram {
        functions: vec![function],
    };

    codegen.generate(&program)
        .expect("indexed slice should compile with a bounds check");

    let llvm = codegen.module.print_to_string().to_string();

    assert!(
        llvm.contains("icmp ult"),
        "slice indexing should compare index against length:\n{}",
        llvm
    );

    assert!(
        llvm.contains("@hydra_bounds_check_fail"),
        "slice indexing should call the runtime bounds failure path:\n{}",
        llvm
    );

    assert!(
        llvm.contains("unreachable"),
        "bounds failure path must not continue execution:\n{}",
        llvm
    );
}
