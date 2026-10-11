use codegen::CodeGen;
use errors::error::Span;
use inkwell::context::Context;
use ir::{Constant, context::DefID, types::Type};
use mir::{
    AggregateKind, BasicBlock, BasicBlockID, LocalDecl, LocalID,
    MIRFunction, MIRProgram, Operand, Place, ProjectionElem,
    Rvalue, Statement, StatementKind, Terminator,
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
fn optional_discriminant_and_payload_projections_codegen() {
    let context = Context::create();
    let optional_ty = Type::OPTIONAL(Box::new(Type::I32));
    let discriminant = Place {
        local: LocalID(1),
        projection: vec![ProjectionElem::Field { index: 0, ty: Type::BOOL }],
    };
    let payload = Place {
        local: LocalID(1),
        projection: vec![ProjectionElem::Field { index: 1, ty: Type::I32 }],
    };

    let function = MIRFunction {
        name: "main".into(),
        def_id: DefID(0),
        display_name: None,
        return_type: Type::VOID,
        arg_count: 0,
        locals: vec![local(Type::VOID), local(optional_ty), local(Type::I32)],
        basic_blocks: vec![
            BasicBlock {
                statements: vec![assign(
                    place(1),
                    Rvalue::Aggregate(AggregateKind::Optional(Type::I32), vec![
                        Operand::Const(Constant::Bool(true)),
                        Operand::Const(Constant::Int(42, Type::I32)),
                    ]),
                )],
                terminator: Terminator::SwitchInt {
                    discriminant: Operand::Copy(discriminant),
                    true_target: BasicBlockID(1),
                    false_target: BasicBlockID(2),
                },
            },
            BasicBlock {
                statements: vec![assign(place(2), Rvalue::Use(Operand::Copy(payload)))],
                terminator: Terminator::Goto { target: BasicBlockID(3) },
            },
            BasicBlock {
                statements: vec![assign(
                    place(2),
                    Rvalue::Use(Operand::Const(Constant::Int(7, Type::I32))),
                )],
                terminator: Terminator::Goto { target: BasicBlockID(3) },
            },
            BasicBlock { statements: vec![], terminator: Terminator::Return },
        ],
        is_inline: false,
    };

    let mut codegen = CodeGen::new(
        &context, "optional_projection_test", "generic".into(), String::new(),
    );

    codegen.generate(&MIRProgram {
        functions: vec![function],
        structs: vec![],
    }).expect("optional projections should lower to LLVM");

    codegen.module.verify()
        .expect("generated LLVM for optional projections should verify");
}
