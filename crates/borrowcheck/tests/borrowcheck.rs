use borrowcheck::borrowcheck::BorrowChecker;

use errors::error::{HydraError, Span};
use ir::{Constant, context::DefID, types::Type};
use mir::{
    AggregateKind, BasicBlock, BasicBlockID, FunctionRef, LocalDecl, LocalID,
    MIRFunction, Operand, Place, ProjectionElem, Rvalue, Statement,
    StatementKind, Terminator,
};

struct MirFixture {
    locals: Vec<LocalDecl>,
}

impl MirFixture {
    fn new() -> Self {
        Self {
            locals: vec![LocalDecl {
                ty: Type::VOID,
                is_mutable: true,
                debug_def_id: None,
                debug_name: None,
                debug_span: None,
            }],
        }
    }

    fn local(&mut self, name: &str, ty: Type) -> LocalID {
        let id = LocalID(self.locals.len());
        self.locals.push(LocalDecl {
            ty,
            is_mutable: true,
            debug_def_id: Some(DefID(id.0)),
            debug_name: Some(name.into()),
            debug_span: Some(Span::default()),
        });
        id
    }

    fn finish(self, statements: Vec<Statement>) -> MIRFunction {
        MIRFunction {
            name: "test".into(),
            def_id: DefID(0),
            display_name: None,
            return_type: Type::VOID,
            arg_count: 0,
            locals: self.locals,
            basic_blocks: vec![BasicBlock {
                statements,
                terminator: Terminator::Return,
            }],
            is_inline: false,
        }
    }
}

fn span(line: usize) -> Span {
    Span { line, column: 1, length: 1 }
}

fn place(local: LocalID) -> Place {
    Place { local, projection: vec![] }
}

fn copy(local: LocalID) -> Operand {
    Operand::Copy(place(local))
}

fn move_(local: LocalID) -> Operand {
    Operand::Move(place(local))
}

fn int(value: i64) -> Operand {
    Operand::Const(Constant::Int(value, Type::I32))
}

fn assign(line: usize, local: LocalID, value: Rvalue) -> Statement {
    Statement {
        kind: StatementKind::Assign(place(local), value),
        span: span(line),
    }
}

fn use_refs(line: usize, destination: LocalID, refs: &[LocalID]) -> Statement {
    Statement {
        kind: StatementKind::Assign(
            place(destination),
            Rvalue::Aggregate(
                AggregateKind::Array(Type::CONST_REF(Box::new(Type::I32))),
                refs.iter().copied().map(copy).collect(),
            ),
        ),
        span: span(line),
    }
}

fn assert_error(result: Result<(), Vec<HydraError>>, code: &str, message: &str) {
    let errors = result.expect_err("borrow checking should reject the program");
    assert!(
        errors.iter().any(|error| error.code == code && error.message.contains(message)),
        "expected {code} containing `{message}`, got {errors:#?}",
    );
}

#[test]
fn shared_borrows_can_coexist() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let a = fixture.local("a", Type::CONST_REF(Box::new(Type::I32)));
    let b = fixture.local("b", Type::CONST_REF(Box::new(Type::I32)));
    let refs = fixture.local("refs", Type::ARRAY(Box::new(Type::CONST_REF(Box::new(Type::I32))), 2));

    let mir = fixture.finish(vec![
        assign(1, a, Rvalue::Ref(false, place(x))),
        assign(2, b, Rvalue::Ref(false, place(x))),
        use_refs(3, refs, &[a, b]),
    ]);

    let result = BorrowChecker::new(&mir).check();
    assert!(result.is_ok(), "multiple shared borrows should be allowed: {result:#?}");
}

#[test]
fn two_live_mutable_borrows_conflict() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let a = fixture.local("a", Type::REF(Box::new(Type::I32)));
    let b = fixture.local("b", Type::REF(Box::new(Type::I32)));
    let refs = fixture.local("refs", Type::ARRAY(Box::new(Type::REF(Box::new(Type::I32))), 2));

    let mir = fixture.finish(vec![
        assign(1, a, Rvalue::Ref(true, place(x))),
        assign(2, b, Rvalue::Ref(true, place(x))),
        use_refs(3, refs, &[a, b]),
    ]);

    assert_error(
        BorrowChecker::new(&mir).check(),
        "BC004",
        "cannot borrow `x` as mutable more than once",
    );
}

