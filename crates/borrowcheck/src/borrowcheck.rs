use std::collections::{HashSet, HashMap};

use errors::error::{self, HydraError};
use ir::context::HIRContext;
use mir::{BasicBlockID, LocalID, MIRFunction, Operand, Rvalue, Statement, StatementKind, Terminator};

pub struct BorrowChecker<'a> {
    mir: &'a MIRFunction,
    context: &'a HIRContext,
}

#[derive(Debug)]
pub struct BlockEffects {
    pub gen: HashSet<LocalID>,  // variables read before being overwritten
    pub kill: HashSet<LocalID>, // variables overwritten
}

impl<'a> BorrowChecker<'a> {

    pub fn new(mir: &'a MIRFunction, context: &'a HIRContext) -> Self {
        Self { 
            mir,
            context
        }
    }

    fn error(&self, code: &'static str, message: impl Into<String>, span: Option<error::Span>) -> HydraError {
        HydraError::new(code, message, span.unwrap_or_default())
    }

    pub fn check(&mut self) -> Result<(), Vec<HydraError>> {
        let mut errors = Vec::new();
        let live_out = self.compute_liveness();

        self.enforce_borrows(&live_out, &mut errors);
        self.enforce_moves(&mut errors);
        self.enforce_borrow_conflicts(&live_out, &mut errors);

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
                    Terminator::Call { target, .. } | Terminator::BuiltinCall { target, .. } => vec![*target],
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

    fn enforce_borrows(&self, live_out: &HashMap<BasicBlockID, HashSet<LocalID>>, errors: &mut Vec<HydraError>) {
        #[derive(Clone, Debug)]
        struct Borrow {
            borrower: LocalID,
            target: LocalID,
            is_mut: bool,
            creation_span: error::Span,
        }

        let mut all_borrows = Vec::new();
        for block in &self.mir.basic_blocks {

            for stmt in &block.statements {
                if let StatementKind::Assign(place, Rvalue::Ref(is_mut, target)) = &stmt.kind {
                    all_borrows.push(Borrow {
                        borrower: place.local,
                        target: target.local,
                        is_mut: *is_mut,
                        creation_span: stmt.span,
                    });
                }
            }        
        }

        let mut changed = true;
        while changed {
            changed = false;
            for block in &self.mir.basic_blocks {
                for stmt in &block.statements {
                    if let StatementKind::Assign(dest, Rvalue::Use(Operand::Copy(src) | Operand::Move(src))) = &stmt.kind {
                        // Check if src is a known borrower
                        let inherited: Option<Borrow> = all_borrows.iter().find(|b| b.borrower == src.local).cloned();
                        if let Some(parent) = inherited {
                            // Check if dest is already registered as a borrower of the same target
                            let already_tracked = all_borrows.iter().any(|b| b.borrower == dest.local && b.target == parent.target);
                            if !already_tracked {
                                all_borrows.push(Borrow {
                                    borrower: dest.local,
                                    target: parent.target,
                                    is_mut: parent.is_mut,
                                    creation_span: stmt.span,
                                });
                                changed = true;
                            }
                        }
                    }
                }
            }
        }

        // 2. Check every block for violations
        for (i, block) in self.mir.basic_blocks.iter().enumerate() {
            let bb = BasicBlockID(i);
            
            // To do accurate intra-block liveness, we walk BACKWARDS from the block's live_out
            // to compute the live set *before* each statement.
            let mut live_at_stmt = Vec::new();
            let mut current_live = live_out.get(&bb).unwrap().clone();

            // Simulate the terminator first (since we are walking backward)
            let (term_reads, term_writes) = self.get_terminator_reads_writes(&block.terminator);
            for w in &term_writes { current_live.remove(w); }
            for r in &term_reads { current_live.insert(*r); }

            // Simulate statements backward
            for stmt in block.statements.iter().rev() {
                live_at_stmt.push(current_live.clone()); // This is liveness AFTER the statement executes
                
                let (stmt_reads, stmt_writes) = self.get_stmt_reads_writes(stmt);
                for w in &stmt_writes { current_live.remove(w); }
                for r in &stmt_reads { current_live.insert(*r); }
            }
            // Reverse the array so it aligns with the forward iteration of statements
            live_at_stmt.reverse();

            // 3. Now walk FORWARDS to enforce the rules!
            for (stmt_idx, stmt) in block.statements.iter().enumerate() {
                let live_here = &live_at_stmt[stmt_idx];

                let active_borrows: Vec<&Borrow> = all_borrows.iter()
                    .filter(|b| live_here.contains(&b.borrower))
                    .collect();

                if active_borrows.is_empty() { continue; }

                let (stmt_reads, stmt_writes) = self.get_stmt_reads_writes(stmt);

                for active in active_borrows {
                    if let StatementKind::Assign(p, Rvalue::Ref(..)) = &stmt.kind {
                        continue;
                    }

                    let (target_name, target_span) = self.resolve_local(active.target);
                    let (borrower_name, _) = self.resolve_local(active.borrower);
                    
                    // RULE 1 & 2: Cannot mutate target while borrowed
                    if stmt_writes.contains(&active.target) {
                        let kind = if active.is_mut { "mutably" } else { "immutably" };


                        errors.push(self.error(
                            "BC001",
                            format!("cannot assign to `{}` as it is currently {} borrowed", target_name, kind),
                            Some(stmt.span),
                        ).with_note(
                                format!("`{}` is {} borrowed here", target_name, kind),
                                active.creation_span,
                            )
                        );
                    }

                    // RULE 2: Cannot read target if MUTABLY borrowed
                    if active.is_mut && stmt_reads.contains(&active.target) {
                        errors.push(self.error(
                            "BC002",
                            format!("cannot use `{}` because it is currently mutably borrowed", target_name),
                            Some(stmt.span),
                        ).with_note(
                                format!("`{}` is mutably borrowed here", target_name),
                                active.creation_span,
                            )
                        );
                    }
                }
            }

            let live_at_term = live_out.get(&bb).unwrap();
            let active_borrows: Vec<&Borrow> = all_borrows.iter()
                .filter(|b| live_at_term.contains(&b.borrower))
                .collect();

            if !active_borrows.is_empty() {
                let (term_reads, term_writes) = self.get_terminator_reads_writes(&block.terminator);

                for active in active_borrows {
                    let (target_name, target_span) = self.resolve_local(active.target);
                    let span = target_span.unwrap_or_default(); 

                    if term_writes.contains(&active.target) {
                        let kind = if active.is_mut { "mutably" } else { "immutably" };
                        errors.push(self.error("BC001", format!("cannot assign to `{}` as it is currently {} borrowed", target_name, kind), Some(span)));
                    }

                    if active.is_mut && term_reads.contains(&active.target) {
                        errors.push(self.error("BC002", format!("cannot use `{}` because it is currently mutably borrowed", target_name), Some(span)));
                    }
                }
            }
        }
    }

    fn enforce_moves(&self, errors: &mut Vec<HydraError>) {
        for block in &self.mir.basic_blocks {
            let mut moved: HashSet<LocalID> = HashSet::new();

            for stmt in &block.statements {
                match &stmt.kind {
                    StatementKind::Assign(place, rval) => {
                        // Check rvalue for moves of already-moved locals
                        self.check_rvalue_moves(rval, &moved, errors, stmt.span);

                        // If this is a direct assignment (not a projection),
                        // it reinitializes the local — clear its moved status
                        if place.projection.is_empty() {
                            moved.remove(&place.local);
                        }

                        // Record any moves in the rvalue
                        self.record_rvalue_moves(rval, &mut moved);
                    }
                    StatementKind::Drop(place) => {
                        moved.insert(place.local);
                    }
                }
            }

            // Check terminator args for use-after-move
            match &block.terminator {
                Terminator::Call { args, .. } | Terminator::BuiltinCall { args, .. } => {
                    for arg in args {
                        if let Operand::Move(place) = arg {
                            if moved.contains(&place.local) {
                                let (name, span) = self.resolve_local(place.local);
                                errors.push(self.error(
                                    "BC003",
                                    format!("use of moved value `{}`", name),
                                    span,
                                ));
                            }
                            moved.insert(place.local);
                        }
                        if let Operand::Copy(place) = arg {
                            if moved.contains(&place.local) {
                                let (name, span) = self.resolve_local(place.local);
                                errors.push(self.error(
                                    "BC003",
                                    format!("use of moved value `{}`", name),
                                    span,
                                ));
                            }
                        }
                    }
                }
                Terminator::Return => {
                    if moved.contains(&LocalID(0)) {
                        errors.push(self.error("BC003", "use of moved return value", None));
                    }
                }
                _ => {}
            }
        }
    }

    fn enforce_borrow_conflicts(&self, live_out: &HashMap<BasicBlockID, HashSet<LocalID>>, errors: &mut Vec<HydraError>) {
        #[derive(Clone, Debug)]
        struct Borrow {
            borrower: LocalID,
            target: LocalID,
            is_mut: bool,
            span: error::Span,
        }

        let mut borrows: Vec<Borrow> = Vec::new();
        for block in &self.mir.basic_blocks {
            for stmt in &block.statements {
                if let StatementKind::Assign(place, Rvalue::Ref(is_mut, target)) = &stmt.kind {
                    borrows.push(Borrow {
                        borrower: place.local,
                        target: target.local,
                        is_mut: *is_mut,
                        span: stmt.span,
                    });
                }
            }
        }

        // Track reported pairs so we only flag a specific conflict once per function
        let mut reported_conflicts = HashSet::new();

        for (i, block) in self.mir.basic_blocks.iter().enumerate() {
            let bb = BasicBlockID(i);

            let mut live_at_stmt = Vec::new();
            let mut current_live = live_out.get(&bb).unwrap().clone();

            let (term_reads, term_writes) = self.get_terminator_reads_writes(&block.terminator);
            for w in &term_writes { current_live.remove(w); }
            for r in &term_reads { current_live.insert(*r); }

            for stmt in block.statements.iter().rev() {
                live_at_stmt.push(current_live.clone());
                let (stmt_reads, stmt_writes) = self.get_stmt_reads_writes(stmt);
                for w in &stmt_writes { current_live.remove(w); }
                for r in &stmt_reads { current_live.insert(*r); }
            }

            live_at_stmt.reverse();

            for (stmt_idx, _stmt) in block.statements.iter().enumerate() {
                let live_here = &live_at_stmt[stmt_idx];

                let active_borrows: Vec<&Borrow> = borrows.iter()
                    .filter(|b| live_here.contains(&b.borrower))
                    .collect();

                for idx_a in 0..active_borrows.len() {
                    for idx_b in (idx_a + 1)..active_borrows.len() {
                        let a = active_borrows[idx_a];
                        let b = active_borrows[idx_b];

                        if a.target != b.target { continue; }

                        // Conflict if either is mutable
                        if a.is_mut || b.is_mut {
                            // Create canonical pair to prevent duplicates
                            let pair = if a.borrower.0 < b.borrower.0 { 
                                (a.borrower, b.borrower) 
                            } else { 
                                (b.borrower, a.borrower) 
                            };

                            if reported_conflicts.insert(pair) {
                                let (target_name, _) = self.resolve_local(a.target);

                                let msg = if a.is_mut && b.is_mut {
                                    format!("cannot borrow `{}` as mutable more than once at a time", target_name)
                                } else if a.is_mut {
                                    format!("cannot borrow `{}` as immutable because it is also borrowed as mutable", target_name)
                                } else {
                                    format!("cannot borrow `{}` as mutable because it is also borrowed as immutable", target_name)
                                };

                                let (first, second) = if a.borrower.0 < b.borrower.0 { (a, b) } else { (b, a) };

                                errors.push(self.error("BC004", msg, Some(second.span))
                                    .with_note(
                                        format!("`{}` is first borrowed here", target_name),
                                        first.span,
                                    )
                                );
                            }
                        }
                    }
                }
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
                    StatementKind::Drop(place) => {
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
                Terminator::BuiltinCall { args, .. } => {
                    for arg in args {
                        self.extract_uses_operand(arg, &mut gen, &kill);
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
            Rvalue::Ref(_, place) => {
                // Taking a reference to a place requires that place to be alive!
                if !kill.contains(&place.local) {
                    gen.insert(place.local);
                }
            }
            Rvalue::Aggregate(_, operands) => {
                for op in operands {
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
            StatementKind::Drop(place) => {
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
            Rvalue::Ref(is_mut, place) => {
                reads.insert(place.local); // Taking any ref requires reading the memory address
                if *is_mut {
                    // ENFORCING XOR RULE: Creating a &mut reference requires EXCLUSIVE access.
                    // By registering this as a "write", it will instantly conflict with any 
                    // existing immutable or mutable borrows!
                    writes.insert(place.local);
                }
            }
            Rvalue::Aggregate(_, operands) => {
                for op in operands {
                    if let Operand::Copy(p) | Operand::Move(p) = op { reads.insert(p.local); }
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
            Terminator::BuiltinCall { args, .. } => {
                for arg in args {
                    if let Operand::Copy(p) | Operand::Move(p) = arg { reads.insert(p.local); }
                }
            }
            Terminator::Return => { reads.insert(LocalID(0)); }
            _ => {}
        }

        (reads, writes)
    }

    fn extract_rvalue_reads(&self, rvalue: &Rvalue, reads: &mut HashSet<LocalID>) {
        match rvalue {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                if let Operand::Copy(p) | Operand::Move(p) = op { reads.insert(p.local); }
            }
            Rvalue::BinaryOp(_, lhs, rhs) => {
                if let Operand::Copy(p) | Operand::Move(p) = lhs { reads.insert(p.local); }
                if let Operand::Copy(p) | Operand::Move(p) = rhs { reads.insert(p.local); }
            }
            Rvalue::Ref(_, place) => {
                reads.insert(place.local);
            }
            Rvalue::Aggregate(_, operands) => {
                for op in operands {
                    if let Operand::Copy(p) | Operand::Move(p) = op { reads.insert(p.local); }
                }
            }
        }
    }

    fn resolve_local(&self, local: LocalID) -> (String, Option<error::Span>) {
        if let Some(def_id) = self.mir.locals[local.0].debug_def_id {
            if let Some(info) = self.context.get_def(def_id) {
                return (info.name.clone(), Some(info.span));
            }
        }
        
        // If it's a compiler temporary (like `_3`), just return its MIR name and no span
        (format!("_{}", local.0), None)
    }

    fn check_rvalue_moves(&self, rval: &Rvalue, moved: &HashSet<LocalID>, errors: &mut Vec<HydraError>, span: error::Span) 
    {
        let operands: Vec<&Operand> = match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => vec![op],
            Rvalue::BinaryOp(_, l, r) => vec![l, r],
            Rvalue::Ref(_, place) => {
                if moved.contains(&place.local) {
                    let (name, def_span) = self.resolve_local(place.local);
                    errors.push(self.error(
                        "BC003",
                        format!("use of moved value `{}`", name),
                        def_span,
                    ));
                }
                return;
            }
            Rvalue::Aggregate(_, ops) => ops.iter().collect(),
        };

        for op in operands {
            match op {
                Operand::Move(place) | Operand::Copy(place) => {
                    if moved.contains(&place.local) {
                        let (name, _def_span) = self.resolve_local(place.local);

                        errors.push(self.error(
                            "BC003",
                            format!("use of moved value `{}`", name),
                            Some(span),
                        ));
                    }
                }
                Operand::Const(_) => {}
            }
        }
    }

    fn record_rvalue_moves(&self, rval: &Rvalue, moved: &mut HashSet<LocalID>) {
        let operands: Vec<&Operand> = match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => vec![op],
            Rvalue::BinaryOp(_, l, r) => vec![l, r],
            Rvalue::Ref(_, _) => vec![], // borrows don't move
            Rvalue::Aggregate(_, ops) => ops.iter().collect(),
        };

        for op in operands {
            if let Operand::Move(place) = op {
                moved.insert(place.local);
            }
        }
    }
}
