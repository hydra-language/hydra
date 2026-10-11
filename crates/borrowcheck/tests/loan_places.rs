use borrowcheck::borrowcheck::BorrowChecker;
use errors::error::{HydraError, Span};
use ir::{context::DefID, types::{Type, TypeRef}, Constant};
use mir::{AggregateKind, BasicBlock, BasicBlockID, LocalDecl, LocalID, MIRFunction, Operand,
    Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator};

fn place(local: usize) -> Place { Place { local: LocalID(local), projection: vec![] } }

fn field(local: usize, index: usize) -> Place {
    Place { local: LocalID(local), projection: vec![ProjectionElem::Field { index, ty: Type::I32 }] }
}

fn ref_field(local: usize, index: usize) -> Place {
    Place { local: LocalID(local), projection: vec![
        ProjectionElem::Field { index, ty: Type::CONST_REF(Box::new(Type::I32)) },
    ] }
}

fn deref(local: usize) -> Place {
    Place { local: LocalID(local), projection: vec![ProjectionElem::Deref] }
}

fn deref_field(local: usize, index: usize) -> Place {
    Place { local: LocalID(local), projection: vec![
        ProjectionElem::Field { index, ty: Type::CONST_REF(Box::new(Type::I32)) },
        ProjectionElem::Deref,
    ] }
}

fn assign(dest: Place, value: Rvalue) -> Statement {
    Statement { kind: StatementKind::Assign(dest, value), span: Span::default() }
}

fn borrow(dest: usize, target: Place, mutable: bool) -> Statement {
    assign(place(dest), Rvalue::Ref(mutable, target))
}

fn read(dest: usize, src: Place) -> Statement {
    assign(place(dest), Rvalue::Use(Operand::Copy(src)))
}

fn write(dest: Place) -> Statement {
    assign(dest, Rvalue::Use(Operand::Const(Constant::Int(7, Type::I32))))
}

fn use_ref(dest: usize, source: usize) -> Statement { read(dest, deref(source)) }

fn block(statements: Vec<Statement>, terminator: Terminator) -> BasicBlock {
    BasicBlock { statements, terminator }
}

fn pair() -> Type { Type::STRUCT(TypeRef::new(DefID(100), "test::Pair".into())) }

fn holder() -> Type { Type::STRUCT(TypeRef::new(DefID(101), "test::Holder".into())) }

fn fixture(blocks: Vec<BasicBlock>) -> MIRFunction {
    let types = [Type::VOID, pair(), pair(), Type::REF(Box::new(Type::I32)),
        Type::CONST_REF(Box::new(Type::I32)), Type::REF(Box::new(Type::I32)),
        Type::I32, holder()];
    let locals = types.into_iter().enumerate().map(|(i, ty)| LocalDecl {
        ty, is_mutable: true, debug_def_id: None, debug_name: Some(format!("local_{i}")), debug_span: None,
    }).collect();
    MIRFunction { name: "loan_places".into(), def_id: DefID(0), display_name: None,
        return_type: Type::VOID, arg_count: 2, locals, basic_blocks: blocks, is_inline: false }
}

fn check(mir: &MIRFunction) -> Result<(), Vec<HydraError>> { BorrowChecker::new(mir).check() }

fn expect_code(mir: &MIRFunction, code: &str) {
    let errors = check(mir).expect_err("expected a borrow-checking diagnostic");
    assert!(errors.iter().any(|e| e.code == code), "expected {code}, got {errors:?}");
}