#[test]
fn mutable_and_shared_borrow_conflict() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let mutable = fixture.local("mutable", Type::REF(Box::new(Type::I32)));
    let shared = fixture.local("shared", Type::CONST_REF(Box::new(Type::I32)));
    let refs = fixture.local("refs", Type::ARRAY(Box::new(Type::CONST_REF(Box::new(Type::I32))), 2));

    let mir = fixture.finish(vec![
        assign(1, mutable, Rvalue::Ref(true, place(x))),
        assign(2, shared, Rvalue::Ref(false, place(x))),
        use_refs(3, refs, &[mutable, shared]),
    ]);

    let errors = BorrowChecker::new(&mir).check()
        .expect_err("mutable and shared borrow must conflict");
    assert!(
        errors.iter().any(|error| error.code == "BC004"),
        "expected BC004, got {errors:#?}",
    );
}

#[test]
fn cannot_assign_to_value_while_shared_borrow_is_live() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let reference = fixture.local("reference", Type::CONST_REF(Box::new(Type::I32)));
    let use_reference = fixture.local("use_reference", Type::CONST_REF(Box::new(Type::I32)));

    let mir = fixture.finish(vec![
        assign(1, reference, Rvalue::Ref(false, place(x))),
        assign(2, x, Rvalue::Use(int(42))),
        assign(3, use_reference, Rvalue::Use(copy(reference))),
    ]);

    assert_error(
        BorrowChecker::new(&mir).check(),
        "BC001",
        "cannot assign to `x`",
    );
}

#[test]
fn cannot_read_value_while_mutably_borrowed() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let reference = fixture.local("reference", Type::REF(Box::new(Type::I32)));
    let read_x = fixture.local("read_x", Type::I32);
    let use_reference = fixture.local("use_reference", Type::REF(Box::new(Type::I32)));

    let mir = fixture.finish(vec![
        assign(1, reference, Rvalue::Ref(true, place(x))),
        assign(2, read_x, Rvalue::Use(copy(x))),
        assign(3, use_reference, Rvalue::Use(copy(reference))),
    ]);

    assert_error(
        BorrowChecker::new(&mir).check(),
        "BC002",
        "cannot use `x`",
    );
}

#[test]
fn borrow_ends_after_last_use() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let reference = fixture.local("reference", Type::REF(Box::new(Type::I32)));
    let last_use = fixture.local("last_use", Type::REF(Box::new(Type::I32)));

    let mir = fixture.finish(vec![
        assign(1, reference, Rvalue::Ref(true, place(x))),
        assign(2, last_use, Rvalue::Use(copy(reference))),
        assign(3, x, Rvalue::Use(int(42))),
    ]);

    let result = BorrowChecker::new(&mir).check();
    assert!(result.is_ok(), "borrow should end after its last use: {result:#?}");
}

#[test]
fn copied_reference_preserves_borrow_information() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let original = fixture.local("original", Type::REF(Box::new(Type::I32)));
    let alias = fixture.local("alias", Type::REF(Box::new(Type::I32)));
    let shared = fixture.local("shared", Type::CONST_REF(Box::new(Type::I32)));
    let refs = fixture.local("refs", Type::ARRAY(Box::new(Type::CONST_REF(Box::new(Type::I32))), 2));

    let mir = fixture.finish(vec![
        assign(1, original, Rvalue::Ref(true, place(x))),
        assign(2, alias, Rvalue::Use(copy(original))),
        assign(3, shared, Rvalue::Ref(false, place(x))),
        use_refs(4, refs, &[alias, shared]),
    ]);

    let errors = BorrowChecker::new(&mir).check()
        .expect_err("copied mutable reference must retain its borrow");
    assert!(
        errors.iter().any(|error| error.code == "BC004"),
        "expected BC004 through copied reference, got {errors:#?}",
    );
}

