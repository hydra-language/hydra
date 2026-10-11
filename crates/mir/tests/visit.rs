use mir::{LocalID, Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind};
use mir::visit::{self, PlaceContext, Visitor};

#[derive(Default)]
struct Recorder(Vec<(PlaceContext, LocalID)>);

impl Visitor for Recorder {
    fn visit_place(&mut self, place: &Place, context: PlaceContext) {
        self.0.push((context, place.local));
        visit::walk_place(self, place);
    }
}

fn place(local: usize) -> Place {
    Place { local: LocalID(local), projection: vec![] }
}

#[test]
fn visit_statement_preserves_access_order_and_projected_address_reads() {
    let destination = Place {
        local: LocalID(1),
        projection: vec![ProjectionElem::Deref, ProjectionElem::Index(LocalID(2))],
    };
    let statement = Statement {
        kind: StatementKind::Assign(destination, Rvalue::Use(Operand::Move(place(3)))),
        span: Default::default(),
    };

    let mut visitor = Recorder::default();
    visitor.visit_statement(&statement);
    assert_eq!(visitor.0, vec![
        (PlaceContext::Move, LocalID(3)), (PlaceContext::Write, LocalID(1)),
        (PlaceContext::Read, LocalID(1)), (PlaceContext::Read, LocalID(2)),
    ]);
}

#[test]
fn visit_rvalue_distinguishes_shared_and_mutable_borrow() {
    let mut visitor = Recorder::default();
    visitor.visit_rvalue(&Rvalue::Ref(false, place(1)));
    visitor.visit_rvalue(&Rvalue::Ref(true, place(2)));

    assert_eq!(visitor.0, vec![
        (PlaceContext::SharedBorrow, LocalID(1)),
        (PlaceContext::MutBorrow, LocalID(2)),
    ]);
}
