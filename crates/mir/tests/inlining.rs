use errors::error::Span;
use ir::{Constant, context::DefID, types::Type};
use mir::{
    BasicBlock, BasicBlockID, FunctionRef, LocalDecl, LocalID, MIRFunction,
    MIRProgram, Operand, Place, ProjectionElem, Rvalue, Statement,
    StatementKind, Terminator,
    optimizer::{OptimizationLevel, Optimizer},
};

fn local(ty: Type, is_mutable: bool) -> LocalDecl {
    LocalDecl {
        ty,
        is_mutable,
        debug_def_id: None,
        debug_name: None,
        debug_span: None,
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
fn o3_inlining_preserves_write_only_reference_parameter_binding() {
    let caller_def = DefID(0);
    let callee_def = DefID(1);
    let bool_ref = Type::REF(Box::new(Type::BOOL));

    let caller = MIRFunction {
        name: "caller".into(),
        def_id: caller_def,
        display_name: None,
        return_type: Type::BOOL,
        arg_count: 0,
        locals: vec![
            local(Type::BOOL, true),
            local(Type::BOOL, true),
            local(bool_ref.clone(), false),
            local(Type::VOID, false),
        ],
        basic_blocks: vec![
            BasicBlock {
                statements: vec![
                    assign(
                        place(1),
                        Rvalue::Use(Operand::Const(Constant::Bool(false))),
                    ),
                    assign(place(2), Rvalue::Ref(true, place(1))),
                ],
                terminator: Terminator::Call {
                    callee: FunctionRef {
                        def_id: callee_def,
                        symbol: "write_true".into(),
                        display_symbol: None,
                    },
                    args: vec![Operand::Copy(place(2))],
                    destination: place(3),
                    target: BasicBlockID(1),
                },
            },
            BasicBlock {
                statements: vec![assign(
                    place(0),
                    Rvalue::Use(Operand::Copy(place(1))),
                )],
                terminator: Terminator::Return,
            },
        ],
        is_inline: false,
    };

    let callee = MIRFunction {
        name: "write_true".into(),
        def_id: callee_def,
        display_name: None,
        return_type: Type::VOID,
        arg_count: 1,
        locals: vec![
            local(Type::VOID, true),
            local(bool_ref, false),
        ],
        basic_blocks: vec![BasicBlock {
            statements: vec![assign(
                Place {
                    local: LocalID(1),
                    projection: vec![ProjectionElem::Deref],
                },
                Rvalue::Use(Operand::Const(Constant::Bool(true))),
            )],
            terminator: Terminator::Return,
        }],
        is_inline: true,
    };

    let mut program = MIRProgram {
        functions: vec![caller, callee],
        structs: vec![],
    };

    Optimizer::optimize(&mut program, OptimizationLevel::O3);

    let caller = program.functions.iter()
        .find(|function| function.def_id == caller_def)
        .expect("caller should remain in the MIR program");

    let parameter_local = caller.basic_blocks.iter()
        .flat_map(|block| &block.statements)
        .find_map(|statement| {
            let StatementKind::Assign(destination, _) = &statement.kind else {
                return None;
            };

            destination.projection.contains(&ProjectionElem::Deref)
                .then_some(destination.local)
        })
        .expect("inlined callee should write through its reference");

    assert!(
        caller.basic_blocks.iter().flat_map(|block| &block.statements).any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign(destination, _)
                    if destination.local == parameter_local && destination.projection.is_empty()
            )
        }),
        "dead-store elimination removed the binding for an inlined write-only reference parameter",
    );
}