#[test]
fn using_value_after_move_is_rejected() {
    let mut fixture = MirFixture::new();
    let ty = Type::ARRAY(Box::new(Type::I32), 1);
    let value = fixture.local("value", ty.clone());
    let moved_into = fixture.local("moved_into", ty.clone());
    let used_again = fixture.local("used_again", ty);

    let mir = fixture.finish(vec![
        assign(1, moved_into, Rvalue::Use(move_(value))),
        assign(2, used_again, Rvalue::Use(move_(value))),
    ]);

    assert_error(
        BorrowChecker::new(&mir).check(),
        "BC003",
        "use of moved value `value`",
    );
}

#[test]
fn reassigning_local_after_move_reinitializes_it() {
    let mut fixture = MirFixture::new();
    let ty = Type::ARRAY(Box::new(Type::I32), 1);
    let value = fixture.local("value", ty.clone());
    let moved_into = fixture.local("moved_into", ty.clone());
    let used_again = fixture.local("used_again", ty);

    let mir = fixture.finish(vec![
        assign(1, moved_into, Rvalue::Use(move_(value))),
        assign(2, value, Rvalue::Aggregate(AggregateKind::Array(Type::I32), vec![int(123)])),
        assign(3, used_again, Rvalue::Use(move_(value))),
    ]);

    let result = BorrowChecker::new(&mir).check();
    assert!(result.is_ok(), "reinitialized local should be usable again: {result:#?}");
}

#[test]
fn using_value_after_drop_is_rejected() {
    let mut fixture = MirFixture::new();
    let ty = Type::ARRAY(Box::new(Type::I32), 1);
    let value = fixture.local("value", ty.clone());
    let used_again = fixture.local("used_again", ty);

    let mir = fixture.finish(vec![
        Statement {
            kind: StatementKind::Drop {
                place: place(value),
                callee: FunctionRef {
                    def_id: DefID(100),
                    symbol: "test::drop".into(),
                    display_symbol: None,
                },
            },
            span: span(1),
        },
        assign(2, used_again, Rvalue::Use(move_(value))),
    ]);

    let errors = BorrowChecker::new(&mir).check()
        .expect_err("using a dropped value must fail");
    assert!(
        errors.iter().any(|error| error.code == "BC003"),
        "expected BC003 after drop, got {errors:#?}",
    );
}

#[test]
fn direct_assignment_kills_previous_local_value() {
    let mut fixture = MirFixture::new();
    let x = fixture.local("x", Type::I32);
    let mir = fixture.finish(vec![assign(1, x, Rvalue::Use(int(42)))]);

    let checker = BorrowChecker::new(&mir);
    let effects = checker.compute_gen_kill();
    let bb0 = effects.get(&BasicBlockID(0)).expect("bb0 should have dataflow effects");

    assert!(bb0.kill.contains(&x), "direct assignment should kill the previous value");
    assert!(!bb0.gen.contains(&x), "direct assignment should not read x first");
}

#[test]
fn assignment_to_projection_reads_base_instead_of_killing_it() {
    let mut fixture = MirFixture::new();
    let object = fixture.local("object", Type::ARRAY(Box::new(Type::I32), 1));
    let index = fixture.local("index", Type::USIZE);
    let projection = Place {
        local: object,
        projection: vec![ProjectionElem::Index(index)],
    };

    let mir = fixture.finish(vec![
        assign(1, index, Rvalue::Use(Operand::Const(Constant::Int(0, Type::USIZE)))),
        Statement {
            kind: StatementKind::Assign(projection, Rvalue::Use(int(42))),
            span: span(2),
        },
    ]);

    let checker = BorrowChecker::new(&mir);
    let effects = checker.compute_gen_kill();
    let bb0 = effects.get(&BasicBlockID(0)).expect("bb0 should have dataflow effects");

    assert!(bb0.gen.contains(&object), "projection assignment must read its base");
    assert!(!bb0.kill.contains(&object), "projected writes must not kill the base local");
}
