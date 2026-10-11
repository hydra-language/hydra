use borrowcheck::borrowcheck::BorrowChecker;
use errors::error::Span;
use ir::{context::DefID, types::{Type, TypeRef}};
use mir::{
    AggregateKind, BasicBlock, LocalDecl, LocalID, MIRFunction, Operand,
    Place, Rvalue, Statement, StatementKind, Terminator,
};

fn item() -> Type {
    Type::STRUCT(TypeRef::new(DefID(99), "test::Item".into()))
}

fn place(index: usize) -> Place {
    Place { local: LocalID(index), projection: vec![] }
}

fn local(ty: Type) -> LocalDecl {
    LocalDecl {
        ty, is_mutable: false, debug_def_id: None,
        debug_name: None, debug_span: None,
    }
}

fn fixture(operands: Vec<Operand>) -> MIRFunction {
    let item_ty = item();
    MIRFunction {
        name: "test_moves".into(), def_id: DefID(0), display_name: None,
        return_type: Type::VOID, arg_count: 2,
        locals: vec![
            local(Type::VOID), local(item_ty.clone()), local(item_ty.clone()),
            local(Type::ARRAY(Box::new(item_ty.clone()), 2)),
        ],
        basic_blocks: vec![BasicBlock {
            statements: vec![Statement {
                kind: StatementKind::Assign(
                    place(3),
                    Rvalue::Aggregate(AggregateKind::Array(item_ty), operands),
                ),
                span: Span::default(),
            }],
            terminator: Terminator::Return,
        }],
        is_inline: false,
    }
}

#[test]
fn moving_the_same_local_twice_in_one_aggregate_is_rejected() {
    let mir = fixture(vec![Operand::Move(place(1)), Operand::Move(place(1))]);
    let errors = BorrowChecker::new(&mir).check().expect_err("double move must fail");
    assert!(errors.iter().any(|error| error.code == "BC003"), "{errors:?}");
}

#[test]
fn moving_distinct_locals_in_one_aggregate_is_allowed() {
    let mir = fixture(vec![Operand::Move(place(1)), Operand::Move(place(2))]);
    assert!(BorrowChecker::new(&mir).check().is_ok());
}

fn borrowed_item_fixture(conflicting: StatementKind) -> MIRFunction {
    use mir::ProjectionElem;

    let read_borrow = StatementKind::Assign(place(4), Rvalue::Use(Operand::Copy(Place {
        local: LocalID(2),
        projection: vec![
            ProjectionElem::Deref,
            ProjectionElem::Field { index: 0, ty: Type::I32 },
        ],
    })));

    MIRFunction {
        name: "test_borrow".into(), def_id: DefID(1), display_name: None,
        return_type: Type::VOID, arg_count: 1,
        locals: vec![
            local(Type::VOID), local(item()),
            local(Type::CONST_REF(Box::new(item()))),
            local(item()), local(Type::I32),
        ],
        basic_blocks: vec![BasicBlock {
            statements: vec![
                Statement {
                    kind: StatementKind::Assign(place(2), Rvalue::Ref(false, place(1))),
                    span: Span::default(),
                },
                Statement { kind: conflicting, span: Span::default() },
                Statement { kind: read_borrow, span: Span::default() },
            ],
            terminator: Terminator::Return,
        }],
        is_inline: false,
    }
}

#[test]
fn writing_a_field_of_an_immutably_borrowed_local_is_rejected() {
    use ir::Constant;
    use mir::ProjectionElem;

    let write = StatementKind::Assign(
        Place {
            local: LocalID(1),
            projection: vec![ProjectionElem::Field { index: 0, ty: Type::I32 }],
        },
        Rvalue::Use(Operand::Const(Constant::Int(20, Type::I32))),
    );

    let mir = borrowed_item_fixture(write);
    let errors = BorrowChecker::new(&mir).check().expect_err("projected write must fail");
    assert!(errors.iter().any(|error| error.code == "BC001"), "{errors:?}");
}

#[test]
fn moving_a_local_while_an_immutable_borrow_is_live_is_rejected() {
    let mir = borrowed_item_fixture(StatementKind::Assign(
        place(3), Rvalue::Use(Operand::Move(place(1))),
    ));
    let errors = BorrowChecker::new(&mir).check().expect_err("move through live borrow must fail");
    assert!(errors.iter().any(|error| error.code == "BC001"), "{errors:?}");
}

#[test]
fn dropping_a_borrowed_local_is_rejected() {
    use mir::FunctionRef;

    let drop_stmt = StatementKind::Drop {
        place: place(1),
        callee: FunctionRef {
            def_id: DefID(100),
            symbol: "test::Item::drop".into(),
            display_symbol: None,
        },
    };

    let mir = borrowed_item_fixture(drop_stmt);
    let errors = BorrowChecker::new(&mir).check().expect_err("dropping borrowed owner must fail");
    assert!(errors.iter().any(|error| error.code == "BC001"), "{errors:?}");
}
