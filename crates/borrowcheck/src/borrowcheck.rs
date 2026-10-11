use std::collections::{HashMap, HashSet, VecDeque};

use errors::error::{self, HydraError, Span};

use mir::{
    BasicBlockID, LocalID, MIRFunction, Operand, Place, Rvalue, Statement, StatementKind, Terminator
};

use mir::visit::{self, PlaceContext, Visitor};

use crate::init::InitChecker;
use crate::loans::LoanChecker;

#[derive(Debug)]
pub struct BlockEffects {
    pub gen: HashSet<LocalID>,  // variables read before being overwritten
    pub kill: HashSet<LocalID>, // variables overwritten
}

#[derive(Clone, Debug)]
struct Borrow {
    borrower: LocalID,
    target: LocalID,
    is_mut: bool,
    creation_span: error::Span,
}

struct SequentialMoveChecker<'check, 'mir, 'state> {
    checker: &'check BorrowChecker<'mir>,
    moved: &'state mut HashSet<LocalID>,
    errors: &'state mut Vec<HydraError>,
    span: error::Span,
}

impl<'check, 'mir, 'state> Visitor for SequentialMoveChecker<'check, 'mir, 'state> {

    fn visit_place(&mut self, place: &Place, context: PlaceContext) {
        if matches!(context, PlaceContext::Read | PlaceContext::Move | PlaceContext::SharedBorrow | PlaceContext::MutBorrow) && self.moved.contains(&place.local) 
        {
            let (name, _) = self.checker.resolve_local(place.local);

            self.errors.push(self.checker.error(
                "BC003", format!("use of moved value `{}`", name), Some(self.span),
            ));
        }

        if context == PlaceContext::Move {
            self.moved.insert(place.local);
        }

        visit::walk_place(self, place);
    }
}

#[derive(Default)]
struct AccessCollector(Vec<(Place, PlaceContext)>);

impl Visitor for AccessCollector {
    fn visit_place(&mut self, place: &Place, context: PlaceContext) {
        self.0.push((place.clone(), context));
        visit::walk_place(self, place);
    }
}

pub struct BorrowChecker<'a> {
    mir: &'a MIRFunction,
}

impl<'a> BorrowChecker<'a> {

    pub fn new(mir: &'a MIRFunction) -> Self {
        Self { 
            mir
        }
    }

    fn error(&self, code: &'static str, message: impl Into<String>, span: Option<error::Span>) -> HydraError {
        HydraError::new(code, message, span.unwrap_or_default())
    }

