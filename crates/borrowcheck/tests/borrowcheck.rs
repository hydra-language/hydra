use borrowcheck::borrowcheck::BorrowChecker;

use errors::error::Span;

use ir::{
    Constant,
    context::{
        DefID,
        DefKind,
        HIRContext,
        SymbolInfo,
    },
    types::Type,
};

use mir::{
    AggregateKind,
    BasicBlock,
    BasicBlockID,
    LocalDecl,
    LocalID,
    MIRFunction,
    Operand,
    Place,
    ProjectionElem,
    Rvalue,
    Statement,
    StatementKind,
    Terminator,
};

// ============================================================================
// TEST FIXTURE
// ============================================================================

struct MirFixture {
    context: HIRContext,
    function_def: DefID,
    locals: Vec<LocalDecl>,
}

impl MirFixture {

    fn new() -> Self {
        let mut context = HIRContext::new();

        let function_def = context.insert_def(SymbolInfo {
            name: "test".to_string(),
            span: Span::default(),
            absolute_path: vec!["test".to_string()],
            kind: DefKind::Function {
                params: vec![],
                annotations: vec![],
                return_type: Type::VOID,
                generic_params: vec![],
                owner_generic_count: 0,
                intrinsic: None,
            },
            is_pub: false,
        });

        //
        // MIR local zero is always the return place.
        //
        let locals = vec![
            LocalDecl {
                ty: Type::VOID,
                is_mutable: true,
                debug_def_id: None,
            },
        ];

        Self {
            context,
            function_def,
            locals,
        }
    }

    fn local(&mut self, name: &str, ty: Type) -> LocalID {
        let def_id = self.context.insert_def(SymbolInfo {
            name: name.to_string(),
            span: Span::default(),
            absolute_path: vec![],
            kind: DefKind::Variable {
                ty: ty.clone(),
                is_mutable: true,
            },
            is_pub: false,
        });

        let local = LocalID(self.locals.len());

        self.locals.push(LocalDecl {
            ty,
            is_mutable: true,
            debug_def_id: Some(def_id),
        });

        local
    }

    fn finish(self, statements: Vec<Statement>) -> (HIRContext, MIRFunction) {
        let mir = MIRFunction {
            name: "test".to_string(),
            def_id: self.function_def,
            return_type: Type::VOID,
            arg_count: 0,
            locals: self.locals,
            basic_blocks: vec![
                BasicBlock {
                    statements,
                    terminator: Terminator::Return,
                },
            ],
            is_inline: false,
        };

        (self.context, mir)
    }
}

// ============================================================================
// HELPERS
// ============================================================================

fn span(line: usize) -> Span {
    Span {
        line,
        column: 1,
        length: 1,
    }
}

fn place(local: LocalID) -> Place {
    Place {
        local,
        projection: vec![],
    }
}

fn copy(local: LocalID) -> Operand {
    Operand::Copy(place(local))
}

fn move_(local: LocalID) -> Operand {
    Operand::Move(place(local))
}

fn int(value: i64) -> Operand {
    Operand::Const(
        Constant::Int(
            value,
            Type::I32,
        )
    )
}

fn assign(line: usize, local: LocalID, rvalue: Rvalue) -> Statement {
    Statement {
        kind: StatementKind::Assign(
            place(local),
            rvalue,
        ),
        span: span(line),
    }
}

fn use_refs(line: usize, destination: LocalID, refs: &[LocalID]) -> Statement {
    Statement {
        kind: StatementKind::Assign(
            place(destination),
            Rvalue::Aggregate(
                AggregateKind::Array(
                    Type::CONST_REF(
                        Box::new(Type::I32),
                    ),
                ),
                refs
                    .iter()
                    .copied()
                    .map(copy)
                    .collect(),
            ),
        ),
        span: span(line),
    }
}

fn error_codes(result: Result<(), Vec<errors::error::HydraError>>) -> Vec<&'static str> {
    match result {
        Ok(()) => vec![],
        Err(errors) => {
            errors
                .iter()
                .map(|error| error.code)
                .collect()
        }
    }
}

// ============================================================================
// BASIC BORROWING
// ============================================================================

#[test]
fn shared_borrows_can_coexist() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let a = fixture.local(
        "a",
        Type::CONST_REF(
            Box::new(Type::I32),
        ),
    );

    let b = fixture.local(
        "b",
        Type::CONST_REF(
            Box::new(Type::I32),
        ),
    );

    let refs = fixture.local(
        "refs",
        Type::ARRAY(
            Box::new(
                Type::CONST_REF(
                    Box::new(Type::I32),
                ),
            ),
            2,
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            a,
            Rvalue::Ref(
                false,
                place(x),
            ),
        ),

        assign(
            2,
            b,
            Rvalue::Ref(
                false,
                place(x),
            ),
        ),

        use_refs(
            3,
            refs,
            &[a, b],
        ),
    ]);

    let result =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check();

    assert!(
        result.is_ok(),
        "multiple shared borrows should be allowed: {result:#?}",
    );
}


