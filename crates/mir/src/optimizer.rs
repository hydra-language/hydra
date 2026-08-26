use std::collections::{HashMap, HashSet, VecDeque};
use crate::{
    MIRProgram, MIRFunction, StatementKind, Rvalue, 
    Operand, Terminator, LocalID, ProjectionElem, Statement,
    Place
};
use ir::hir::{HIRBinOp, HIRUnaryOp};
use ir::Constant;

pub struct Optimizer;

impl Optimizer {

    pub fn optimize(program: &mut MIRProgram) {
        let mut changed = true;
        let mut iterations = 0;

        changed |= Self::inline_functions(program);

        while changed && iterations < 20 {
            changed = false;
    
            for function in &mut program.functions {
                changed |= Self::simplify_dataflow(function);

                // cse and licm here
                changed |= Self::eliminate_common_subexpressions(function);

                changed |= Self::simplify_branches(function);
                changed |= Self::eliminate_dead_blocks(function); 
                changed |= Self::eliminate_dead_stores(function);
                changed |= Self::merge_blocks(function);
            }

            iterations += 1;
        }
    }

    /// Evaluates expressions after propagation until a fixed point is reached 
    /// within the same function. This chains transitive constants (z = y, y = x, x = 5)
    /// and immediately folds the resulting math.
    fn simplify_dataflow(function: &mut MIRFunction) -> bool {
        let mut overall_changed = false;
        let mut local_changed = true;
        let mut iterations = 0;
        
        while local_changed && iterations < 100 {
            local_changed = false;
            
            local_changed |= Self::propagate_copies(function);
            local_changed |= Self::propagate_constants(function);
            local_changed |= Self::propagate_aggregate_fields(function);
            local_changed |= Self::fold_constants(function);
            
            if local_changed {
                overall_changed = true;
            }

            iterations += 1;
        }
        
        overall_changed
    }

    // ========================================================================
    // PHASE 1: LOCAL CLEANUP
    // ========================================================================

    /// Block Merging (CFG Flattening)
    /// If Block A goes unconditionally to Block B, and Block B is ONLY reached 
    /// from Block A, append B into A and bypass B entirely.
    fn merge_blocks(function: &mut MIRFunction) -> bool {
        let mut preds: HashMap<usize, Vec<usize>> = HashMap::new();
        
        // 1. Build a map of Predecessors (Who points to who?)
        for (i, block) in function.basic_blocks.iter().enumerate() {
            match &block.terminator {
                Terminator::Goto { target } => preds.entry(target.0).or_default().push(i),
                Terminator::SwitchInt { true_target, false_target, .. } => {
                    preds.entry(true_target.0).or_default().push(i);
                    preds.entry(false_target.0).or_default().push(i);
                }
                Terminator::Call { target, .. } | Terminator::BuiltinCall { target, .. } => {
                    preds.entry(target.0).or_default().push(i);
                }
                _ => {}
            }
        }

        // 2. Look for an exclusive 1-to-1 connection
        for i in 0..function.basic_blocks.len() {
            if let Terminator::Goto { target } = &function.basic_blocks[i].terminator {
                let target_idx = target.0;
                
                // Prevent merging a block into itself (infinite loop)
                if target_idx == i { continue; }

                if let Some(target_preds) = preds.get(&target_idx) {
                    // If the target is ONLY reached by Block `i`...
                    if target_preds.len() == 1 && target_preds[0] == i {
                        
                        // Extract B's payload
                        let b_stmts = function.basic_blocks[target_idx].statements.clone();
                        let b_term = function.basic_blocks[target_idx].terminator.clone();
                        
                        // Merge into A
                        let block_a = &mut function.basic_blocks[i];
                        block_a.statements.extend(b_stmts);
                        block_a.terminator = b_term;
                        
                        // Neutralize B (Dead Block Elimination will sweep it up in the next loop pass)
                        let block_b = &mut function.basic_blocks[target_idx];
                        block_b.statements.clear();
                        block_b.terminator = Terminator::Unreachable;
                        
                        // We return true immediately. Modifying the CFG invalidates our `preds` map, 
                        // so we let the master loop restart us with a fresh graph.
                        return true; 
                    }
                }
            }
        }
        false
    }

