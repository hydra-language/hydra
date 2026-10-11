use borrowcheck::borrowcheck::BorrowChecker;
use errors::error::{HydraError, Span};
use ir::{context::DefID, types::{Type, TypeRef}};
use mir::{
    AggregateKind, BasicBlock, BasicBlockID, FunctionRef, LocalDecl, LocalID,
    MIRFunction, Operand, Place, ProjectionElem, Rvalue, Statement,
    StatementKind, Terminator,
};

fn place(n: usize) -> Place { Place { local: LocalID(n), projection: vec![] } }

fn deref(n: usize) -> Place { Place { local: LocalID(n), projection: vec![ProjectionElem::Deref] } }

fn field(n: usize, index: usize, ty: Type) -> Place {
    Place { local: LocalID(n), projection: vec![ProjectionElem::Field { index, ty }] }
}

fn reference() -> Type { Type::CONST_REF(Box::new(Type::I32)) }

fn mutable_reference() -> Type { Type::REF(Box::new(Type::I32)) }

fn assign(dest: usize, rhs: Rvalue) -> Statement {
    Statement { kind: StatementKind::Assign(place(dest), rhs), span: Span::default() }
}

fn borrow(dest: usize, src: Place, mutable: bool) -> Statement {
    assign(dest, Rvalue::Ref(mutable, src))
}

fn copy(dest: usize, src: Place) -> Statement { assign(dest, Rvalue::Use(Operand::Copy(src))) }

fn block(statements: Vec<Statement>, terminator: Terminator) -> BasicBlock { BasicBlock { statements, terminator } }

fn call(dest: usize, args: Vec<Operand>, next: usize) -> Terminator {
    Terminator::Call {
        callee: FunctionRef {
            def_id: DefID(100), symbol: "test::get".into(), display_symbol: None,
        },
        args, destination: place(dest), target: BasicBlockID(next),
    }
}

fn function(return_type: Type, params: Vec<Type>, temps: Vec<Type>, blocks: Vec<BasicBlock>) -> MIRFunction {
    let arg_count = params.len();
    let locals = std::iter::once(return_type.clone()).chain(params).chain(temps)
        .enumerate().map(|(i, ty)| LocalDecl {
            ty, is_mutable: true, debug_def_id: None,
            debug_name: Some(format!("local_{i}")), debug_span: None,
        }).collect();

    MIRFunction {
        name: "return_provenance".into(), def_id: DefID(0), display_name: None,
        return_type, arg_count, locals, basic_blocks: blocks, is_inline: false,
    }
}

fn check(mir: &MIRFunction) -> Result<(), Vec<HydraError>> { BorrowChecker::new(mir).check() }

fn expect_code(mir: &MIRFunction, code: &str) {
    let errors = check(mir).expect_err("expected a borrow-checking error");
    assert!(errors.iter().any(|err| err.code == code), "expected {code}, got {errors:?}");
}

#[test]
fn returned_shared_reference_keeps_owner_borrowed() {
    let mir = function(Type::VOID, vec![Type::I32],
        vec![reference(), reference(), mutable_reference(), Type::I32, Type::I32], vec![
        block(vec![borrow(2, place(1), false)], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![borrow(4, place(1), true), copy(5, deref(3)),
            copy(6, deref(4))], Terminator::Return),
    ]);
    expect_code(&mir, "BC004");
}

#[test]
fn returned_reference_ends_at_last_use() {
    let mir = function(Type::VOID, vec![Type::I32],
        vec![reference(), reference(), Type::I32, mutable_reference(), Type::I32], vec![
        block(vec![borrow(2, place(1), false)], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![copy(4, deref(3)), borrow(5, place(1), true),
            copy(6, deref(5))], Terminator::Return),
    ]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}

#[test]
fn deref_result_and_reborrow_keep_original_owner_borrowed() {
    let array = Type::ARRAY(Box::new(Type::I32), 2);
    let slice_ref = Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::I32))));
    let mut_array_ref = Type::REF(Box::new(array.clone()));

    let mir = function(Type::VOID, vec![array.clone()], vec![
        Type::CONST_REF(Box::new(array)), slice_ref.clone(), slice_ref.clone(),
        mut_array_ref.clone(), slice_ref, mut_array_ref,
    ], vec![
        block(vec![borrow(2, place(1), false)], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![borrow(4, deref(3), false), borrow(5, place(1), true),
            copy(6, place(4)), copy(7, place(5))], Terminator::Return),
    ]);
    expect_code(&mir, "BC004");
}

