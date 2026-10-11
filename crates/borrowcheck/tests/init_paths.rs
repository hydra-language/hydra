use borrowcheck::borrowcheck::BorrowChecker;
use errors::error::{HydraError, Span};
use ir::{context::DefID, types::{Type, TypeRef}, Constant};
use mir::{AggregateKind, BasicBlock, BasicBlockID, LocalDecl, LocalID, MIRFunction, Operand,
    Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator};

fn place(n: usize) -> Place { Place { local: LocalID(n), projection: vec![] } }
fn field(n: usize, index: usize) -> Place {
    Place { local: LocalID(n), projection: vec![ProjectionElem::Field { index, ty: Type::I32 }] }
}
fn assign(dest: Place, value: Rvalue) -> Statement {
    Statement { kind: StatementKind::Assign(dest, value), span: Span::default() }
}
fn int(n: i64) -> Rvalue { Rvalue::Use(Operand::Const(Constant::Int(n, Type::I32))) }
fn read(dest: usize, src: Place) -> Statement { assign(place(dest), Rvalue::Use(Operand::Copy(src))) }
fn move_to(dest: usize, src: Place) -> Statement { assign(place(dest), Rvalue::Use(Operand::Move(src))) }
fn block(statements: Vec<Statement>, terminator: Terminator) -> BasicBlock { BasicBlock { statements, terminator } }
fn go(n: usize) -> Terminator { Terminator::Goto { target: BasicBlockID(n) } }
fn pair() -> Type { Type::STRUCT(TypeRef::new(DefID(99), "test::Pair".into())) }
fn fixture(return_ty: Type, args: usize, types: Vec<Type>, blocks: Vec<BasicBlock>) -> MIRFunction {
    let locals = std::iter::once(return_ty.clone()).chain(types).enumerate().map(|(i, ty)| LocalDecl {
        ty, is_mutable: true, debug_def_id: None, debug_name: Some(format!("local_{i}")), debug_span: None,
    }).collect();
    MIRFunction { name: "init_paths".into(), def_id: DefID(0), display_name: None,
        return_type: return_ty, arg_count: args, locals, basic_blocks: blocks, is_inline: false }
}
fn check(mir: &MIRFunction) -> Result<(), Vec<HydraError>> { BorrowChecker::new(mir).check() }
fn expect_uninitialized(mir: &MIRFunction) {
    let errors = check(mir).expect_err("expected BC003");
    assert!(errors.iter().any(|e| e.code == "BC003"), "{errors:?}");
}

#[test]
fn read_before_first_assignment_is_rejected() {
    let mir = fixture(Type::VOID, 0, vec![Type::I32, Type::I32], vec![block(vec![
        read(2, place(1)),
    ], Terminator::Return)]);
    expect_uninitialized(&mir);
}

#[test]
fn initialization_on_only_one_branch_is_insufficient() {
    let mir = fixture(Type::VOID, 1, vec![Type::BOOL, Type::I32, Type::I32], vec![
        block(vec![], Terminator::SwitchInt { discriminant: Operand::Copy(place(1)),
            true_target: BasicBlockID(1), false_target: BasicBlockID(2) }),
        block(vec![assign(place(2), int(10))], go(3)),
        block(vec![], go(3)),
        block(vec![read(3, place(2))], Terminator::Return),
    ]);
    expect_uninitialized(&mir);
}

#[test]
fn initialization_on_both_branches_is_sufficient() {
    let mir = fixture(Type::VOID, 1, vec![Type::BOOL, Type::I32, Type::I32], vec![
        block(vec![], Terminator::SwitchInt { discriminant: Operand::Copy(place(1)),
            true_target: BasicBlockID(1), false_target: BasicBlockID(2) }),
        block(vec![assign(place(2), int(10))], go(3)),
        block(vec![assign(place(2), int(20))], go(3)),
        block(vec![read(3, place(2))], Terminator::Return),
    ]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}

#[test]
fn partial_move_preserves_sibling_but_invalidates_parent() {
    let ty = pair();
    let init = assign(place(1), Rvalue::Aggregate(AggregateKind::Struct(
        TypeRef::new(DefID(99), "test::Pair".into())),
        vec![Operand::Const(Constant::Int(1, Type::I32)), Operand::Const(Constant::Int(2, Type::I32))]));
    let base = vec![init.clone(), move_to(2, field(1, 0)), read(3, field(1, 1))];
    let good = fixture(Type::VOID, 0, vec![ty.clone(), Type::I32, Type::I32],
        vec![block(base.clone(), Terminator::Return)]);
    assert!(check(&good).is_ok(), "{:?}", check(&good));

    let mut bad = base;
    bad.push(read(4, place(1)));
    let bad = fixture(Type::VOID, 0, vec![ty, Type::I32, Type::I32, pair()],
        vec![block(bad, Terminator::Return)]);
    expect_uninitialized(&bad);
}

#[test]
fn reinitializing_moved_field_restores_whole_aggregate() {
    let mir = fixture(Type::VOID, 0, vec![pair(), Type::I32, pair()], vec![block(vec![
        assign(place(1), Rvalue::Aggregate(AggregateKind::Struct(
            TypeRef::new(DefID(99), "test::Pair".into())),
            vec![Operand::Const(Constant::Int(1, Type::I32)), Operand::Const(Constant::Int(2, Type::I32))])),
        move_to(2, field(1, 0)), assign(field(1, 0), int(3)), read(3, place(1)),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}

#[test]
fn two_moves_in_one_rvalue_are_rejected() {
    let mir = fixture(Type::VOID, 0, vec![Type::I32, Type::ARRAY(Box::new(Type::I32), 2)],
        vec![block(vec![assign(place(1), int(4)),
            assign(place(2), Rvalue::Aggregate(AggregateKind::Array(Type::I32),
                vec![Operand::Move(place(1)), Operand::Move(place(1))]))], Terminator::Return)]);
    expect_uninitialized(&mir);
}

#[test]
fn uninitialized_return_place_is_rejected() {
    let mir = fixture(Type::I32, 0, vec![], vec![block(vec![], Terminator::Return)]);
    expect_uninitialized(&mir);
}

#[test]
fn reassignment_after_move_restores_availability() {
    let mir = fixture(Type::VOID, 0, vec![Type::I32, Type::I32, Type::I32], vec![block(vec![
        assign(place(1), int(1)), move_to(2, place(1)), assign(place(1), int(2)),
        read(3, place(1)),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}
  