    /// Copy Propagation
    /// If `_X = _Y` is found, and neither are mutated in complex ways,
    /// replace downstream uses of `_X` with `_Y`.
    fn propagate_copies(function: &mut MIRFunction) -> bool {
        let mut assign_counts = HashMap::new();
        let mut copy_values = HashMap::new();

        // 1. Tally up assignments and record pure copies
        for block in &function.basic_blocks {
            for stmt in &block.statements {
                if let StatementKind::Assign(place, rval) = &stmt.kind {
                    assign_counts.entry(place.local).and_modify(|c| *c += 1).or_insert(1);
                    
                    if place.projection.is_empty() {
                        if let Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) = rval {
                            if src.projection.is_empty() {
                                copy_values.insert(place.local, src.local);
                            }
                        }
                    }
                }
            }
        }

        // 2. Safety Check: Only propagate if BOTH variables are assigned exactly once.
        // (Function arguments start at 0 assignments in the body, so src_count <= 1 is safe).
        let safe_copies: HashMap<LocalID, LocalID> = copy_values
            .into_iter()
            .filter(|(dest, src)| {
                let dest_count = assign_counts.get(dest).copied().unwrap_or(0);
                let src_count = assign_counts.get(src).copied().unwrap_or(0);
                dest_count == 1 && src_count <= 1
            })
            .collect();

        if safe_copies.is_empty() { return false; }
        
        let mut changed = false;

        // 3. Replace Uses
        for block in &mut function.basic_blocks {
            for stmt in &mut block.statements {
                if let StatementKind::Assign(_, rval) = &mut stmt.kind {
                    changed |= Self::replace_locals_in_rvalue(rval, &safe_copies);
                }
            }

            match &mut block.terminator {
                Terminator::SwitchInt { discriminant, .. } => {
                    changed |= Self::replace_local(discriminant, &safe_copies);
                }
                Terminator::Call { args, .. } | Terminator::BuiltinCall { args, .. } => {
                    for arg in args {
                        changed |= Self::replace_local(arg, &safe_copies);
                    }
                }
                _ => {}
            }
        }

