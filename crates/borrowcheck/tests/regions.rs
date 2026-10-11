use borrowcheck::borrowcheck::BorrowChecker;

use errors::error::{HydraError, Span};

use ir::{context::DefID, types::{Type, TypeRef}, Constant};
use mir::{
    AggregateKind, BasicBlock, BasicBlockID, LocalDecl, LocalID, MIRFunction, Operand,
    Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator
};

fn place(local: usize) -> Place { Place { local: LocalID(local), projection: vec![] } }

fn deref(local: usize) -> Place { Place { local: LocalID(local), projection: vec![ProjectionElem::Deref] } }

fn ref_ty() -> Type { Type::CONST_REF(Box::new(Type::I32)) }

fn holder_ty() -> Type { Type::STRUCT(TypeRef::new(DefID(100), "test::Holder".into())) }

fn statement(destination: Place, value: Rvalue) -> Statement {
    Statement { kind: StatementKind::Assign(destination, value), span: Span::default() }
}

fn borrow(destination: usize, target: Place, mutable: bool) -> Statement {
    statement(place(destination), Rvalue::Ref(mutable, target))
}

fn copy(destination: usize, source: Place) -> Statement {
    statement(place(destination), Rvalue::Use(Operand::Copy(source)))
}

fn write_int(destination: usize) -> Statement {
    statement(place(destination), Rvalue::Use(Operand::Const(Constant::Int(7, Type::I32))))
}

fn block(statements: Vec<Statement>, terminator: Terminator) -> BasicBlock {
    BasicBlock { statements, terminator }
}

fn function(return_type: Type, params: Vec<Type>, temps: Vec<Type>, blocks: Vec<BasicBlock>) -> MIRFunction {
    let arg_count = params.len();
    let locals = std::iter::once(return_type.clone()).chain(params).chain(temps)
        .enumerate().map(|(index, ty)| LocalDecl {
            ty, is_mutable: true, debug_def_id: None,
            debug_name: Some(format!("local_{index}")), debug_span: None,
        }).collect();

    MIRFunction { name: "region_regression".into(), def_id: DefID(0), display_name: None,
        return_type, arg_count, locals, basic_blocks: blocks, is_inline: false }
}

fn check(mir: &MIRFunction) -> Result<(), Vec<HydraError>> {
    BorrowChecker::new(mir).check()
}

fn expect_code(mir: &MIRFunction, code: &str) {
    let errors = check(mir).expect_err("expected borrow error");
    assert!(errors.iter().any(|e| e.code == code), "expected {code}, got {errors:?}");
}

#[test]
fn returned_reference_to_local_is_rejected() {
    let mir = function(ref_ty(), vec![], vec![Type::I32, ref_ty()], vec![block(vec![
        borrow(2, place(1), false), copy(0, place(2)),
    ], Terminator::Return)]);
    expect_code(&mir, "BC005");
}

#[test]
fn returned_reference_to_owned_parameter_is_rejected() {
    let mir = function(ref_ty(), vec![Type::I32], vec![ref_ty()], vec![block(vec![
        borrow(2, place(1), false), copy(0, place(2)),
    ], Terminator::Return)]);
    expect_code(&mir, "BC005");
}

#[test]
fn returning_borrowed_parameter_directly_is_allowed() {
    let mir = function(ref_ty(), vec![ref_ty()], vec![], vec![block(vec![
        copy(0, place(1)),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok());
}

#[test]
fn returning_reborrow_of_borrowed_parameter_is_allowed() {
    let mir = function(ref_ty(), vec![ref_ty()], vec![ref_ty()], vec![block(vec![
        borrow(2, deref(1), false), copy(0, place(2)),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok());
}

#[test]
fn returned_aggregate_cannot_hide_a_stack_reference() {
    let mir = function(holder_ty(), vec![], vec![Type::I32, ref_ty()], vec![block(vec![
        borrow(2, place(1), false),
        statement(place(0), Rvalue::Aggregate(AggregateKind::Struct(
            TypeRef::new(DefID(100), "test::Holder".into())), vec![Operand::Move(place(2))])),
    ], Terminator::Return)]);
    expect_code(&mir, "BC005");
}

#[test]
fn returning_local_through_a_reborrow_is_rejected() {
    let mir = function(ref_ty(), vec![], vec![Type::I32, Type::REF(Box::new(Type::I32)), ref_ty()],
        vec![block(vec![
            borrow(2, place(1), true), borrow(3, deref(2), false), copy(0, place(3)),
        ], Terminator::Return)]);
    expect_code(&mir, "BC005");
}

#[test]
fn borrow_ends_after_last_use_even_when_value_is_still_live() {
    let mir = function(Type::VOID, vec![Type::I32], vec![ref_ty(), Type::I32], vec![block(vec![
        borrow(2, place(1), false), copy(3, deref(2)), write_int(1), copy(3, place(1)),
    ], Terminator::Return)]);
    assert!(check(&mir).is_ok());
}

#[test]
fn an_overwritten_reference_field_does_not_keep_its_old_loan_live() {
    let ref_field = |index| Place { local: LocalID(6), projection: vec![
        ProjectionElem::Field { index, ty: ref_ty() },
    ] };
    let read_second = Place { local: LocalID(6), projection: vec![
        ProjectionElem::Field { index: 1, ty: ref_ty() }, ProjectionElem::Deref,
    ] };

    let mir = function(Type::VOID, vec![Type::I32, Type::I32],
        vec![ref_ty(), ref_ty(), Type::I32, holder_ty()], vec![block(vec![
            borrow(3, place(1), false), borrow(4, place(2), false),
            statement(place(6), Rvalue::Aggregate(AggregateKind::Struct(
                TypeRef::new(DefID(100), "test::Holder".into())),
                vec![Operand::Copy(place(3)), Operand::Copy(place(4))])),
            statement(ref_field(0), Rvalue::Use(Operand::Copy(place(4)))),
            write_int(1), copy(5, read_second),
        ], Terminator::Return)]);

    assert!(check(&mir).is_ok());
}

#[test]
fn reborrow_remains_active_after_parent_reference_is_overwritten() {
    let mir = function(Type::VOID, vec![Type::I32, Type::I32],
        vec![Type::REF(Box::new(Type::I32)), ref_ty(), Type::I32], vec![block(vec![
            borrow(3, place(1), true), borrow(4, deref(3), false),
            borrow(3, place(2), true), write_int(1), copy(5, deref(4)),
        ], Terminator::Return)]);
    expect_code(&mir, "BC001");
}

#[test]
fn branch_join_preserves_both_potential_return_escapes() {
    let mir = function(ref_ty(), vec![Type::BOOL], vec![Type::I32, Type::I32, ref_ty()], vec![
        block(vec![], Terminator::SwitchInt {
            discriminant: Operand::Copy(place(1)),
            true_target: BasicBlockID(1), false_target: BasicBlockID(2),
        }),
        block(vec![borrow(4, place(2), false), copy(0, place(4))],
            Terminator::Goto { target: BasicBlockID(3) }),
        block(vec![borrow(4, place(3), false), copy(0, place(4))],
            Terminator::Goto { target: BasicBlockID(3) }),
        block(vec![], Terminator::Return),
    ]);
    expect_code(&mir, "BC005");
}

#[test]
fn reborrow_of_parameter_does_not_borrow_unrelated_local() {
    let mir = function(
        Type::VOID,
        vec![ref_ty()],
        vec![Type::I32, ref_ty(), Type::I32],
        vec![block(vec![
            borrow(3, deref(1), false),
            write_int(2),
            copy(4, deref(3)),
        ], Terminator::Return)],
    );

    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}
