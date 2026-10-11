use ir::{context::DefID, types::Type, Constant};
use errors::error::Span;
use mir::{drop_flags, BasicBlock, BasicBlockID, FunctionRef, LocalDecl, LocalID, MIRFunction,
    Operand, Place, Rvalue, Statement, StatementKind, Terminator};

fn place(n: usize) -> Place { Place { local: LocalID(n), projection: vec![] } }

fn block(statements: Vec<Statement>, terminator: Terminator) -> BasicBlock { BasicBlock { statements, terminator } }

fn assign(dest: usize, source: Operand) -> Statement {
    Statement { kind: StatementKind::Assign(place(dest), Rvalue::Use(source)), span: Span::default() }
}

fn destructor() -> FunctionRef {
    FunctionRef { def_id: DefID(100), symbol: "test::drop".into(), display_symbol: None }
}

fn drop(n: usize) -> Statement {
    Statement { kind: StatementKind::Drop { place: place(n), callee: destructor() }, span: Span::default() }
}

fn function(args: usize, blocks: Vec<BasicBlock>) -> MIRFunction {
    let types = [Type::VOID, Type::BOOL, Type::I32, Type::I32];
    let locals = types.into_iter().enumerate().map(|(i, ty)| LocalDecl {
        ty, is_mutable: true, debug_def_id: None, debug_name: Some(format!("local_{i}")), debug_span: None,
    }).collect();
    MIRFunction { name: "drop_flags".into(), def_id: DefID(0), display_name: None,
        return_type: Type::VOID, arg_count: args, locals, basic_blocks: blocks, is_inline: false }
}

fn bool_assignment(stmt: &Statement, dest: usize, value: bool) -> bool {
    matches!(&stmt.kind, StatementKind::Assign(place, Rvalue::Use(Operand::Const(Constant::Bool(x))))
        if place.local == LocalID(dest) && *x == value)
}

#[test]
fn branch_dependent_move_guards_destructor_at_runtime() {
    let mut mir = function(2, vec![
        block(vec![], Terminator::SwitchInt { discriminant: Operand::Copy(place(1)),
            true_target: BasicBlockID(1), false_target: BasicBlockID(2) }),
        block(vec![assign(3, Operand::Move(place(2)))], Terminator::Goto { target: BasicBlockID(3) }),
        block(vec![], Terminator::Goto { target: BasicBlockID(3) }),
        block(vec![drop(2)], Terminator::Return),
    ]);
    drop_flags::elaborate(&mut mir);

    let flag = 4;
    assert!(mir.basic_blocks[0].statements.iter().any(|s| bool_assignment(s, flag, true)));
    assert!(mir.basic_blocks[1].statements.iter().any(|s| bool_assignment(s, flag, false)));
    let Terminator::SwitchInt { discriminant: Operand::Copy(p), true_target, false_target } =
        &mir.basic_blocks[3].terminator else { panic!("drop must be guarded"); };
    assert_eq!(p.local, LocalID(flag));
    assert!(mir.basic_blocks[true_target.0].statements.iter().any(|s|
        matches!(&s.kind, StatementKind::Drop { place, .. } if place.local == LocalID(2))));
    assert!(mir.basic_blocks[false_target.0].statements.iter().all(|s|
        !matches!(&s.kind, StatementKind::Drop { .. })));
}

#[test]
fn reinitialization_sets_drop_flag_again() {
    let mut mir = function(2, vec![block(vec![
        assign(3, Operand::Move(place(2))),
        assign(2, Operand::Const(Constant::Int(7, Type::I32))), drop(2),
    ], Terminator::Return)]);
    drop_flags::elaborate(&mut mir);
    let updates: Vec<_> = mir.basic_blocks[0].statements.iter().filter_map(|s| {
        if bool_assignment(s, 4, false) { Some(false) }
        else if bool_assignment(s, 4, true) { Some(true) }
        else { None }
    }).collect();
    assert_eq!(updates, vec![true, false, true]);
}

#[test]
fn call_destination_becomes_initialized_only_on_success_edge() {
    let mut mir = function(1, vec![
        block(vec![], Terminator::Call { callee: destructor(), args: vec![],
            destination: place(2), target: BasicBlockID(1) }),
        block(vec![drop(2)], Terminator::Return),
    ]);
    drop_flags::elaborate(&mut mir);
    let Terminator::Call { target, .. } = &mir.basic_blocks[0].terminator else { panic!("expected call"); };
    assert_ne!(*target, BasicBlockID(1));
    assert!(mir.basic_blocks[target.0].statements.iter().any(|s| bool_assignment(s, 4, true)));
    assert!(matches!(mir.basic_blocks[target.0].terminator, Terminator::Goto { target: BasicBlockID(1) }));
}

#[test]
fn never_initialized_local_starts_with_drop_flag_cleared() {
    let mut mir = function(1, vec![block(vec![drop(2)], Terminator::Return)]);
    drop_flags::elaborate(&mut mir);
    assert!(mir.basic_blocks[0].statements.iter().any(|s| bool_assignment(s, 4, false)));
    assert!(matches!(mir.basic_blocks[0].terminator, Terminator::SwitchInt { .. }));
}