#[test]
fn mutable_borrows_of_disjoint_fields_can_coexist() {
    let mir = fixture(vec![block(vec![
        borrow(3, field(1, 0), true), borrow(5, field(1, 1), true),
        use_ref(6, 3), use_ref(6, 5),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok());
}

#[test]
fn borrowing_a_field_still_restricts_writes_to_its_parent() {
    let mir = fixture(vec![block(vec![
        borrow(4, field(1, 0), false),
        assign(place(1), Rvalue::Use(Operand::Copy(place(2)))),
        use_ref(6, 4),
    ], Terminator::Return)]);
    expect_code(&mir, "BC001");
}

#[test]
fn writing_an_unborrowed_sibling_field_is_allowed() {
    let mir = fixture(vec![block(vec![
        borrow(4, field(1, 0), false), write(field(1, 1)), use_ref(6, 4),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok());
}

#[test]
fn reborrow_retains_original_target_after_parent_reference_is_reassigned() {
    let mir = fixture(vec![block(vec![
        borrow(3, field(1, 0), true), borrow(4, deref(3), false),
        borrow(3, field(2, 0), true), write(field(1, 0)), use_ref(6, 4),
    ], Terminator::Return)]);
    expect_code(&mir, "BC001");
}

#[test]
fn shared_reborrow_through_mutable_reference_is_allowed() {
    let mir = fixture(vec![block(vec![
        borrow(3, field(1, 0), true), borrow(4, deref(3), false), use_ref(6, 4),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok());
}

#[test]
fn shared_reborrow_prevents_writing_through_original_mutable_reference() {
    let mir = fixture(vec![block(vec![
        borrow(3, field(1, 0), true), borrow(4, deref(3), false),
        write(deref(3)), use_ref(6, 4),
    ], Terminator::Return)]);
    expect_code(&mir, "BC001");
}

#[test]
fn mutable_reborrow_from_shared_reference_is_rejected() {
    let mir = fixture(vec![block(vec![
        borrow(4, field(1, 0), false), borrow(5, deref(4), true), use_ref(6, 5),
    ], Terminator::Return)]);
    expect_code(&mir, "BC004");
}

#[test]
fn copied_aggregate_field_keeps_its_original_loan() {
    let mir = fixture(vec![block(vec![
        borrow(4, field(1, 0), false), borrow(5, field(2, 0), false),
        assign(place(7), Rvalue::Aggregate(AggregateKind::Struct(
            TypeRef::new(DefID(101), "test::Holder".into())), vec![
            Operand::Copy(place(4)), Operand::Copy(place(5)),
        ])),
        read(4, Place { local: LocalID(7), projection: vec![
            ProjectionElem::Field { index: 0, ty: Type::CONST_REF(Box::new(Type::I32)) },
        ] }),
        write(field(1, 0)), use_ref(6, 4),
    ], Terminator::Return)]);
    expect_code(&mir, "BC001");
}

#[test]
fn overwriting_one_aggregate_reference_field_does_not_preserve_its_old_loan() {
    let mir = fixture(vec![block(vec![
        borrow(4, field(1, 0), false), borrow(5, field(2, 0), false),
        assign(place(7), Rvalue::Aggregate(AggregateKind::Struct(
            TypeRef::new(DefID(101), "test::Holder".into())), vec![
            Operand::Copy(place(4)), Operand::Copy(place(5)),
        ])),
        assign(ref_field(7, 0), Rvalue::Use(Operand::Copy(place(5)))),
        write(field(1, 0)), read(6, deref_field(7, 0)),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok());
}

#[test]
fn target_set_survives_branch_join_and_parent_reassignment() {
    let mir = fixture(vec![
        block(vec![], Terminator::SwitchInt {
            discriminant: Operand::Copy(place(6)),
            true_target: BasicBlockID(1), false_target: BasicBlockID(2),
        }),
        block(vec![borrow(3, field(1, 0), true)], Terminator::Goto { target: BasicBlockID(3) }),
        block(vec![borrow(3, field(2, 0), true)], Terminator::Goto { target: BasicBlockID(3) }),
        block(vec![borrow(4, deref(3), false), borrow(3, field(2, 1), true),
            write(field(1, 0)), use_ref(6, 4)], Terminator::Return),
    ]);
    expect_code(&mir, "BC001");
}

#[test]
fn write_through_reference_updates_aggregate_field_loans() {
    let mut mir = fixture(vec![block(vec![
        borrow(4, field(1, 0), false), borrow(5, field(2, 0), false),
        assign(place(7), Rvalue::Aggregate(AggregateKind::Struct(
            TypeRef::new(DefID(101), "test::Holder".into())), vec![
            Operand::Copy(place(4)), Operand::Copy(place(5)),
        ])),
        borrow(8, place(7), true),
        assign(deref(8), Rvalue::Aggregate(AggregateKind::Struct(
            TypeRef::new(DefID(101), "test::Holder".into())), vec![
            Operand::Copy(place(4)), Operand::Copy(place(5)),
        ])),
        write(field(1, 0)), read(6, deref_field(7, 0)),
    ], Terminator::Return)]);
    mir.locals.push(LocalDecl { ty: Type::REF(Box::new(holder())), is_mutable: true,
        debug_def_id: None, debug_name: Some("holder_ref".into()), debug_span: None });
    expect_code(&mir, "BC001");
}
