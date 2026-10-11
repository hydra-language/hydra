use errors::error::Span;
use ir::{Constant, context::DefID, types::Type};
use mir::{
    AggregateKind, BasicBlock, LocalDecl, LocalID, MIRFunction, Operand, Place,
    ProjectionElem, Rvalue, Statement, StatementKind, Terminator,
    bounds::BoundsChecker,
};

fn span(line: usize) -> Span {
    Span { line, column: 1, length: 1 }
}

fn place(id: usize) -> Place {
    Place { local: LocalID(id), projection: vec![] }
}

fn local(ty: Type) -> LocalDecl {
    LocalDecl {
        ty,
        is_mutable: false,
        debug_def_id: None,
        debug_name: None,
        debug_span: None,
    }
}

fn assign(destination: Place, value: Rvalue, line: usize) -> Statement {
    Statement {
        kind: StatementKind::Assign(destination, value),
        span: span(line),
    }
}

fn int(value: i64) -> Operand {
    Operand::Const(Constant::Int(value, Type::I32))
}

fn function(name: &str, return_type: Type, arg_count: usize, locals: Vec<LocalDecl>, statements: Vec<Statement>) -> MIRFunction {
    MIRFunction {
        name: name.into(),
        def_id: DefID(0),
        display_name: None,
        return_type,
        arg_count,
        locals,
        basic_blocks: vec![BasicBlock { statements, terminator: Terminator::Return }],
        is_inline: false,
    }
}

fn known_slice_access(index: i64) -> MIRFunction {
    let slice_ty = Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::I32))));

    function(
        "main",
        Type::VOID,
        0,
        vec![
            local(Type::VOID),
            local(slice_ty.clone()),
            local(Type::ARRAY(Box::new(Type::I32), 3)),
            local(slice_ty),
            local(Type::USIZE),
            local(Type::I32),
        ],
        vec![
            assign(
                place(2),
                Rvalue::Aggregate(AggregateKind::Array(Type::I32), vec![
                    int(10), int(20), int(30),
                ]),
                2,
            ),
            assign(
                place(3),
                Rvalue::SliceRef {
                    is_mut: false,
                    place: place(2),
                    len: 3,
                    element_ty: Type::I32,
                },
                2,
            ),
            assign(place(1), Rvalue::Use(Operand::Copy(place(3))), 2),
            assign(
                place(4),
                Rvalue::Use(Operand::Const(Constant::Int(index, Type::USIZE))),
                3,
            ),
            assign(
                place(5),
                Rvalue::Use(Operand::Copy(Place {
                    local: LocalID(1),
                    projection: vec![
                        ProjectionElem::Deref,
                        ProjectionElem::Index(LocalID(4)),
                    ],
                })),
                3,
            ),
        ],
    )
}

#[test]
fn rejects_known_out_of_bounds_slice_index() {
    let mir = known_slice_access(3);
    let errors = BoundsChecker::new(&mir).check()
        .expect_err("index equal to slice length must be rejected");

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "S008");
    assert_eq!(errors[0].message, "index out of bounds: len is 3 but index is 3");
}

#[test]
fn accepts_known_in_bounds_slice_index() {
    let mir = known_slice_access(2);
    BoundsChecker::new(&mir).check()
        .expect("index below slice length must be accepted");
}

#[test]
fn leaves_unknown_slice_index_for_runtime_checking() {
    let slice_ty = Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::I32))));
    let mir = function(
        "get",
        Type::I32,
        2,
        vec![local(Type::I32), local(slice_ty), local(Type::USIZE)],
        vec![assign(
            place(0),
            Rvalue::Use(Operand::Copy(Place {
                local: LocalID(1),
                projection: vec![
                    ProjectionElem::Deref,
                    ProjectionElem::Index(LocalID(2)),
                ],
            })),
            2,
        )],
    );

    BoundsChecker::new(&mir).check()
        .expect("unknown runtime bounds must not be rejected statically");
}