    pub fn check(&mut self) -> Result<(), Vec<HydraError>> {
        let mut errors = Vec::new();

        LoanChecker::new(self.mir).check(&mut errors);
        InitChecker::new(self.mir).check(&mut errors);

        let mut seen = HashSet::new();

        errors.retain(|error| {
            let notes: Vec<_> = error.notes.iter().map(|note| (
                note.message.clone(),
                note.span.line,
                note.span.column,
                note.span.length,
            )).collect();

            seen.insert((
                error.code,
                error.message.clone(),
                error.span.line,
                error.span.column,
                error.span.length,
                notes,
            ))
        });

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn compute_liveness(&mut self) -> HashMap<BasicBlockID, HashSet<LocalID>> {
        let effects = self.compute_gen_kill();
        
        let mut live_in: HashMap<BasicBlockID, HashSet<LocalID>> = HashMap::new();
        let mut live_out: HashMap<BasicBlockID, HashSet<LocalID>> = HashMap::new();

        // Initialize empty sets for all blocks
        for i in 0..self.mir.basic_blocks.len() {
            live_in.insert(BasicBlockID(i), HashSet::new());
            live_out.insert(BasicBlockID(i), HashSet::new());
        }

        let mut changed = true;
        
        // Loop until the dataflow equations stabilize
        while changed {
            changed = false;

            // Iterate backwards through the blocks
            for i in (0..self.mir.basic_blocks.len()).rev() {
                let bb = BasicBlockID(i);
                let block = &self.mir.basic_blocks[i];

                // 1. Calculate Live-Out: Union of Live-In of all successor blocks
                let mut new_live_out = HashSet::new();
                
                let successors = match &block.terminator {
                    Terminator::Goto { target } => vec![*target],
                    Terminator::SwitchInt { true_target, false_target, .. } => vec![*true_target, *false_target],
                    Terminator::Call { target, .. } => vec![*target],
                    Terminator::Return | Terminator::Unreachable => vec![],
                };

                for succ in successors {
                    if let Some(succ_in) = live_in.get(&succ) {
                        new_live_out.extend(succ_in.iter().cloned());
                    }
                }

                // 2. Calculate Live-In: Gen U (Live-Out - Kill)
                let block_effects = effects.get(&bb).unwrap();
                let mut new_live_in = block_effects.gen.clone();
                
                for out_var in &new_live_out {
                    if !block_effects.kill.contains(out_var) {
                        new_live_in.insert(*out_var);
                    }
                }

                // 3. Check if anything changed
                if new_live_in != *live_in.get(&bb).unwrap() || new_live_out != *live_out.get(&bb).unwrap() {
                    changed = true;
                    live_in.insert(bb, new_live_in);
                    live_out.insert(bb, new_live_out);
                }
            }
        }

        // Return the Live-Out sets, as these dictate what is alive at the END of a block
        live_out
    }

    fn enforce_moves(&self, errors: &mut Vec<HydraError>) {
        let moved_in = self.compute_moved_in();

        for (index, block) in self.mir.basic_blocks.iter().enumerate() {
            let Some(mut moved) = moved_in[index].clone() else {
                continue;
            };

            for stmt in &block.statements {
                if let StatementKind::Assign(_, rval) = &stmt.kind {
                    self.check_rvalue_moves(rval, &mut moved, errors, stmt.span);
                }

                self.apply_statement_move_state(stmt, &mut moved);
            }

            self.check_terminator_moves(&block.terminator, &mut moved, errors);
        }
    }

    fn compute_moved_in(&self) -> Vec<Option<HashSet<LocalID>>> {
        let mut incoming = vec![None; self.mir.basic_blocks.len()];

        if self.mir.basic_blocks.is_empty() {
            return incoming;
        }

        let mut worklist = VecDeque::new();

        incoming[0] = Some(HashSet::new());
        worklist.push_back(BasicBlockID(0));

        while let Some(block_id) = worklist.pop_front() {
            let Some(mut moved) = incoming[block_id.0].clone() else {
                continue;
            };

            let block = &self.mir.basic_blocks[block_id.0];

            for stmt in &block.statements {
                self.apply_statement_move_state(stmt, &mut moved);
            }

            self.apply_terminator_move_state(&block.terminator, &mut moved);

            for successor in Self::successors(&block.terminator) {
                let changed = match &incoming[successor.0] {
                    Some(existing) => {
                        let mut merged = existing.clone();
                        merged.extend(moved.iter().copied());

                        if merged != *existing {
                            incoming[successor.0] = Some(merged);
                            true
                        } else {
                            false
                        }
                    }

                    None => {
                        incoming[successor.0] = Some(moved.clone());
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

    fn apply_statement_move_state(&self, stmt: &Statement, moved: &mut HashSet<LocalID>) {
        match &stmt.kind {
            StatementKind::Assign(place, rval) => {
                // RHS is evaluated before the destination is written.
                self.record_rvalue_moves(rval, moved);

                // Assigning directly to a local reinitializes it.
                if place.projection.is_empty() {
                    moved.remove(&place.local);
                }
            }

            StatementKind::Drop { place, .. } => {
                moved.insert(place.local);
            }
        }
    }

    fn apply_terminator_move_state(&self, terminator: &Terminator, moved: &mut HashSet<LocalID>) {
        match terminator {
            Terminator::SwitchInt { discriminant, .. } => {
                Self::record_operand_move(discriminant, moved);
            }

            Terminator::Call { args, destination, .. } => {
                for arg in args {
                    Self::record_operand_move(arg, moved);
                }

                // On the success edge, the call has initialized its destination.
                if destination.projection.is_empty() {
                    moved.remove(&destination.local);
                }
            }

            Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
        }
    }

    fn check_terminator_moves(&self, terminator: &Terminator, moved: &mut HashSet<LocalID>, errors: &mut Vec<HydraError>) {
        match terminator {
            Terminator::SwitchInt { discriminant, .. } => {
                self.check_terminator_operand_move(discriminant, moved, errors);
                Self::record_operand_move(discriminant, moved);
            }

            Terminator::Call { args, destination, .. } => {
                for arg in args {
                    self.check_terminator_operand_move(arg, moved, errors);
                    Self::record_operand_move(arg, moved);
                }

                if destination.projection.is_empty() {
                    moved.remove(&destination.local);
                }
            }

            Terminator::Return => {
                if moved.contains(&LocalID(0)) {
                    errors.push(self.error(
                        "BC003",
                        "use of moved return value",
                        None,
                    ));
                }
            }

            Terminator::Goto { .. } | Terminator::Unreachable => {}
        }
    }

    fn check_terminator_operand_move(&self, operand: &Operand, moved: &HashSet<LocalID>, errors: &mut Vec<HydraError>) {
        let (Operand::Copy(place) | Operand::Move(place)) = operand else {
            return;
        };

        if moved.contains(&place.local) {
            let (name, span) = self.resolve_local(place.local);

            errors.push(self.error(
                "BC003",
                format!("use of moved value `{}`", name),
                span,
            ));
        }
    }

    fn record_operand_move(operand: &Operand, moved: &mut HashSet<LocalID>) {
        if let Operand::Move(place) = operand {
            moved.insert(place.local);
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

            Terminator::Call { target, .. } => vec![*target],

            Terminator::Return | Terminator::Unreachable => {
                vec![]
            }
        }
    }

    /// Step 1 of Dataflow Analysis: What does each block read and write?
    pub fn compute_gen_kill(&self) -> HashMap<BasicBlockID, BlockEffects> {
        let mut effects = HashMap::new();

        for (i, block) in self.mir.basic_blocks.iter().enumerate() {
            let bb = BasicBlockID(i);
            let mut gen = HashSet::new();
            let mut kill = HashSet::new();

            // Walk statements top to bottom
            for stmt in &block.statements {
                match &stmt.kind {
                    StatementKind::Assign(place, rvalue) => {
                        // 1. The right side is evaluated first (Reads/Gen)
                        self.extract_uses_rvalue(rvalue, &mut gen, &kill);
                        
                        // 2. The left side is assigned to (Writes/Kill)
                        if place.projection.is_empty() {
                            // Direct assignment like `_1 = ...` kills the old value of `_1`
                            kill.insert(place.local);
                        } else {
                            // Assignment to a projection like `_1.0 = ...` actually REQUIRES 
                            // `_1` to be alive so we can access its memory!
                            if !kill.contains(&place.local) {
                                gen.insert(place.local);
                            }
                        }
                    }
                    StatementKind::Drop { place, .. } => {
                        // Dropping reads the value one last time to clean it up
                        if !kill.contains(&place.local) {
                            gen.insert(place.local);
                        }
                    }
                }
            }

            // The terminator runs last in the block
            match &block.terminator {
                Terminator::SwitchInt { discriminant, .. } => {
                    self.extract_uses_operand(discriminant, &mut gen, &kill);
                }
                Terminator::Call { args, destination, .. } => {
                    for arg in args {
                        self.extract_uses_operand(arg, &mut gen, &kill);
                    }
                    // The return value overwrites the destination
                    if destination.projection.is_empty() {
                        kill.insert(destination.local);
                    } else {
                        if !kill.contains(&destination.local) {
                            gen.insert(destination.local);
                        }
                    }
                }
                Terminator::Return => {
                    // Returning implicitly reads the return value `_0`
                    if !kill.contains(&LocalID(0)) { gen.insert(LocalID(0)); }
                }
                _ => {} // Goto, Unreachable read nothing
            }

            effects.insert(bb, BlockEffects { gen, kill });
        }

        effects
    }

    // --- Helpers to dig into MIR structures ---
    fn extract_uses_rvalue(&self, rvalue: &Rvalue, gen: &mut HashSet<LocalID>, kill: &HashSet<LocalID>) {
        match rvalue {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                self.extract_uses_operand(op, gen, kill);
            }

            Rvalue::BinaryOp(_, lhs, rhs) => {
                self.extract_uses_operand(lhs, gen, kill);
                self.extract_uses_operand(rhs, gen, kill);
            }

            Rvalue::Ref(_, place) | Rvalue::SliceRef { place, .. } => {
                // taking a reference to a place requires that place to be alive
                if !kill.contains(&place.local) {
                    gen.insert(place.local);
                }
            }

            Rvalue::Aggregate(_, operands) => {
                for op in operands {
                    self.extract_uses_operand(op, gen, kill);
                }
            }

            Rvalue::Intrinsic { args, .. } => {
                for op in args {
                    self.extract_uses_operand(op, gen, kill);
                }
            }
        }
    }

    fn extract_uses_operand(&self, op: &Operand, gen: &mut HashSet<LocalID>, kill: &HashSet<LocalID>) {
        match op {
            Operand::Copy(place) | Operand::Move(place) => {
                // We only count it as a "Gen" if it hasn't ALREADY been overwritten (Killed) 
                // in this specific block.
                if !kill.contains(&place.local) {
                    gen.insert(place.local);
                }
            }
            Operand::Const(_) => {} // Constants aren't locals, they don't have liveness
        }
    }

    fn get_stmt_reads_writes(&self, stmt: &Statement) -> (HashSet<LocalID>, HashSet<LocalID>) {
        let mut reads = HashSet::new();
        let mut writes = HashSet::new();

        match &stmt.kind {
            StatementKind::Assign(place, rval) => {
                // Pass BOTH reads and writes down to evaluate the rvalue
                self.extract_rvalue_effects(rval, &mut reads, &mut writes);

                if place.projection.is_empty() {
                    writes.insert(place.local); 
                } else {
                    reads.insert(place.local);  
                }
            }
            StatementKind::Drop { place, .. } => {
                reads.insert(place.local);
            }
        }
        (reads, writes)
    }

    fn extract_rvalue_effects(&self, rvalue: &Rvalue, reads: &mut HashSet<LocalID>, writes: &mut HashSet<LocalID>) {
        match rvalue {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                if let Operand::Copy(p) | Operand::Move(p) = op { reads.insert(p.local); }
            }
            Rvalue::BinaryOp(_, lhs, rhs) => {
                if let Operand::Copy(p) | Operand::Move(p) = lhs { reads.insert(p.local); }
                if let Operand::Copy(p) | Operand::Move(p) = rhs { reads.insert(p.local); }
            }
            Rvalue::Ref(is_mut, place) | Rvalue::SliceRef { is_mut, place, .. } => {
                reads.insert(place.local); // taking any ref requires reading the memory address
                if *is_mut {
                    // ENFORCING XOR RULE: creating a &mut reference requires EXCLUSIVE access.
                    // by registering this as a "write", it will instantly conflict with any 
                    // existing immutable or mutable borrows!
                    writes.insert(place.local);
                }
            }
            Rvalue::Aggregate(_, operands) => {
                for op in operands {
                    if let Operand::Copy(p) | Operand::Move(p) = op { reads.insert(p.local); }
                }
            }

            Rvalue::Intrinsic { args, .. } => {
                for op in args {
                    if let Operand::Copy(p) | Operand::Move(p) = op {
                        reads.insert(p.local);
                    }
                }
            }
        }
    }

    fn get_terminator_reads_writes(&self, term: &Terminator) -> (HashSet<LocalID>, HashSet<LocalID>) {
        let mut reads = HashSet::new();
        let mut writes = HashSet::new();

        match term {
            Terminator::SwitchInt { discriminant, .. } => {
                if let Operand::Copy(p) | Operand::Move(p) = discriminant { reads.insert(p.local); }
            }
            Terminator::Call { args, destination, .. } => {
                for arg in args {
                    if let Operand::Copy(p) | Operand::Move(p) = arg { reads.insert(p.local); }
                }
                if destination.projection.is_empty() {
                    writes.insert(destination.local);
                } else {
                    reads.insert(destination.local);
                }
            }
            Terminator::Return => { reads.insert(LocalID(0)); }
            _ => {}
        }

        (reads, writes)
    }

    fn resolve_local(&self, local: LocalID) -> (String, Option<error::Span>) {
        let decl = &self.mir.locals[local.0];

        if let Some(name) = &decl.debug_name {
            return (name.clone(), decl.debug_span);
        }

        (format!("_{}", local.0), decl.debug_span)
    }

    fn check_rvalue_moves(&self, rval: &Rvalue, moved: &mut HashSet<LocalID>, errors: &mut Vec<HydraError>, span: Span) {
        let mut visitor = SequentialMoveChecker { 
            checker: self, 
            moved, 
            errors, 
            span 
        };

        visitor.visit_rvalue(rval);
    }

    fn record_rvalue_moves(&self, rval: &Rvalue, moved: &mut HashSet<LocalID>) {
        let operands: Vec<&Operand> = match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => vec![op],
            Rvalue::BinaryOp(_, l, r) => vec![l, r],
            Rvalue::Ref(_, _) | Rvalue::SliceRef { .. } => vec![], // borrows don't move
            Rvalue::Aggregate(_, ops) => ops.iter().collect(),
            Rvalue::Intrinsic { args, .. } => {
                args.iter().collect()
            }
        };

        for op in operands {
            if let Operand::Move(place) = op {
                moved.insert(place.local);
            }
        }
    }
}
