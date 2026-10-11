use codegen::CodeGen;
use errors::error::Span;
use inkwell::context::Context;
use ir::{Constant, context::DefID, types::Type};
use mir::{
    BasicBlock, LocalDecl, LocalID, MIRFunction, MIRProgram, Operand,
    Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator,
};

fn local(ty: Type) -> LocalDecl {
    LocalDecl {
        ty, is_mutable: false, debug_def_id: None,
        debug_name: None, debug_span: None,
    }
}

fn place(id: usize) -> Place {
    Place { local: LocalID(id), projection: vec![] }
}

fn assign(destination: Place, value: Rvalue) -> Statement {
    Statement {
        kind: StatementKind::Assign(destination, value),
        span: Span::default(),
    }
}

#[test]
fn indexed_slice_emits_runtime_bounds_check() {
    let context = Context::create();
    let mut codegen = CodeGen::new(
        &context, "bounds_checks", "generic".into(), String::new(),
    );

    let slice_ty = Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::I32))));

    let function = MIRFunction {
        name: "main".into(),
        def_id: DefID(0),
        display_name: None,
        return_type: Type::VOID,
        arg_count: 0,
        locals: vec![
            local(Type::VOID),
            local(slice_ty),
            local(Type::USIZE),
            local(Type::I32),
        ],
        basic_blocks: vec![BasicBlock {
            statements: vec![
                assign(
                    place(2),
                    Rvalue::Use(Operand::Const(Constant::Int(1, Type::USIZE))),
                ),
                assign(
                    place(3),
                    Rvalue::Use(Operand::Copy(Place {
                        local: LocalID(1),
                        projection: vec![
                            ProjectionElem::Deref,
                            ProjectionElem::Index(LocalID(2)),
                        ],
                    })),
                ),
            ],
            terminator: Terminator::Return,
        }],
        is_inline: false,
    };

    codegen.generate(&MIRProgram {
        functions: vec![function],
        structs: vec![],
    }).expect("indexed slice should compile with a bounds check");

    codegen.module.verify()
        .expect("LLVM IR with runtime bounds checks should verify");

    let llvm = codegen.ir_to_string();

    assert!(
        llvm.contains("icmp ult"),
        "slice indexing should compare index against length:\n{llvm}",
    );

    assert!(
        llvm.contains("@hydra_bounds_check_fail"),
        "slice indexing should call the runtime bounds failure path:\n{llvm}",
    );

    assert!(
        llvm.contains("unreachable"),
        "bounds failure path must not continue execution:\n{llvm}",
    );
}
