use borrowcheck::borrowcheck::BorrowChecker;
use errors::error::Span;
use ir::{context::DefID, types::{Type, TypeRef}, Constant};
use mir::{
    BasicBlock, BasicBlockID, LocalDecl, LocalID, MIRFunction, Operand,
    Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator,
};

fn place(local: usize) -> Place {
    Place { local: LocalID(local), projection: Vec::new() }
}

fn stmt(destination: usize, value: Rvalue) -> Statement {
    Statement {
        kind: StatementKind::Assign(place(destination), value),
        span: Span::default(),
    }
}

fn borrow(destination: usize, target: usize, mutable: bool) -> Statement {
    stmt(destination, Rvalue::Ref(mutable, place(target)))
}

fn copy_ref(destination: usize, source: usize) -> Statement {
    stmt(destination, Rvalue::Use(Operand::Copy(place(source))))
}

fn write_int(target: usize) -> Statement {
    stmt(target, Rvalue::Use(Operand::Const(Constant::Int(42, Type::I32))))
}

fn use_ref(source: usize) -> Statement {
    stmt(6, Rvalue::Use(Operand::Copy(Place {
        local: LocalID(source),
        projection: vec![ProjectionElem::Deref],
    })))
}

fn block(statements: Vec<Statement>, terminator: Terminator) -> BasicBlock {
    BasicBlock { statements, terminator }
}

fn goto(index: usize) -> Terminator {
    Terminator::Goto { target: BasicBlockID(index) }
}

fn fixture(blocks: Vec<BasicBlock>, mutable_r: bool) -> MIRFunction {
    let types = [
        Type::VOID, Type::I32, Type::I32,
        if mutable_r { Type::REF(Box::new(Type::I32)) }
        else { Type::CONST_REF(Box::new(Type::I32)) },
        Type::CONST_REF(Box::new(Type::I32)), Type::BOOL, Type::I32,
    ];

    let locals = types.into_iter().enumerate().map(|(index, ty)| LocalDecl {
        ty, is_mutable: true, debug_def_id: None,
        debug_name: Some(format!("local_{index}")), debug_span: None,
    }).collect();

    MIRFunction {
        name: "loan_regression".into(),
        def_id: DefID(0),
        display_name: None,
        return_type: Type::VOID,
        arg_count: 5,
        locals,
        basic_blocks: blocks,
        is_inline: false,
    }
}

fn check(mir: &MIRFunction) -> Result<(), Vec<errors::error::HydraError>> {
    BorrowChecker::new(mir).check()
}

#[test]
fn overwritten_reference_no_longer_borrows_old_target() {
    let mir = fixture(vec![block(vec![
        borrow(3, 1, false),
        borrow(3, 2, false),
        write_int(1),
        use_ref(3),
    ], Terminator::Return)], false);

    assert!(check(&mir).is_ok());
}

#[test]
fn copied_reference_keeps_its_loan_after_source_is_overwritten() {
    let mir = fixture(vec![block(vec![
        borrow(3, 1, false),
        copy_ref(4, 3),
        borrow(3, 2, false),
        write_int(1),
        use_ref(4),
    ], Terminator::Return)], false);

    let errors = check(&mir).expect_err("copied reference must keep original loan live");
    assert!(errors.iter().any(|e| e.code == "BC001"), "{errors:?}");
}

#[test]
fn loan_expires_after_last_use_of_reference() {
    let mir = fixture(vec![block(vec![
        borrow(3, 1, false),
        use_ref(3),
        write_int(1),
    ], Terminator::Return)], false);

    assert!(check(&mir).is_ok());
}