#[test]
fn mutable_return_remains_exclusive() {
    let mir = function(Type::VOID, vec![Type::I32], vec![
        mutable_reference(), mutable_reference(), reference(),
        mutable_reference(), reference(),
    ], vec![
        block(vec![borrow(2, place(1), true)], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![borrow(4, place(1), false), copy(5, place(3)),
            copy(6, place(4))], Terminator::Return),
    ]);
    expect_code(&mir, "BC004");
}

#[test]
fn copied_returned_reference_keeps_provenance() {
    let mir = function(Type::VOID, vec![Type::I32], vec![
        reference(), reference(), reference(), mutable_reference(), Type::I32, Type::I32,
    ], vec![
        block(vec![borrow(2, place(1), false)], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![copy(4, place(3)), borrow(5, place(1), true),
            copy(6, deref(4)), copy(7, deref(5))], Terminator::Return),
    ]);
    expect_code(&mir, "BC004");
}

#[test]
fn return_may_borrow_from_either_reference_argument() {
    for target in [1, 2] {
        let mir = function(Type::VOID, vec![Type::I32, Type::I32],
            vec![reference(), reference(), reference(), mutable_reference(), Type::I32, Type::I32], vec![
            block(vec![borrow(3, place(1), false), borrow(4, place(2), false)],
                call(5, vec![Operand::Copy(place(3)), Operand::Copy(place(4))], 1)),
            block(vec![borrow(6, place(target), true), copy(7, deref(5)),
                copy(8, deref(6))], Terminator::Return),
        ]);
        expect_code(&mir, "BC004");
    }
}

#[test]
fn returned_reference_to_local_still_cannot_escape() {
    let mir = function(reference(), vec![Type::I32], vec![reference(), reference()], vec![
        block(vec![borrow(2, place(1), false)], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![copy(0, place(3))], Terminator::Return),
    ]);
    expect_code(&mir, "BC005");
}

#[test]
fn reference_parameter_can_be_copied_through_call_and_returned() {
    let mir = function(reference(), vec![reference()], vec![reference(), reference()], vec![
        block(vec![copy(2, place(1))], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![copy(0, place(3))], Terminator::Return),
    ]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}

#[test]
fn non_reference_call_result_does_not_extend_borrow() {
    let mir = function(Type::VOID, vec![Type::I32],
        vec![reference(), Type::I32, mutable_reference(), Type::I32], vec![
        block(vec![borrow(2, place(1), false)], call(3, vec![Operand::Copy(place(2))], 1)),
        block(vec![borrow(4, place(1), true), copy(5, deref(4))], Terminator::Return),
    ]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}

#[test]
fn returned_reference_from_no_safe_reference_arguments_has_no_local_origin() {
    let pointer = Type::CONST_POINTER(Box::new(Type::I32));
    let mir = function(reference(), vec![pointer], vec![reference()], vec![
        block(vec![], call(2, vec![Operand::Copy(place(1))], 1)),
        block(vec![copy(0, place(2))], Terminator::Return),
    ]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}

#[test]
fn distinct_reference_parameters_do_not_spuriously_conflict() {
    let mir = function(Type::VOID, vec![mutable_reference(), mutable_reference()],
        vec![Type::I32, Type::I32], vec![block(vec![
            copy(3, deref(1)), copy(4, deref(2)),
        ], Terminator::Return)]);

    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}

#[test]
fn projected_reference_argument_preserves_origin() {
    let holder = TypeRef::new(DefID(101), "test::Holder".into());
    let mir = function(Type::VOID, vec![Type::I32], vec![
        reference(), Type::STRUCT(holder.clone()), reference(),
        mutable_reference(), Type::I32, Type::I32,
    ], vec![
        block(vec![
            borrow(2, place(1), false),
            assign(3, Rvalue::Aggregate(
                AggregateKind::Struct(holder), vec![Operand::Copy(place(2)),
            ])),
        ], call(4, vec![Operand::Copy(field(3, 0, reference()))], 1)),
        block(vec![
            borrow(5, place(1), true), copy(6, deref(4)),
            copy(7, deref(5)),
        ], Terminator::Return),
    ]);
    expect_code(&mir, "BC004");
}

#[test]
fn reborrow_of_static_reference_return_is_allowed() {
    let mir = function(reference(), vec![], vec![reference(), reference()], vec![
        block(vec![], call(1, vec![], 1)),
        block(vec![borrow(2, deref(1), false), copy(0, place(2))], Terminator::Return),
    ]);
    assert!(check(&mir).is_ok(), "{:?}", check(&mir));
}
