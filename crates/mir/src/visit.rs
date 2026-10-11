use crate::{
    LocalID, MIRFunction, Operand, Place, ProjectionElem, Rvalue,
    Statement, StatementKind, Terminator
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceContext {
    Read,
    Move,
    Write,
    SharedBorrow,
    MutBorrow,
    Drop,
}

pub trait Visitor {

    fn visit_function(&mut self, function: &MIRFunction) {
        walk_function(self, function);
    }

    fn visit_statement(&mut self, statement: &Statement) {
        walk_statement(self, statement);
    }

    fn visit_terminator(&mut self, terminator: &Terminator) {
        walk_terminator(self, terminator);
    }

    fn visit_rvalue(&mut self, rvalue: &Rvalue) {
        walk_rvalue(self, rvalue);
    }

    fn visit_operand(&mut self, operand: &Operand) {
        walk_operand(self, operand);
    }

    fn visit_place(&mut self, place: &Place, _context: PlaceContext) {
        walk_place(self, place);
    }
}

pub fn walk_function<V: Visitor + ?Sized>(visitor: &mut V, function: &MIRFunction) {
    for block in &function.basic_blocks {
        for statement in &block.statements {
            visitor.visit_statement(statement);
        }

        visitor.visit_terminator(&block.terminator);
    }
}

pub fn walk_statement<V: Visitor + ?Sized>(visitor: &mut V, statement: &Statement) {
    match &statement.kind {
        StatementKind::Assign(destination, value) => {
            visitor.visit_rvalue(value);
            visitor.visit_place(destination, PlaceContext::Write);
        }

        StatementKind::Drop { place, .. } => {
            visitor.visit_place(place, PlaceContext::Drop);
        }
    }
}

pub fn walk_terminator<V: Visitor + ?Sized>(visitor: &mut V, terminator: &Terminator) {
    match terminator {
        Terminator::SwitchInt { discriminant, .. } => visitor.visit_operand(discriminant),

        Terminator::Call { args, destination, .. } => {
            for argument in args {
                visitor.visit_operand(argument);
            }

            visitor.visit_place(destination, PlaceContext::Write);
        }

        Terminator::Return => visitor.visit_place(
            &Place { local: LocalID(0), projection: Vec::new() },
            PlaceContext::Read,
        ),

        Terminator::Goto { .. } | Terminator::Unreachable => {}
    }
}

pub fn walk_rvalue<V: Visitor + ?Sized>(visitor: &mut V, rvalue: &Rvalue) {
    match rvalue {
        Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
            visitor.visit_operand(op);
        }

        Rvalue::BinaryOp(_, lhs, rhs) => {
            visitor.visit_operand(lhs);
            visitor.visit_operand(rhs);
        }

        Rvalue::Ref(is_mut, place) => {
            let context = if *is_mut { PlaceContext::MutBorrow } else { PlaceContext::SharedBorrow };
            visitor.visit_place(place, context);
        }

        Rvalue::SliceRef { is_mut, place, .. } => {
            let context = if *is_mut { PlaceContext::MutBorrow } else { PlaceContext::SharedBorrow };
            visitor.visit_place(place, context);
        }

        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                visitor.visit_operand(operand);
            }
        }

        Rvalue::Intrinsic { args, .. } => {
            for operand in args {
                visitor.visit_operand(operand);
            }
        }
    }
}

pub fn walk_operand<V: Visitor + ?Sized>(visitor: &mut V, operand: &Operand) {
    match operand {
        Operand::Copy(place) => visitor.visit_place(place, PlaceContext::Read),
        Operand::Move(place) => visitor.visit_place(place, PlaceContext::Move),
        Operand::Const(_) => {}
    }
}

pub fn walk_place<V: Visitor + ?Sized>(visitor: &mut V, place: &Place) {
    for (index, projection) in place.projection.iter().enumerate() {
        match projection {
            ProjectionElem::Deref => {
                visitor.visit_place(
                    &Place {
                        local: place.local,
                        projection: place.projection[..index].to_vec(),
                    },
                    PlaceContext::Read,
                );
            }

            ProjectionElem::Index(local) => {
                visitor.visit_place(
                    &Place { local: *local, projection: Vec::new() },
                    PlaceContext::Read,
                );
            }

            ProjectionElem::Field { .. } => {}
        }
    }
}