        changed
    }

    /// Finds variables assigned a constant exactly once, and replaces all 
    /// downstream uses of that variable with the raw constant.
    fn propagate_constants(function: &mut MIRFunction) -> bool {
        let mut assign_counts = HashMap::new();
        let mut const_values = HashMap::new();

        for block in &function.basic_blocks {
            for stmt in &block.statements {
                if let StatementKind::Assign(place, rval) = &stmt.kind {
                    if place.projection.is_empty() {
                        let count = assign_counts.entry(place.local).or_insert(0);
                        *count += 1;
                        if let Rvalue::Use(Operand::Const(c)) = rval {
                            const_values.insert(place.local, c.clone());
                        }
                    } else {
                        assign_counts.entry(place.local).and_modify(|c| *c += 1).or_insert(1);
                    }
                }
            }
        }

        let safe_constants: HashMap<LocalID, Constant> = const_values
            .into_iter()
            .filter(|(local, _)| assign_counts.get(local) == Some(&1))
            .collect();

        if safe_constants.is_empty() { return false; }
        let mut changed = false;

        for block in &mut function.basic_blocks {
            for stmt in &mut block.statements {
                if let StatementKind::Assign(_, rval) = &mut stmt.kind {
                    changed |= Self::replace_operands_in_rvalue(rval, &safe_constants);
                }
            }

            match &mut block.terminator {
                Terminator::SwitchInt { discriminant, .. } => {
                    changed |= Self::replace_operand(discriminant, &safe_constants);
                }
                Terminator::Call { args, .. } | Terminator::BuiltinCall { args, .. } => {
                    for arg in args {
                        changed |= Self::replace_operand(arg, &safe_constants);
                    }
                }
                _ => {}
            }
        }
        changed
    }

    fn propagate_aggregate_fields(function: &mut MIRFunction) -> bool {
        let mut whole_assign_counts: HashMap<LocalID, usize> = HashMap::new();
        let mut aggregates: HashMap<LocalID, Vec<Operand>> = HashMap::new();
        let mut origin: HashMap<LocalID, LocalID> = HashMap::new();

        for block in &function.basic_blocks {
            for stmt in &block.statements {
                if let StatementKind::Assign(place, rval) = &stmt.kind {
                    if place.projection.is_empty() {
                        *whole_assign_counts.entry(place.local).or_insert(0) += 1;
                        match rval {
                            Rvalue::Aggregate(_, ops) => { aggregates.insert(place.local, ops.clone()); }
                            Rvalue::Ref(_, inner) if inner.projection.is_empty() => {
                                origin.insert(place.local, inner.local);
                            }
                            Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) if src.projection.is_empty() => {
                                origin.insert(place.local, src.local);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let safe_origin: HashMap<LocalID, LocalID> = origin
            .into_iter()
            .filter(|(local, _)| whole_assign_counts.get(local) == Some(&1))
            .collect();

        let resolve_root = |mut local: LocalID| -> LocalID {
            let mut steps = 0;
            while steps < 16 {
                match safe_origin.get(&local) {
                    Some(&next) => { local = next; steps += 1; }
                    None => return local,
                }
            }
            local
        };

        let mut mutated_aggregates: HashSet<LocalID> = HashSet::new();
        for block in &function.basic_blocks {
            for stmt in &block.statements {
                if let StatementKind::Assign(place, _) = &stmt.kind {
                    if !place.projection.is_empty() {
                        mutated_aggregates.insert(resolve_root(place.local));
                    }
                }
            }
        }

        let safe_aggregates: HashMap<LocalID, Vec<Operand>> = aggregates
            .into_iter()
            .filter(|(local, _)| whole_assign_counts.get(local) == Some(&1))
            .filter(|(local, _)| !mutated_aggregates.contains(local))
            .collect();

        if safe_aggregates.is_empty() { return false; }
        let mut changed = false;

        let resolve = |local: LocalID| -> LocalID {
            let root = resolve_root(local);
            if safe_aggregates.contains_key(&root) { root } else { local }
        };

        let try_replace = |op: &mut Operand, aggs: &HashMap<LocalID, Vec<Operand>>| -> bool {
            if let Operand::Copy(place) | Operand::Move(place) = op {
                if let [ProjectionElem::Field(idx)] = place.projection.as_slice() {
                    let base = resolve(place.local);
                    if let Some(fields) = aggs.get(&base) {
                        if let Some(field_op) = fields.get(*idx) {
                            *op = field_op.clone();
                            return true;
                        }
                    }
                }
            }
            false
        };

        for block in &mut function.basic_blocks {
            for stmt in &mut block.statements {
                if let StatementKind::Assign(_, rval) = &mut stmt.kind {
                    changed |= Self::replace_operands_in_rvalue_with(rval, &|op| try_replace(op, &safe_aggregates));
                }
            }
            match &mut block.terminator {
                Terminator::SwitchInt { discriminant, .. } => { changed |= try_replace(discriminant, &safe_aggregates); }
                Terminator::Call { args, .. } | Terminator::BuiltinCall { args, .. } => {
                    for arg in args { changed |= try_replace(arg, &safe_aggregates); }
                }
                _ => {}
            }
        }

        changed
    }

    fn fold_constants(function: &mut MIRFunction) -> bool {
        let mut changed = false;
        for block in &mut function.basic_blocks {
            for stmt in &mut block.statements {
                if let StatementKind::Assign(_place, rval) = &mut stmt.kind {
                    match rval {
                        // 1. Binary Operations & Comparisons
                        Rvalue::BinaryOp(op, Operand::Const(c1), Operand::Const(c2)) => {
                            let folded = match (op, c1, c2) {
                                // Integer Arithmetic
                                (HIRBinOp::Add, Constant::Int(i1, t1), Constant::Int(i2, _)) => Some(Constant::Int(*i1 + *i2, t1.clone())),
                                (HIRBinOp::Sub, Constant::Int(i1, t1), Constant::Int(i2, _)) => Some(Constant::Int(*i1 - *i2, t1.clone())),
                                (HIRBinOp::Mul, Constant::Int(i1, t1), Constant::Int(i2, _)) => Some(Constant::Int(*i1 * *i2, t1.clone())),
                                (HIRBinOp::Div, Constant::Int(i1, t1), Constant::Int(i2, _)) if *i2 != 0 => Some(Constant::Int(*i1 / *i2, t1.clone())),
                                (HIRBinOp::Mod, Constant::Int(i1, t1), Constant::Int(i2, _)) if *i2 != 0 => Some(Constant::Int(*i1 % *i2, t1.clone())),
                                
                                // Float Arithmetic
                                (HIRBinOp::Add, Constant::Float(f1, t1), Constant::Float(f2, _)) => Some(Constant::Float(*f1 + *f2, t1.clone())),
                                (HIRBinOp::Sub, Constant::Float(f1, t1), Constant::Float(f2, _)) => Some(Constant::Float(*f1 - *f2, t1.clone())),
                                (HIRBinOp::Mul, Constant::Float(f1, t1), Constant::Float(f2, _)) => Some(Constant::Float(*f1 * *f2, t1.clone())),
                                (HIRBinOp::Div, Constant::Float(f1, t1), Constant::Float(f2, _)) if *f2 != 0.0 => Some(Constant::Float(*f1 / *f2, t1.clone())),

                                // Integer Comparisons
                                (HIRBinOp::Eq, Constant::Int(i1, _), Constant::Int(i2, _)) => Some(Constant::Bool(i1 == i2)),
                                (HIRBinOp::Ne, Constant::Int(i1, _), Constant::Int(i2, _)) => Some(Constant::Bool(i1 != i2)),
                                (HIRBinOp::Lt, Constant::Int(i1, _), Constant::Int(i2, _)) => Some(Constant::Bool(i1 < i2)),
                                (HIRBinOp::Le, Constant::Int(i1, _), Constant::Int(i2, _)) => Some(Constant::Bool(i1 <= i2)),
                                (HIRBinOp::Gt, Constant::Int(i1, _), Constant::Int(i2, _)) => Some(Constant::Bool(i1 > i2)),
                                (HIRBinOp::Ge, Constant::Int(i1, _), Constant::Int(i2, _)) => Some(Constant::Bool(i1 >= i2)),

                                // Float Comparisons
                                (HIRBinOp::Eq, Constant::Float(f1, _), Constant::Float(f2, _)) => Some(Constant::Bool(f1 == f2)),
                                (HIRBinOp::Ne, Constant::Float(f1, _), Constant::Float(f2, _)) => Some(Constant::Bool(f1 != f2)),
                                (HIRBinOp::Lt, Constant::Float(f1, _), Constant::Float(f2, _)) => Some(Constant::Bool(f1 < f2)),
                                (HIRBinOp::Le, Constant::Float(f1, _), Constant::Float(f2, _)) => Some(Constant::Bool(f1 <= f2)),
                                (HIRBinOp::Gt, Constant::Float(f1, _), Constant::Float(f2, _)) => Some(Constant::Bool(f1 > f2)),
                                (HIRBinOp::Ge, Constant::Float(f1, _), Constant::Float(f2, _)) => Some(Constant::Bool(f1 >= f2)),

                                // Boolean Logic
                                (HIRBinOp::And, Constant::Bool(b1), Constant::Bool(b2)) => Some(Constant::Bool(*b1 && *b2)),
                                (HIRBinOp::Or, Constant::Bool(b1), Constant::Bool(b2)) => Some(Constant::Bool(*b1 || *b2)),
                                (HIRBinOp::Eq, Constant::Bool(b1), Constant::Bool(b2)) => Some(Constant::Bool(b1 == b2)),
                                (HIRBinOp::Ne, Constant::Bool(b1), Constant::Bool(b2)) => Some(Constant::Bool(b1 != b2)),

                                _ => None,
                            };

                            if let Some(new_const) = folded {
                                *rval = Rvalue::Use(Operand::Const(new_const));
                                changed = true;
                            }
                        }

                        // 2. Unary Operations
                        Rvalue::UnaryOp(op, Operand::Const(c)) => {
                            let folded = match (op, c) {
                                (HIRUnaryOp::Not, Constant::Bool(b)) => Some(Constant::Bool(!*b)),
                                (HIRUnaryOp::Neg, Constant::Int(i, ty)) => Some(Constant::Int(-*i, ty.clone())),
                                (HIRUnaryOp::Neg, Constant::Float(f, ty)) => Some(Constant::Float(-*f, ty.clone())),
                                _ => None,
                            };
                            
                            if let Some(new_const) = folded {
                                *rval = Rvalue::Use(Operand::Const(new_const));
                                changed = true;
                            }
                        }
                        
                        _ => {}
                    }
                }
            }
        }
        changed
    }

    fn eliminate_dead_blocks(function: &mut MIRFunction) -> bool {
        if function.basic_blocks.is_empty() { return false; }

        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(0); 

        while let Some(bb_idx) = queue.pop_front() {
            if !reachable.insert(bb_idx) { continue; }
            let block = &function.basic_blocks[bb_idx];
            match &block.terminator {
                Terminator::Goto { target } => queue.push_back(target.0),
                Terminator::SwitchInt { true_target, false_target, .. } => {
                    queue.push_back(true_target.0);
                    queue.push_back(false_target.0);
                }
                Terminator::Call { target, .. } | Terminator::BuiltinCall { target, .. } => {
                    queue.push_back(target.0);
                }
                Terminator::Return | Terminator::Unreachable => {}
            }
        }

        if reachable.len() == function.basic_blocks.len() {
            return false;
        }

        let mut new_blocks = Vec::new();
        let mut old_to_new = HashMap::new();

        for old_idx in 0..function.basic_blocks.len() {
            if reachable.contains(&old_idx) {
                old_to_new.insert(old_idx, new_blocks.len());
                new_blocks.push(function.basic_blocks[old_idx].clone());
            }
        }

        for block in &mut new_blocks {
            match &mut block.terminator {
                Terminator::Goto { target } => target.0 = old_to_new[&target.0],
                Terminator::SwitchInt { true_target, false_target, .. } => {
                    true_target.0 = old_to_new[&true_target.0];
                    false_target.0 = old_to_new[&false_target.0];
                }
                Terminator::Call { target, .. } | Terminator::BuiltinCall { target, .. } => {
                    target.0 = old_to_new[&target.0];
                }
                _ => {}
            }
        }

        function.basic_blocks = new_blocks;
        true
    }

    fn eliminate_dead_stores(function: &mut MIRFunction) -> bool {
        let mut loop_changed = false;
        let mut overall_changed = false;
        
        loop {
            loop_changed = false;
            let mut read_counts: HashMap<LocalID, usize> = HashMap::new();

            for block in &function.basic_blocks {
                for stmt in &block.statements {
                    match &stmt.kind {
                        StatementKind::Assign(place, rval) => {
                            Self::count_reads_in_rvalue(rval, &mut read_counts);
                            for proj in &place.projection {
                                if let ProjectionElem::Index(idx_local) = proj {
                                    *read_counts.entry(*idx_local).or_insert(0) += 1;
                                }
                            }
                        }
                        StatementKind::Drop(place) => {
                            *read_counts.entry(place.local).or_insert(0) += 1;
                            for proj in &place.projection {
                                if let ProjectionElem::Index(idx_local) = proj {
                                    *read_counts.entry(*idx_local).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                }

                match &block.terminator {
                    Terminator::SwitchInt { discriminant, .. } => Self::count_reads(discriminant, &mut read_counts),
                    Terminator::Call { args, destination, .. } => {
                        for arg in args { Self::count_reads(arg, &mut read_counts); }
                        for proj in &destination.projection {
                            if let ProjectionElem::Index(idx_local) = proj {
                                *read_counts.entry(*idx_local).or_insert(0) += 1;
                            }
                        }
                    }
                    Terminator::BuiltinCall { args, .. } => {
                        for arg in args { Self::count_reads(arg, &mut read_counts); }
                    }
                    _ => {}
                }
            }

            for block in &mut function.basic_blocks {
                let original_len = block.statements.len();
                
                block.statements.retain(|stmt| {
                    if let StatementKind::Assign(place, rval) = &stmt.kind {

                        // never delete an effectful intrinsic merely because
                        // its result is unused.
                        if let Rvalue::Intrinsic { kind, .. } = rval {
                            if kind.has_side_effects() {
                                return true;
                            }
                        }

                        if place.local.0 == 0 || !place.projection.is_empty() { return true; }
                        read_counts.get(&place.local).unwrap_or(&0) > &0
                    } else {
                        true 
                    }
                });

                if block.statements.len() < original_len {
                    loop_changed = true;
                    overall_changed = true;
                }
            }
            
            if !loop_changed { break; }
        }
        overall_changed
    }

    fn simplify_branches(function: &mut MIRFunction) -> bool {
        let mut changed = false;
        for block in &mut function.basic_blocks {
            if let Terminator::SwitchInt { discriminant, true_target, false_target } = &block.terminator {
                if let Operand::Const(Constant::Bool(val)) = discriminant {
                    let definitive_target = if *val { *true_target } else { *false_target };
                    block.terminator = Terminator::Goto { target: definitive_target };
                    changed = true;
                }
            }
        }
        changed
    }

    /// Sweeps the program for function calls to `#[inline]` functions, 
    /// clones their MIR, remaps their variables/blocks, and splices them into the caller.
    fn inline_functions(program: &mut MIRProgram) -> bool {
        let mut overall_changed = false;

        // 1. Identify which functions are eligible for inlining
        let inlineable_fn_names: Vec<String> = program.functions
            .iter()
            .filter(|f| f.is_inline)
            .map(|f| f.name.clone())
            .collect();

        if inlineable_fn_names.is_empty() { return false; }

        // 2. Process functions one by one to avoid double-borrowing the whole program
        for i in 0..program.functions.len() {
            let mut local_changed = true;

            while local_changed {
                local_changed = false;

                // Find a call site in the current function
                let mut target_site = None;
                for (bb_idx, block) in program.functions[i].basic_blocks.iter().enumerate() {
                    if let Terminator::Call { callee, args, destination, target } = &block.terminator {
                        if inlineable_fn_names.contains(callee) {
                            // Find the callee
                            if let Some(callee_mir) = program.functions.iter().find(|f| &f.name == callee) {
                                target_site = Some((bb_idx, callee_mir.clone(), args.clone(), destination.clone(), *target));
                                break;
                            }
                        }
                    }
                }

                if let Some((caller_bb_idx, callee, args, destination, return_target)) = target_site {
                    // Now we mutate the specific function we are currently borrowing
                    let caller = &mut program.functions[i];
                    let local_offset = caller.locals.len();
                    let block_offset = caller.basic_blocks.len();

                    // Append Locals
                    for (idx, decl) in callee.locals.into_iter().enumerate() {
                        if idx == 0 { continue; }
                        caller.locals.push(decl);
                    }

                    // Prepare blocks
                    let mut callee_blocks = callee.basic_blocks;
                    Self::remap_locals_and_blocks(&mut callee_blocks, local_offset, block_offset, destination.local);

                    // Wire Exit
                    for block in &mut callee_blocks {
                        if let Terminator::Return = block.terminator {
                            block.terminator = Terminator::Goto { target: return_target };
                        }
                    }

                    // Wire Entrance
                    let mut parameter_assignments = Vec::new();
                    for (idx, arg) in args.into_iter().enumerate() {
                        let param_local = LocalID(idx + 1 + local_offset - 1);
                        parameter_assignments.push(Statement {
                            kind: StatementKind::Assign(
                                Place { local: param_local, projection: vec![] },
                                Rvalue::Use(arg)
                            ),
                            span: errors::error::Span::default()
                        });
                    }

                    let caller_block = &mut caller.basic_blocks[caller_bb_idx];
                    caller_block.statements.extend(parameter_assignments);
                    caller_block.terminator = Terminator::Goto { target: crate::BasicBlockID(block_offset) };

                    // Append Blocks
                    caller.basic_blocks.extend(callee_blocks);

                    local_changed = true;
                    overall_changed = true;
                }
            }
        }

        overall_changed
    }

    fn eliminate_common_subexpressions(func: &mut MIRFunction) -> bool {
        let mut changed = false;

        for block in &mut func.basic_blocks {
            // Cache stores (Rvalue, Destination Local)
            // We use a Vec because Rvalue does not derive Hash
            let mut available: Vec<(Rvalue, LocalID)> = Vec::new();

            for stmt in &mut block.statements {
                match &mut stmt.kind {
                    StatementKind::Assign(place, rval) => {
                        // 1. Is this math we can optimize?
                        if Self::is_cse_candidate(rval) {
                            
                            // Look for a match in our cache
                            let mut found = None;
                            for (avail_rval, avail_local) in &available {
                                if Self::is_rvalue_eq(rval, avail_rval) {
                                    found = Some(*avail_local);
                                    break;
                                }
                            }

                            // If found, replace the math with a direct copy of the cached local
                            if let Some(cached_local) = found {
                                *rval = Rvalue::Use(Operand::Copy(Place {
                                    local: cached_local,
                                    projection: vec![],
                                }));
                                changed = true;
                            } else {
                                // If not found, add it to the cache for future statements to use
                                if place.projection.is_empty() {
                                    available.push((rval.clone(), place.local));
                                }
                            }
                        }

                        // 2. INVALIDATION: 
                        // If we assign to `_1`, any cached math using `_1` (e.g. `_1 + _2`) is now poisoned.
                        // Furthermore, if `_1` was holding a cached value, it isn't anymore.
                        let mutated = place.local;
                        available.retain(|(avail_rval, avail_local)| {
                            *avail_local != mutated && !Self::rvalue_uses_local(avail_rval, mutated)
                        });
                    }
                    StatementKind::Drop(place) => {
                        // Dropping a variable poisons it as well
                        let mutated = place.local;
                        available.retain(|(avail_rval, avail_local)| {
                            *avail_local != mutated && !Self::rvalue_uses_local(avail_rval, mutated)
                        });
                    }
                }
            }
        }
        changed
    }

    // ========================================================================
    // HELPERS
    // ========================================================================

    fn replace_operands_in_rvalue(rval: &mut Rvalue, safe_constants: &HashMap<LocalID, Constant>) -> bool {
        let mut changed = false;
        match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                changed |= Self::replace_operand(op, safe_constants);
            }

            Rvalue::BinaryOp(_, lhs, rhs) => {
                changed |= Self::replace_operand(lhs, safe_constants);
                changed |= Self::replace_operand(rhs, safe_constants);
            }

            Rvalue::Aggregate(_, ops) => {
                for op in ops { changed |= Self::replace_operand(op, safe_constants); }
            }

            Rvalue::Ref(_, _) => {}

            Rvalue::Intrinsic { args, .. } => {
                for op in args {
                    changed |= Self::replace_operand(
                        op,
                        safe_constants,
                    );
                }
            }
        }

        changed
    }

    /// Same traversal as `replace_operands_in_rvalue`, but takes a closure instead
    /// of being hardcoded to constant-substitution. Lets both propagate_constants-style
    /// passes and propagate_aggregate_fields share one traversal instead of duplicating
    /// the Rvalue match arms twice.
    fn replace_operands_in_rvalue_with<F: Fn(&mut Operand) -> bool>(rval: &mut Rvalue, replace: &F) -> bool {
        let mut changed = false;
        match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                changed |= replace(op);
            }
            Rvalue::BinaryOp(_, lhs, rhs) => {
                changed |= replace(lhs);
                changed |= replace(rhs);
            }
            Rvalue::Aggregate(_, ops) => {
                for op in ops { changed |= replace(op); }
            }
            Rvalue::Ref(_, _) => {}

            Rvalue::Intrinsic { args, .. } => {
                for op in args {
                    changed |= replace(op);
                }
            }
        }

        changed
    }

    fn replace_operand(op: &mut Operand, safe_constants: &HashMap<LocalID, Constant>) -> bool {
        if let Operand::Copy(place) | Operand::Move(place) = op {
            if place.projection.is_empty() {
                if let Some(c) = safe_constants.get(&place.local) {
                    *op = Operand::Const(c.clone());
                    return true;
                }
            }
        }
        false
    }

    fn replace_locals_in_rvalue(rval: &mut Rvalue, safe_copies: &HashMap<LocalID, LocalID>) -> bool {
        let mut changed = false;
        match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                changed |= Self::replace_local(op, safe_copies);
            }
            Rvalue::BinaryOp(_, lhs, rhs) => {
                changed |= Self::replace_local(lhs, safe_copies);
                changed |= Self::replace_local(rhs, safe_copies);
            }
            Rvalue::Aggregate(_, ops) => {
                for op in ops { changed |= Self::replace_local(op, safe_copies); }
            }
            Rvalue::Ref(_, place) => {
                if place.projection.is_empty() {
                    if let Some(&src_local) = safe_copies.get(&place.local) {
                        place.local = src_local;
                        changed = true;
                    }
                }
            }

            Rvalue::Intrinsic { args, .. } => {
                for op in args {
                    changed |= Self::replace_local(
                        op,
                        safe_copies,
                    );
                }
            }
        }
        changed
    }

    fn replace_local(op: &mut Operand, safe_copies: &HashMap<LocalID, LocalID>) -> bool {
        if let Operand::Copy(place) | Operand::Move(place) = op {
            if place.projection.is_empty() {
                if let Some(&src_local) = safe_copies.get(&place.local) {
                    place.local = src_local;
                    return true;
                }
            }
        }
        false
    }

    fn count_reads_in_rvalue(rval: &Rvalue, counts: &mut HashMap<LocalID, usize>) {
        match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                Self::count_reads(op, counts);
            }
            Rvalue::BinaryOp(_, lhs, rhs) => {
                Self::count_reads(lhs, counts);
                Self::count_reads(rhs, counts);
            }
            Rvalue::Aggregate(_, ops) => {
                for op in ops { Self::count_reads(op, counts); }
            }
            Rvalue::Ref(_, place) => {
                *counts.entry(place.local).or_insert(0) += 1;
                for proj in &place.projection {
                    if let ProjectionElem::Index(idx_local) = proj {
                        *counts.entry(*idx_local).or_insert(0) += 1;
                    }
                }
            }

            Rvalue::Intrinsic { args, .. } => {
                for op in args {
                    Self::count_reads(op, counts);
                }
            }
        }
    }

    fn count_reads(op: &Operand, counts: &mut HashMap<LocalID, usize>) {
        if let Operand::Copy(place) | Operand::Move(place) = op {
            *counts.entry(place.local).or_insert(0) += 1;
            for proj in &place.projection {
                if let ProjectionElem::Index(idx_local) = proj {
                    *counts.entry(*idx_local).or_insert(0) += 1;
                }
            }
        }
    }

    fn remap_locals_and_blocks(blocks: &mut Vec<crate::BasicBlock>, local_offset: usize, block_offset: usize,destination_local: LocalID) 
    {
        // Helper closure to shift a single LocalID
        let shift_local = |local: &mut LocalID| {
            if local.0 == 0 {
                // Callee's `_0` becomes the caller's destination local!
                *local = destination_local;
            } else {
                // Shift all other locals up
                local.0 += local_offset - 1; 
            }
        };

        for block in blocks.iter_mut() {
            // Remap Statements
            for stmt in &mut block.statements {
                match &mut stmt.kind {
                    StatementKind::Assign(place, rval) => {
                        shift_local(&mut place.local);
                        // You will need to implement a small helper `visit_locals_in_rvalue` 
                        // that runs `shift_local` on every LocalID inside the Rvalue/Operands.
                        Self::shift_locals_in_rvalue(rval, &shift_local);
                    }
                    StatementKind::Drop(place) => {
                        shift_local(&mut place.local);
                    }
                }
            }

            // Remap Terminators
            match &mut block.terminator {
                Terminator::Goto { target } => target.0 += block_offset,
                Terminator::SwitchInt { discriminant, true_target, false_target } => {
                    Self::shift_locals_in_operand(discriminant, &shift_local);
                    true_target.0 += block_offset;
                    false_target.0 += block_offset;
                }
                Terminator::Call { args, destination, target, .. } => {
                    for arg in args { Self::shift_locals_in_operand(arg, &shift_local); }
                    shift_local(&mut destination.local);
                    target.0 += block_offset;
                }
                Terminator::BuiltinCall { args, target, .. } => {
                    for arg in args { Self::shift_locals_in_operand(arg, &shift_local); }
                    target.0 += block_offset;
                }
                _ => {}
            }
        }
    }

    fn shift_locals_in_rvalue<F: Fn(&mut LocalID)>(rval: &mut Rvalue, shift: &F) {
        match rval {
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => Self::shift_locals_in_operand(op, shift),
            Rvalue::BinaryOp(_, lhs, rhs) => {
                Self::shift_locals_in_operand(lhs, shift);
                Self::shift_locals_in_operand(rhs, shift);
            }
            Rvalue::Aggregate(_, ops) => {
                for op in ops { Self::shift_locals_in_operand(op, shift); }
            }
            Rvalue::Ref(_, place) => shift(&mut place.local),

            Rvalue::Intrinsic { args, .. } => {
                for op in args {
                    Self::shift_locals_in_operand(op, shift);
                }
            }
        }
    }

    fn shift_locals_in_operand<F: Fn(&mut LocalID)>(op: &mut Operand, shift: &F) {
        if let Operand::Copy(place) | Operand::Move(place) = op {
            shift(&mut place.local);
        }
    }

    // We restrict CSE to Binary and Unary math. Doing this on Refs or Aggregates is risky.
    fn is_cse_candidate(rval: &Rvalue) -> bool {
        matches!(rval, Rvalue::BinaryOp(_, _, _) | Rvalue::UnaryOp(_, _))
    }

    // Custom equality checks because `Rvalue` doesn't #[derive(PartialEq)]
    fn is_rvalue_eq(a: &Rvalue, b: &Rvalue) -> bool {
        match (a, b) {
            (Rvalue::BinaryOp(op_a, l_a, r_a), Rvalue::BinaryOp(op_b, l_b, r_b)) => {
                op_a == op_b && Self::is_operand_eq(l_a, l_b) && Self::is_operand_eq(r_a, r_b)
            }
            (Rvalue::UnaryOp(op_a, o_a), Rvalue::UnaryOp(op_b, o_b)) => {
                op_a == op_b && Self::is_operand_eq(o_a, o_b)
            }
            _ => false,
        }
    }

    fn is_operand_eq(a: &Operand, b: &Operand) -> bool {
        match (a, b) {
            (Operand::Copy(p_a), Operand::Copy(p_b)) |
            (Operand::Move(p_a), Operand::Move(p_b)) => p_a == p_b,
            (Operand::Const(c_a), Operand::Const(c_b)) => c_a == c_b,
            _ => false,
        }
    }

    // Used to detect if a cached mathematical operation relies on a local that just got modified
    fn rvalue_uses_local(rval: &Rvalue, target: LocalID) -> bool {
        match rval {
            Rvalue::BinaryOp(_, l, r) => Self::operand_uses_local(l, target) || Self::operand_uses_local(r, target),
            Rvalue::UnaryOp(_, op) => Self::operand_uses_local(op, target),
            Rvalue::Use(op) | Rvalue::Cast(_, op, _) => Self::operand_uses_local(op, target),
            Rvalue::Aggregate(_, ops) => ops.iter().any(|op| Self::operand_uses_local(op, target)),
            Rvalue::Ref(_, place) => place.local == target,
            Rvalue::Intrinsic { args, .. } => {
                args.iter().any(|op| {
                    Self::operand_uses_local(op, target)
                })
            }
        }
    }

    fn operand_uses_local(op: &Operand, target: LocalID) -> bool {
        match op {
            Operand::Copy(p) | Operand::Move(p) => p.local == target,
            Operand::Const(_) => false,
        }
    }
}
