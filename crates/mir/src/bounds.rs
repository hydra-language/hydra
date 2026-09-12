use std::collections::{HashMap, VecDeque};

use errors::error::{HydraError, Span};
use ir::{
    Constant,
    context::{DefKind, HIRContext},
    hir::{HIRBinOp, HIRUnaryOp},
    types::Type,
};

use crate::{
    BasicBlockID,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct KnownInt {
    value: i64,
    span: Span,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Facts {
    ints: HashMap<LocalID, KnownInt>,
    lengths: HashMap<LocalID, usize>,
}

pub struct BoundsChecker<'a> {
    mir: &'a MIRFunction,
    context: &'a HIRContext,
}

impl<'a> BoundsChecker<'a> {

    pub fn new(mir: &'a MIRFunction, context: &'a HIRContext) -> Self {
        Self {
            mir,
            context,
        }
    }

    pub fn check(&self) -> Result<(), Vec<HydraError>> {
        if self.mir.basic_blocks.is_empty() {
            return Ok(());
        }

        let incoming = self.compute_facts();
        let mut errors = Vec::new();

        for (index, block) in self.mir.basic_blocks.iter().enumerate() {
            let Some(mut facts) = incoming[index].clone() else {
                continue;
            };

            for stmt in &block.statements {
                self.check_statement(stmt, &facts, &mut errors);
                self.apply_statement(stmt, &mut facts);
            }

            self.check_terminator(
                &block.terminator,
                &facts,
                &mut errors,
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn compute_facts(&self) -> Vec<Option<Facts>> {
        let mut incoming = vec![None; self.mir.basic_blocks.len()];
        let mut worklist = VecDeque::new();

        incoming[0] = Some(Facts::default());
        worklist.push_back(BasicBlockID(0));

        while let Some(block_id) = worklist.pop_front() {
            let Some(mut facts) = incoming[block_id.0].clone() else {
                continue;
            };

            let block = &self.mir.basic_blocks[block_id.0];

            for stmt in &block.statements {
                self.apply_statement(stmt, &mut facts);
            }

            self.apply_terminator(
                &block.terminator,
                &mut facts,
            );

            for successor in Self::successors(&block.terminator) {
                let changed = match &incoming[successor.0] {
                    Some(existing) => {
                        let merged = Self::merge_facts(
                            existing,
                            &facts,
                        );

                        if merged != *existing {
                            incoming[successor.0] = Some(merged);
                            true
                        } else {
                            false
                        }
                    }

                    None => {
                        incoming[successor.0] = Some(facts.clone());
                        true
                    }
                };

                if changed {
                    worklist.push_back(successor);
                }
            }
        }

        incoming
    }

    fn merge_facts(left: &Facts, right: &Facts) -> Facts {
        let mut ints = HashMap::new();
        let mut lengths = HashMap::new();

        for (local, known) in &left.ints {
            let Some(other) = right.ints.get(local) else {
                continue;
            };

            if known.value == other.value {
                ints.insert(*local, known.clone());
            }
        }

        for (local, len) in &left.lengths {
            let Some(other) = right.lengths.get(local) else {
                continue;
            };

            if len == other {
                lengths.insert(*local, *len);
            }
        }

        Facts {
            ints,
            lengths,
        }
    }

    fn successors(terminator: &Terminator) -> Vec<BasicBlockID> {
        match terminator {
            Terminator::Goto { target } => {
                vec![*target]
            }

            Terminator::SwitchInt { true_target, false_target, .. } => {
                vec![*true_target, *false_target]
            }

            Terminator::Call { target, .. } |
            Terminator::BuiltinCall { target, .. } => {
                vec![*target]
            }

            Terminator::Return | Terminator::Unreachable => {
                Vec::new()
            }
        }
    }

    fn check_statement(&self, stmt: &Statement, facts: &Facts, errors: &mut Vec<HydraError>) {
        match &stmt.kind {
            StatementKind::Assign(place, rval) => {
                self.check_place( place, facts, stmt.span, errors);

                self.check_rvalue( rval, facts, stmt.span, errors);
            }

            StatementKind::Drop(place) => {
                self.check_place( place, facts, stmt.span, errors);
            }
        }
    }

    fn check_rvalue(&self, rval: &Rvalue, facts: &Facts, span: Span, errors: &mut Vec<HydraError>) {
        match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                self.check_operand( op, facts, span, errors);
            }

            Rvalue::BinaryOp(_, lhs, rhs) => {
                self.check_operand( lhs, facts, span, errors);

                self.check_operand( rhs, facts, span, errors);
            }

            Rvalue::Aggregate(_, operands) => {
                for operand in operands {
                    self.check_operand( operand, facts, span, errors);
                }
            }

            Rvalue::Intrinsic { args, .. } => {
                for arg in args {
                    self.check_operand( arg, facts, span, errors);
                }
            }

            Rvalue::Ref(_, place) |
            Rvalue::SliceRef { place, .. } => {
                self.check_place( place, facts, span, errors);
            }
        }
    }

    fn check_operand(
        &self,
        operand: &Operand,
        facts: &Facts,
        span: Span,
        errors: &mut Vec<HydraError>,
    ) {
        match operand {
            Operand::Copy(place) |
            Operand::Move(place) => {
                self.check_place( place, facts, span, errors);
            }

            Operand::Const(_) => {}
        }
    }

    fn check_terminator(&self, terminator: &Terminator, facts: &Facts, errors: &mut Vec<HydraError>) {
        match terminator {
            Terminator::SwitchInt { discriminant, .. } => {
                self.check_operand( discriminant, facts, Span::default(), errors);
            }

            Terminator::Call { args, destination, .. } => {
                for arg in args {
                    self.check_operand( arg, facts, Span::default(), errors);
                }

                self.check_place( destination, facts, Span::default(), errors);
            }

            Terminator::BuiltinCall { args, .. } => {
                for arg in args {
                    self.check_operand( arg, facts, Span::default(), errors);
                }
            }

            Terminator::Goto { .. } |
            Terminator::Return |
            Terminator::Unreachable => {}
        }
    }

    fn check_place(&self, place: &Place, facts: &Facts, fallback_span: Span, errors: &mut Vec<HydraError>) {
        let Some(local) = self.mir.locals.get(place.local.0) else {
            return;
        };

        let mut ty = local.ty.clone();
        let mut slice_len = facts.lengths.get(&place.local).copied();

        for projection in &place.projection {
            match projection {
                ProjectionElem::Deref => {
                    match ty {
                        Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) | Type::CONST_POINTER(inner) => {
                            let is_slice = matches!(inner.as_ref(), Type::SLICE(_));

                            ty = *inner;

                            if !is_slice {
                                slice_len = None;
                            }
                        }

                        _ => return,
                    }
                }

                ProjectionElem::Field(index) => {
                    let Type::STRUCT(type_ref) = ty else {
                        return;
                    };

                    let Some(info) = self.context.get_def(type_ref.def_id) else {
                        return;
                    };

                    let DefKind::Struct { fields, .. } = &info.kind else {
                        return;
                    };

                    let Some((_, field_ty, _)) = fields.get(*index) else {
                        return;
                    };

                    ty = field_ty.clone();
                    slice_len = None;
                }

                ProjectionElem::Index(index_local) => {
                    let len = match &ty {
                        Type::ARRAY(_, len) => {
                            Some(*len)
                        }

                        Type::SLICE(_) => {
                            slice_len
                        }

                        _ => None,
                    };

                    if let (Some(len), Some(index)) = (len, facts.ints.get(index_local)) {
                        if index.value < 0 || index.value as usize >= len {
                            let span = if index.span == Span::default() {
                                fallback_span
                            } else {
                                index.span
                            };

                            errors.push(HydraError::new(
                                "S008",
                                format!(
                                    "index out of bounds: len is {} but index is {}",
                                    len,
                                    index.value,
                                ),
                                span,
                            ));
                        }
                    }

                    ty = match ty {
                        Type::ARRAY(inner, _) | Type::INFERRED_ARRAY(inner) | Type::SLICE(inner) => {
                            *inner
                        }

                        _ => return,
                    };

                    slice_len = None;
                }
            }
        }
    }

    fn apply_statement(&self, stmt: &Statement, facts: &mut Facts) {
        let StatementKind::Assign(place, rval) = &stmt.kind else {
            return;
        };

        if !place.projection.is_empty() {
            return;
        }

        let int_value = self.eval_int_rvalue(rval, facts);

        let length = self.eval_length_rvalue(rval, facts);

        facts.ints.remove(&place.local);
        facts.lengths.remove(&place.local);

        if let Some(value) = int_value {
            facts.ints.insert(place.local, KnownInt { value, span: stmt.span, });
        }

        if let Some(len) = length {
            facts.lengths.insert(place.local, len);
        }
    }

    fn apply_terminator(&self, terminator: &Terminator, facts: &mut Facts) {
        if let Terminator::Call { destination, .. } = terminator {
            if destination.projection.is_empty() {
                facts.ints.remove(&destination.local);
                facts.lengths.remove(&destination.local);
            }

            //
            // until alias analysis is strong enough to prove which locals
            // a call may mutate, do not carry local value/length facts
            // across arbitrary function calls.
            //
            facts.ints.clear();
            facts.lengths.clear();
        }
    }

    fn eval_int_rvalue(&self, rval: &Rvalue, facts: &Facts) -> Option<i64> {
        match rval {
            Rvalue::Use(operand) => {
                self.eval_int_operand(operand, facts)
            }

            Rvalue::BinaryOp(op, lhs, rhs) => {
                let lhs = self.eval_int_operand(lhs, facts)?;

                let rhs = self.eval_int_operand(rhs, facts)?;

                match op {
                    HIRBinOp::Add => lhs.checked_add(rhs),
                    HIRBinOp::Sub => lhs.checked_sub(rhs),
                    HIRBinOp::Mul => lhs.checked_mul(rhs),

                    HIRBinOp::Div if rhs != 0 => {
                        lhs.checked_div(rhs)
                    }

                    HIRBinOp::Mod if rhs != 0 => {
                        lhs.checked_rem(rhs)
                    }

                    _ => None,
                }
            }

            Rvalue::UnaryOp(HIRUnaryOp::Neg, operand) => {
                self.eval_int_operand(operand, facts)?.checked_neg()
            }

            _ => None,
        }
    }

    fn eval_int_operand(&self, operand: &Operand, facts: &Facts) -> Option<i64> {
        match operand {
            Operand::Const(Constant::Int(value, _)) => {
                Some(*value)
            }

            Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
                facts.ints.get(&place.local).map(|known| known.value)
            }

            _ => None,
        }
    }

    fn eval_length_rvalue(&self, rval: &Rvalue, facts: &Facts) -> Option<usize> {
        match rval {
            Rvalue::SliceRef { len, .. } => {
                Some(*len)
            }

            Rvalue::Use(Operand::Const( Constant::String(value))) => {
                Some(value.chars().count())
            }

            Rvalue::Use( Operand::Copy(place) | Operand::Move(place)) if place.projection.is_empty() => 
            {
                facts.lengths.get(&place.local).copied()
            }

            _ => None,
        }
    }
}
