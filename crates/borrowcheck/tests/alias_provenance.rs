use borrowcheck::borrowcheck::BorrowChecker;
use errors::error::{HydraError, Span};
use ir::{Constant, context::DefID, hir::CastKind, types::{Type, TypeRef}};
use mir::{
    BasicBlock, BasicBlockID, FunctionRef, LocalDecl, LocalID, MIRFunction,
    Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator,
};

fn place(n: usize) -> Place {
    Place { local: LocalID(n), projection: vec![] }
}

fn deref(n: usize) -> Place {
    Place { local: LocalID(n), projection: vec![ProjectionElem::Deref] }
}

fn field(n: usize, index: usize, ty: Type) -> Place {
    Place { local: LocalID(n), projection: vec![ProjectionElem::Field { index, ty }] }
}

fn assign(n: usize, rhs: Rvalue) -> Statement {
    Statement { kind: StatementKind::Assign(place(n), rhs), span: Span::default() }
}

fn borrow(n: usize, target: Place, mutable: bool) -> Statement {
    assign(n, Rvalue::Ref(mutable, target))
}

fn copy(n: usize, target: Place) -> Statement {
    assign(n, Rvalue::Use(Operand::Copy(target)))
}

fn constant(n: usize, value: i64) -> Statement {
    assign(n, Rvalue::Use(Operand::Const(Constant::Int(value, Type::I32))))
}

fn block(statements: Vec<Statement>, terminator: Terminator) -> BasicBlock {
    BasicBlock { statements, terminator }
}

fn call(name: &str, destination: usize, args: Vec<Operand>, next: usize) -> Terminator {
    Terminator::Call {
        callee: FunctionRef { def_id: DefID(100), symbol: name.into(), display_symbol: None },
        args, destination: place(destination), target: BasicBlockID(next),
    }
}

fn function(params: Vec<Type>, temps: Vec<Type>, blocks: Vec<BasicBlock>) -> MIRFunction {
    let arg_count = params.len();
    let locals = std::iter::once(Type::VOID).chain(params).chain(temps)
        .enumerate().map(|(i, ty)| LocalDecl {
            ty, is_mutable: true, debug_def_id: None,
            debug_name: Some(format!("local_{i}")), debug_span: None,
        }).collect();

    MIRFunction {
        name: "alias_provenance".into(), def_id: DefID(0), display_name: None,
        return_type: Type::VOID, arg_count, locals, basic_blocks: blocks, is_inline: false,
    }
}

fn check(mir: &MIRFunction) -> Result<(), Vec<HydraError>> {
    BorrowChecker::new(mir).check()
}

fn assert_valid(mir: &MIRFunction) {
    let result = check(mir);
    assert!(result.is_ok(), "expected valid borrow behavior: {result:#?}");
}

fn assert_error(mir: &MIRFunction, code: &str) {
    let errors = check(mir).expect_err("expected borrow conflict");
    assert!(errors.iter().any(|err| err.code == code), "expected {code}: {errors:#?}");
}

#[test]
fn raw_pointer_reference_does_not_conflict_with_unrelated_local_storage() {
    let raw = Type::POINTER(Box::new(Type::I32));
    let shared = Type::CONST_REF(Box::new(Type::I32));
    let mutable = Type::REF(Box::new(Type::I32));

    let mir = function(vec![raw], vec![
        Type::I32, shared.clone(), shared, mutable, Type::I32, Type::I32, Type::I32,
    ], vec![
        block(vec![
            constant(2, 10),
            borrow(3, deref(1), false),
            copy(4, place(3)),             // Copy borrowed reference into `value`.
            borrow(5, place(2), true),    // Mutably borrow independent storage.
        ], call("test::clone", 6, vec![Operand::Copy(place(4))], 1)),
        block(vec![
            copy(7, deref(4)),             // Keep raw-derived loan live.
            copy(8, deref(5)),
        ], Terminator::Return),
    ]);

    assert_valid(&mir);
}

#[test]
fn string_push_str_receiver_is_disjoint_from_source_reference() {
    let string_ref = TypeRef::new(DefID(20), "test::String".into());
    let vec_ref = TypeRef::new(DefID(21), "test::Vec".into());
    let bytes = Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::U8))));

    let mir = function(vec![
        Type::REF(Box::new(Type::STRUCT(string_ref))),
        Type::CONST_REF(Box::new(Type::STRING)),
    ], vec![
        bytes, Type::REF(Box::new(Type::STRUCT(vec_ref.clone()))),
        Type::USIZE, Type::U8, Type::VOID,
    ], vec![
        block(vec![],
            call("test::as_bytes", 3, vec![Operand::Copy(place(2))], 1)),
        block(vec![
            assign(5, Rvalue::Use(Operand::Const(Constant::Int(0, Type::USIZE)))),
            borrow(4, field(1, 0, Type::STRUCT(vec_ref)), true),
            copy(6, Place {
                local: LocalID(3),
                projection: vec![ProjectionElem::Deref, ProjectionElem::Index(LocalID(5))],
            }),
        ], call("test::push", 7,
            vec![Operand::Copy(place(4)), Operand::Copy(place(6))], 2)),
        block(vec![], Terminator::Return),
    ]);

    assert_valid(&mir);
}

#[test]
fn raw_pointer_cast_from_local_preserves_possible_alias() {
    let shared = Type::CONST_REF(Box::new(Type::I32));
    let raw = Type::CONST_POINTER(Box::new(Type::I32));

    let mir = function(vec![], vec![
        Type::I32, shared.clone(), raw.clone(), shared, Type::I32,
    ], vec![block(vec![
        constant(1, 10),
        borrow(2, place(1), false),
        assign(3, Rvalue::Cast(CastKind::Pointer, Operand::Copy(place(2)), raw)),
        borrow(4, deref(3), false),
        constant(1, 20),                // Must conflict with the live borrow.
        copy(5, deref(4)),
    ], Terminator::Return)]);

    assert_error(&mir, "BC001");
}

#[test]
fn two_borrows_through_same_raw_pointer_still_conflict() {
    let raw = Type::POINTER(Box::new(Type::I32));
    let shared = Type::CONST_REF(Box::new(Type::I32));
    let mutable = Type::REF(Box::new(Type::I32));

    let mir = function(vec![raw], vec![
        shared, mutable, Type::I32, Type::I32,
    ], vec![block(vec![
        borrow(2, deref(1), false),
        borrow(3, deref(1), true),
        copy(4, deref(2)),
        copy(5, deref(3)),
    ], Terminator::Return)]);

    assert_error(&mir, "BC004");
}

#[test]
fn mutation_through_same_safe_reference_origin_is_still_rejected() {
    let shared = Type::CONST_REF(Box::new(Type::I32));
    let mutable = Type::REF(Box::new(Type::I32));

    let mir = function(vec![Type::I32], vec![
        shared, mutable, Type::I32, Type::I32,
    ], vec![block(vec![
        borrow(2, place(1), false),
        borrow(3, place(1), true),
        copy(4, deref(2)),
        copy(5, deref(3)),
    ], Terminator::Return)]);

    assert_error(&mir, "BC004");
}