#[test]
fn two_live_mutable_borrows_conflict() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let a = fixture.local(
        "a",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let b = fixture.local(
        "b",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let refs = fixture.local(
        "refs",
        Type::ARRAY(
            Box::new(
                Type::REF(
                    Box::new(Type::I32),
                ),
            ),
            2,
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            a,
            Rvalue::Ref(
                true,
                place(x),
            ),
        ),

        assign(
            2,
            b,
            Rvalue::Ref(
                true,
                place(x),
            ),
        ),

        use_refs(
            3,
            refs,
            &[a, b],
        ),
    ]);

    let result =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check();

    let errors =
        result.expect_err(
            "two live mutable borrows must conflict",
        );

    assert!(
        errors.iter().any(|error| {
            error.code == "BC004"
                && error.message.contains(
                    "cannot borrow `x` as mutable more than once",
                )
        }),
        "expected BC004 mutable/mutable conflict, got {errors:#?}",
    );
}


#[test]
fn mutable_and_shared_borrow_conflict() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let mutable = fixture.local(
        "mutable",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let shared = fixture.local(
        "shared",
        Type::CONST_REF(
            Box::new(Type::I32),
        ),
    );

    let refs = fixture.local(
        "refs",
        Type::ARRAY(
            Box::new(
                Type::CONST_REF(
                    Box::new(Type::I32),
                ),
            ),
            2,
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            mutable,
            Rvalue::Ref(
                true,
                place(x),
            ),
        ),

        assign(
            2,
            shared,
            Rvalue::Ref(
                false,
                place(x),
            ),
        ),

        use_refs(
            3,
            refs,
            &[mutable, shared],
        ),
    ]);

    let result =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check();

    let errors =
        result.expect_err(
            "mutable and shared borrow must conflict",
        );

    assert!(
        errors
            .iter()
            .any(|error| error.code == "BC004"),
        "expected BC004, got {errors:#?}",
    );
}


// ============================================================================
// BORROW LIVENESS
// ============================================================================

#[test]
fn cannot_assign_to_value_while_shared_borrow_is_live() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let reference = fixture.local(
        "reference",
        Type::CONST_REF(
            Box::new(Type::I32),
        ),
    );

    let use_reference = fixture.local(
        "use_reference",
        Type::CONST_REF(
            Box::new(Type::I32),
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            reference,
            Rvalue::Ref(
                false,
                place(x),
            ),
        ),

        //
        // The reference is still needed by the next statement,
        // so this write happens while the borrow is live.
        //
        assign(
            2,
            x,
            Rvalue::Use(
                int(42),
            ),
        ),

        assign(
            3,
            use_reference,
            Rvalue::Use(
                copy(reference),
            ),
        ),
    ]);

    let errors =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check()
        .expect_err(
            "writing through a live shared borrow must fail",
        );

    assert!(
        errors.iter().any(|error| {
            error.code == "BC001"
                && error.message.contains(
                    "cannot assign to `x`",
                )
        }),
        "expected BC001, got {errors:#?}",
    );
}


#[test]
fn cannot_read_value_while_mutably_borrowed() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let reference = fixture.local(
        "reference",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let read_x = fixture.local(
        "read_x",
        Type::I32,
    );

    let use_reference = fixture.local(
        "use_reference",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            reference,
            Rvalue::Ref(
                true,
                place(x),
            ),
        ),

        assign(
            2,
            read_x,
            Rvalue::Use(
                copy(x),
            ),
        ),

        //
        // Keep the mutable borrow live across the read above.
        //
        assign(
            3,
            use_reference,
            Rvalue::Use(
                copy(reference),
            ),
        ),
    ]);

    let errors =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check()
        .expect_err(
            "reading a mutably borrowed value must fail",
        );

    assert!(
        errors.iter().any(|error| {
            error.code == "BC002"
                && error.message.contains(
                    "cannot use `x`",
                )
        }),
        "expected BC002, got {errors:#?}",
    );
}


#[test]
fn borrow_ends_after_last_use() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let reference = fixture.local(
        "reference",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let last_use = fixture.local(
        "last_use",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            reference,
            Rvalue::Ref(
                true,
                place(x),
            ),
        ),

        //
        // Last use of the borrow.
        //
        assign(
            2,
            last_use,
            Rvalue::Use(
                copy(reference),
            ),
        ),

        //
        // Neither reference nor last_use is live here.
        //
        assign(
            3,
            x,
            Rvalue::Use(
                int(42),
            ),
        ),
    ]);

    let result =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check();

    assert!(
        result.is_ok(),
        "borrow should end after its last live use: {result:#?}",
    );
}


// ============================================================================
// BORROW ALIASES
// ============================================================================

