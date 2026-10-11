use borrowcheck::borrowcheck::BorrowChecker;
use errors::error::{HydraError, Span};
use ir::{context::DefID, types::Type, Constant};
use mir::{AggregateKind, BasicBlock, LocalDecl, LocalID, MIRFunction, Operand, Place,
    ProjectionElem, Rvalue, Statement, StatementKind, Terminator};

fn place(local: usize) -> Place { Place { local: LocalID(local), projection: vec![] } }

fn deref(local: usize) -> Place {
    Place { local: LocalID(local), projection: vec![ProjectionElem::Deref] }
}

fn assign(line: usize, destination: Place, value: Rvalue) -> Statement {
    Statement { kind: StatementKind::Assign(destination, value), span: Span { line, column: 1, length: 1 } }
}

fn borrow(line: usize, destination: usize, target: Place, mutable: bool) -> Statement {
    assign(line, place(destination), Rvalue::Ref(mutable, target))
}

fn write(line: usize, destination: Place) -> Statement {
    assign(line, destination, Rvalue::Use(Operand::Const(Constant::Int(7, Type::I32))))
}

fn copy(line: usize, destination: usize, source: Place) -> Statement {
    assign(line, place(destination), Rvalue::Use(Operand::Copy(source)))
}

fn check(params: Vec<Type>, temps: Vec<Type>, statements: Vec<Statement>) -> Result<(), Vec<HydraError>> {
    let arg_count = params.len();
    let locals = std::iter::once(Type::VOID).chain(params).chain(temps)
        .enumerate().map(|(i, ty)| LocalDecl {
            ty, is_mutable: true, debug_def_id: None,
            debug_name: Some(format!("local_{i}")), debug_span: None,
        }).collect();
    let mir = MIRFunction {
        name: "diagnostic_alias_regression".into(), def_id: DefID(0), display_name: None,
        return_type: Type::VOID, arg_count, locals,
        basic_blocks: vec![BasicBlock { statements, terminator: Terminator::Return }], is_inline: false,
    };
    BorrowChecker::new(&mir).check()
}

#[test]
fn external_reference_reborrow_does_not_alias_a_distinct_stack_local() {
    assert!(check(vec![Type::CONST_REF(Box::new(Type::I32))],
        vec![Type::I32, Type::CONST_REF(Box::new(Type::I32)), Type::I32], vec![
            borrow(1, 3, deref(1), false), write(2, place(2)), copy(3, 4, deref(3)),
        ]).is_ok());
}

#[test]
fn reassigning_a_reference_slot_does_not_write_its_old_pointee() {
    let reference = Type::CONST_REF(Box::new(Type::I32));
    assert!(check(vec![reference.clone(), reference.clone()], vec![reference, Type::I32], vec![
        borrow(1, 3, deref(1), false), copy(2, 1, place(2)), copy(3, 4, deref(3)),
    ]).is_ok());
}

#[test]
fn unknown_raw_pointer_write_may_alias_shared_reference_parameter() {
    let errors = check(
        vec![
            Type::POINTER(Box::new(Type::I32)),
            Type::CONST_REF(Box::new(Type::I32)),
        ],
        vec![Type::CONST_REF(Box::new(Type::I32)), Type::I32],
        vec![
            borrow(1, 3, deref(2), false),
            write(2, deref(1)),
            copy(3, 4, deref(3)),
        ],
    ).expect_err("raw pointer may alias the shared reference parameter");

    assert!(errors.iter().any(|e| e.code == "BC001"), "{errors:?}");
    assert!(!errors.iter().any(|e| e.code == "BC003"), "test must not rely on uninitialized values: {errors:?}");
}

#[test]
fn duplicate_accesses_in_one_statement_produce_one_diagnostic() {
    let errors = check(vec![Type::I32], vec![Type::REF(Box::new(Type::I32)),
        Type::ARRAY(Box::new(Type::I32), 2), Type::I32], vec![
        borrow(1, 2, place(1), true),
        assign(2, place(3), Rvalue::Aggregate(AggregateKind::Array(Type::I32),
            vec![Operand::Copy(place(1)), Operand::Copy(place(1))])),
        copy(3, 4, deref(2)),
    ]).expect_err("reading a mutably borrowed value must be rejected");
    let conflicts: Vec<_> = errors.iter().filter(|e| e.code == "BC002").collect();
    assert_eq!(conflicts.len(), 1, "{errors:?}");
    assert_eq!(conflicts[0].span.line, 2);
    assert_eq!(conflicts[0].notes[0].span.line, 1);
}

#[test]
fn raw_pointer_slot_is_not_assumed_disjoint_from_its_pointee() {
    let pointer = Type::POINTER(Box::new(Type::I32));
    let errors = check(vec![pointer.clone(), pointer],
        vec![Type::CONST_REF(Box::new(Type::I32)), Type::I32], vec![
            borrow(1, 3, deref(1), false), copy(2, 1, place(2)), copy(3, 4, deref(3)),
        ]).expect_err("raw pointer may point into its own stack slot");
    assert!(errors.iter().any(|e| e.code == "BC001"), "{errors:?}");
}