#[test]
fn branch_join_preserves_loans_from_both_predecessors() {
    let mir = fixture(vec![
        block(vec![], Terminator::SwitchInt {
            discriminant: Operand::Copy(place(5)),
            true_target: BasicBlockID(1),
            false_target: BasicBlockID(2),
        }),
        block(vec![borrow(3, 1, false)], goto(3)),
        block(vec![borrow(3, 2, false)], goto(3)),
        block(vec![write_int(1), use_ref(3)], Terminator::Return),
    ], false);

    let errors = check(&mir).expect_err("x may be borrowed on the true predecessor");
    assert!(errors.iter().any(|e| e.code == "BC001"), "{errors:?}");
}

#[test]
fn borrow_from_other_branch_is_not_active_before_join() {
    let mir = fixture(vec![
        block(vec![], Terminator::SwitchInt {
            discriminant: Operand::Copy(place(5)),
            true_target: BasicBlockID(1),
            false_target: BasicBlockID(2),
        }),
        block(vec![borrow(3, 1, false)], goto(3)),
        block(vec![write_int(1), borrow(3, 2, false)], goto(3)),
        block(vec![use_ref(3)], Terminator::Return),
    ], false);

    assert!(check(&mir).is_ok());
}

#[test]
fn conflicting_borrow_creation_is_rejected_at_its_site() {
    let mir = fixture(vec![block(vec![
        borrow(3, 1, true),
        borrow(4, 1, false),
        use_ref(3),
        use_ref(4),
    ], Terminator::Return)], true);

    let errors = check(&mir).expect_err("shared borrow overlaps live mutable loan");
    assert!(errors.iter().any(|e| e.code == "BC004"), "{errors:?}");
}

#[test]
fn unreachable_borrow_does_not_constrain_reachable_code() {
    let mir = fixture(vec![
        block(vec![], goto(1)),
        block(vec![write_int(1)], Terminator::Return),
        block(vec![borrow(3, 1, true)], goto(1)),
    ], false);

    assert!(check(&mir).is_ok());
}

#[test]
fn unused_mutable_loan_does_not_block_later_shared_borrow() {
    let mir = fixture(vec![block(vec![
        borrow(3, 1, true),
        borrow(4, 1, false),
        use_ref(4),
    ], Terminator::Return)], true);

    assert!(check(&mir).is_ok());
}

#[test]
fn moving_reference_into_aggregate_keeps_referent_borrowed() {
    use mir::AggregateKind;

    let mut mir = fixture(vec![block(vec![
        borrow(3, 1, false),
        stmt(7, Rvalue::Aggregate(
            AggregateKind::Struct(TypeRef::new(DefID(99), "test::Holder".into())),
            vec![Operand::Move(place(3))],
        )),
        write_int(1),
        stmt(6, Rvalue::Use(Operand::Copy(Place {
            local: LocalID(7),
            projection: vec![
                ProjectionElem::Field {
                    index: 0,
                    ty: Type::CONST_REF(Box::new(Type::I32)),
                },
                ProjectionElem::Deref,
            ],
        }))),
    ], Terminator::Return)], false);

    mir.locals.push(LocalDecl {
        ty: Type::STRUCT(TypeRef::new(DefID(99), "test::Holder".into())),
        is_mutable: false,
        debug_def_id: None,
        debug_name: Some("holder".into()),
        debug_span: None,
    });

    let errors = check(&mir).expect_err("aggregate must retain moved reference loan");
    assert!(errors.iter().any(|e| e.code == "BC001"), "{errors:?}");
}

#[test]
fn borrow_used_only_by_call_terminator_is_live_before_call() {
    use mir::FunctionRef;

    let mir = fixture(vec![
        block(vec![borrow(3, 1, true), write_int(1)], Terminator::Call {
            callee: FunctionRef {
                def_id: DefID(50),
                symbol: "test::consume".into(),
                display_symbol: None,
            },
            args: vec![Operand::Copy(place(3))],
            destination: place(6),
            target: BasicBlockID(1),
        }),
        block(vec![], Terminator::Return),
    ], true);

    let errors = check(&mir).expect_err("terminator's argument must keep the loan live");
    assert!(errors.iter().any(|e| e.code == "BC001"), "{errors:?}");
}