#[test]
fn copied_reference_preserves_borrow_information() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let original = fixture.local(
        "original",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let alias = fixture.local(
        "alias",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let shared = fixture.local(
        "shared",
        Type::CONST_REF(
            Box::new(Type::I32),
        ),
    );

    let refs = fixture.local(
        "refs",
        Type::ARRAY(
            Box::new(
                Type::CONST_REF(
                    Box::new(Type::I32),
                ),
            ),
            2,
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            original,
            Rvalue::Ref(
                true,
                place(x),
            ),
        ),

        //
        // alias now carries the same mutable borrow of x.
        //
        assign(
            2,
            alias,
            Rvalue::Use(
                copy(original),
            ),
        ),

        //
        // original itself no longer needs to remain live.
        //
        assign(
            3,
            shared,
            Rvalue::Ref(
                false,
                place(x),
            ),
        ),

        use_refs(
            4,
            refs,
            &[alias, shared],
        ),
    ]);

    let errors =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check()
        .expect_err(
            "copied mutable reference must retain its borrow",
        );

    assert!(
        errors
            .iter()
            .any(|error| error.code == "BC004"),
        "expected BC004 through copied reference, got {errors:#?}",
    );
}


// ============================================================================
// MOVES
// ============================================================================

#[test]
fn using_value_after_move_is_rejected() {
    let mut fixture = MirFixture::new();

    let value = fixture.local(
        "value",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let moved_into = fixture.local(
        "moved_into",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let used_again = fixture.local(
        "used_again",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            moved_into,
            Rvalue::Use(
                move_(value),
            ),
        ),

        assign(
            2,
            used_again,
            Rvalue::Use(
                move_(value),
            ),
        ),
    ]);

    let errors =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check()
        .expect_err(
            "using a moved value must fail",
        );

    assert!(
        errors.iter().any(|error| {
            error.code == "BC003"
                && error.message.contains(
                    "use of moved value `value`",
                )
        }),
        "expected BC003, got {errors:#?}",
    );
}


#[test]
fn reassigning_local_after_move_reinitializes_it() {
    let mut fixture = MirFixture::new();

    let value = fixture.local(
        "value",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let moved_into = fixture.local(
        "moved_into",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let used_again = fixture.local(
        "used_again",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            moved_into,
            Rvalue::Use(
                move_(value),
            ),
        ),

        //
        // Direct assignment reinitializes value.
        //
        assign(
            2,
            value,
            Rvalue::Aggregate(
                AggregateKind::Array(
                    Type::I32,
                ),
                vec![int(123)],
            ),
        ),

        assign(
            3,
            used_again,
            Rvalue::Use(
                move_(value),
            ),
        ),
    ]);

    let result =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check();

    assert!(
        result.is_ok(),
        "reinitialized local should be usable again: {result:#?}",
    );
}


#[test]
fn using_value_after_drop_is_rejected() {
    let mut fixture = MirFixture::new();

    let value = fixture.local(
        "value",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let used_again = fixture.local(
        "used_again",
        Type::ARRAY(
            Box::new(Type::I32),
            1,
        ),
    );

    let (context, mir) = fixture.finish(vec![
        Statement {
            kind: StatementKind::Drop(
                place(value),
            ),
            span: span(1),
        },

        assign(
            2,
            used_again,
            Rvalue::Use(
                move_(value),
            ),
        ),
    ]);

    let errors =
        BorrowChecker::new(
            &mir,
            &context,
        )
        .check()
        .expect_err(
            "using a dropped value must fail",
        );

    assert!(
        errors
            .iter()
            .any(|error| error.code == "BC003"),
        "expected BC003 after drop, got {errors:#?}",
    );
}


// ============================================================================
// MIR DATAFLOW / GEN-KILL
// ============================================================================

#[test]
fn direct_assignment_kills_previous_local_value() {
    let mut fixture = MirFixture::new();

    let x = fixture.local(
        "x",
        Type::I32,
    );

    let (context, mir) = fixture.finish(vec![
        assign(
            1,
            x,
            Rvalue::Use(
                int(42),
            ),
        ),
    ]);

    let checker =
        BorrowChecker::new(
            &mir,
            &context,
        );

    let effects =
        checker.compute_gen_kill();

    let bb0 =
        effects
            .get(&BasicBlockID(0))
            .expect("bb0 should have dataflow effects");

    assert!(
        bb0.kill.contains(&x),
        "direct assignment should kill the previous value of x",
    );

    assert!(
        !bb0.gen.contains(&x),
        "direct assignment should not read x first",
    );
}


#[test]
fn assignment_to_projection_reads_base_instead_of_killing_it() {
    let mut fixture = MirFixture::new();

    let object = fixture.local(
        "object",
        Type::I32,
    );

    let projection = Place {
        local: object,
        projection: vec![
            ProjectionElem::Field(0),
        ],
    };

    let (context, mir) = fixture.finish(vec![
        Statement {
            kind: StatementKind::Assign(
                projection,
                Rvalue::Use(
                    int(42),
                ),
            ),
            span: span(1),
        },
    ]);

    let checker =
        BorrowChecker::new(
            &mir,
            &context,
        );

    let effects =
        checker.compute_gen_kill();

    let bb0 =
        effects
            .get(&BasicBlockID(0))
            .expect("bb0 should have dataflow effects");

    assert!(
        bb0.gen.contains(&object),
        "projection assignment must read its base place",
    );

    assert!(
        !bb0.kill.contains(&object),
        "writing a projection must not kill the entire base local",
    );
}
