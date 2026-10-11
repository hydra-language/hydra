use std::collections::{HashSet, VecDeque};

use errors::error::{HydraError, Span};
use mir::{LocalID, MIRFunction, Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator};
use mir::visit::{self, PlaceContext, Visitor};

use ir::types::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LoanID(usize);

struct Loan {
    borrower: LocalID,
    mutable: bool,
    span: Span,
    // Parameter provenance tokens are not new borrow conflicts.
    ambient: bool,
}

// a resolved target carries the chain of reference loans through which it was reached
#[derive(Clone, Debug, PartialEq, Eq)]
struct Origin {
    place: Place,
    via: Vec<LoanID>,
    // The referent is outside this function's owned stack storage.
    external: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Slot { 
    fields: Vec<usize>, 
    loans: HashSet<LoanID> 
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LivePlace {
    local: LocalID,
    fields: Vec<usize>,
}

impl LivePlace {

    fn new(place: &Place) -> Self {
        let fields = place.projection.iter()
            .take_while(|projection| matches!(projection, ProjectionElem::Field { .. }))
            .filter_map(|projection| match projection {
                ProjectionElem::Field { index, .. } => Some(*index),
                _ => None,
            })
            .collect();

        Self { local: place.local, fields }
    }

    fn overlaps(&self, local: LocalID, fields: &[usize]) -> bool {
        self.local == local && (self.fields.starts_with(fields) || fields.starts_with(&self.fields))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoanState {
    // reference values owned by a local, indexed by their field path
    slots: Vec<Vec<Slot>>,
    // the referent is captured at loan creation, not looked up later through its borrower
    origins: Vec<Vec<Origin>>,
}

impl LoanState {

    fn empty(locals: usize, loans: usize) -> Self {
        Self { slots: vec![Vec::new(); locals], origins: vec![Vec::new(); loans] }
    }

    fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;

        for (ours, theirs) in self.slots.iter_mut().zip(&other.slots) {
            for slot in theirs {
                if let Some(existing) = ours.iter_mut().find(|s| s.fields == slot.fields) {
                    let old = existing.loans.len();
                    existing.loans.extend(slot.loans.iter().copied());

                    changed |= old != existing.loans.len();
                } else {
                    ours.push(slot.clone());
                    changed = true;
                }
            }
        }

        for (ours, theirs) in self.origins.iter_mut().zip(&other.origins) {
            for origin in theirs {
                if !ours.contains(origin) { 
                    ours.push(origin.clone()); 
                    changed = true; 
                }
            }
        }

        changed
    }

    fn active(&self, live: &HashSet<LivePlace>) -> HashSet<LoanID> {
        let mut active = HashSet::new();
        let mut pending = Vec::new();

        for place in live {
            for slot in &self.slots[place.local.0] {
                if place.overlaps(place.local, &slot.fields) {
                    pending.extend(slot.loans.iter().copied());
                }
            }
        }

        while let Some(id) = pending.pop() {
            if active.insert(id) {
                for origin in &self.origins[id.0] {
                    pending.extend(origin.via.iter().copied());
                }
            }
        }

        active
    }

    fn read(&self, place: &Place) -> Vec<Slot> {
        let Some(fields) = field_path(place) else {
            // unknown index/deref: conservatively collect every reference in the base local
            return self.slots[place.local.0].clone();
        };

        self.slots[place.local.0].iter().filter_map(|slot| {
            slot.fields.strip_prefix(fields.as_slice()).map(|suffix| Slot {
                fields: suffix.to_vec(), loans: slot.loans.clone(),
            })
        }).collect()
    }

    fn write(&mut self, place: &Place, values: Vec<Slot>) {
        let Some(fields) = field_path(place) else {
            // potentially aliased destination: never kill another path's provenance
            for slot in values { 
                self.union_slot(place.local, slot.fields, slot.loans);
            }

            return;
        };

        self.slots[place.local.0].retain(|slot| !slot.fields.starts_with(&fields));

        for slot in values {
            let mut key = fields.clone();
            key.extend(slot.fields);

            self.union_slot(place.local, key, slot.loans);
        }
    }

    fn union_slot(&mut self, local: LocalID, fields: Vec<usize>, loans: HashSet<LoanID>) {
        if loans.is_empty() { 
            return; 
        }

        if let Some(slot) = self.slots[local.0].iter_mut().find(|s| s.fields == fields) {
            slot.loans.extend(loans);
        } else {
            self.slots[local.0].push(Slot { fields, loans });
        }
    }

    fn weak_write(&mut self, place: &Place, values: &[Slot]) {
        if let Some(fields) = field_path(place) {
            for slot in values {
                let mut key = fields.clone();
                key.extend(&slot.fields);

                self.union_slot(place.local, key, slot.loans.clone());
            }
        } else {
            let loans: HashSet<LoanID> = values.iter().flat_map(|slot| slot.loans.iter().copied()).collect();

            // without alias information an unresolved pointer could address any local
            for i in 0..self.slots.len() { self.union_slot(LocalID(i), Vec::new(), loans.clone()); }
        }
    }

    fn remove(&mut self, place: &Place) {
        if let Some(fields) = field_path(place) {
            self.slots[place.local.0].retain(|slot| !slot.fields.starts_with(&fields));
        }
    }
}

fn field_path(place: &Place) -> Option<Vec<usize>> {
    place.projection.iter().map(|p| match p {
        ProjectionElem::Field { index, .. } => Some(*index),
        ProjectionElem::Deref | ProjectionElem::Index(_) => None,
    }).collect()
}

// different struct fields are disjoint; prefixes overlap; dynamic indices might alias
fn places_overlap(a: &Place, b: &Place) -> bool {
    if a.local != b.local { 
        return false; 
    }

    for (index, (left, right)) in a.projection.iter().zip(&b.projection).enumerate() {
        if let (ProjectionElem::Field { index: x, .. }, ProjectionElem::Field { index: y, .. }) = (left, right) {
            if x != y {
                return a.projection[index + 1..].iter().chain(&b.projection[index + 1..]).any(|p| matches!(p, ProjectionElem::Deref));
            }
        }
    }

    true
}

#[derive(Default)]
struct Accesses(Vec<(Place, PlaceContext)>);

impl Visitor for Accesses {
    fn visit_place(&mut self, place: &Place, context: PlaceContext) {
        self.0.push((place.clone(), context));
        visit::walk_place(self, place);
    }
}

impl Accesses {

    fn statement(stmt: &Statement) -> Self { 
        let mut v = Self::default(); 
        v.visit_statement(stmt); 
        v 
    }

    fn terminator(term: &Terminator) -> Self { 
        let mut v = Self::default(); 
        v.visit_terminator(term); 
        v 
    
    }
    fn rvalue(rvalue: &Rvalue) -> Self { 
        let mut v = Self::default(); 
        v.visit_rvalue(rvalue); 
        v 
    }
}

struct BlockLiveness {
    before: Vec<HashSet<LivePlace>>,
    after: Vec<HashSet<LivePlace>>,
    term_before: HashSet<LivePlace>,
}

pub(crate) struct LoanChecker<'a> {
    mir: &'a MIRFunction,
    loans: Vec<Loan>,
    sites: Vec<Vec<Option<LoanID>>>,
    call_sites: Vec<Option<LoanID>>,
    parameter_sites: Vec<(LocalID, LoanID)>,

    pointer_inputs: Vec<HashSet<LocalID>>,
    address_taken: Vec<HashSet<LocalID>>,
}

impl<'a> LoanChecker<'a> {

    pub(crate) fn new(mir: &'a MIRFunction) -> Self {
        let mut loans = Vec::new();
        let mut parameter_sites = Vec::new();

        // Safe-reference parameters start with external provenance.
        for index in 1..=mir.arg_count {
            let ty = &mir.locals[index].ty;

            if !matches!(ty, Type::REF(_) | Type::CONST_REF(_)) { 
                continue; 
            }

            let id = LoanID(loans.len());

            loans.push(Loan {
                borrower: LocalID(index),
                mutable: matches!(ty, Type::REF(_)),
                span: mir.locals[index].debug_span.unwrap_or_default(),
                ambient: true,
            });

            parameter_sites.push((LocalID(index), id));
        }

        let sites = mir.basic_blocks.iter().map(|block| {
            block.statements.iter().map(|stmt| {
                let StatementKind::Assign(dest, value) = &stmt.kind else { 
                    return None; 
                };

                let mutable = match value {
                    Rvalue::Ref(mutable, _) => *mutable,
                    Rvalue::SliceRef { is_mut, .. } => *is_mut,
                    _ => return None,
                };

                let id = LoanID(loans.len());

                loans.push(Loan { 
                    borrower: dest.local, 
                    mutable, 
                    span: stmt.span, 
                    ambient: false 
                });

                Some(id)
            }).collect()
        }).collect();

        let call_sites = mir.basic_blocks.iter().map(|block| {
            let Terminator::Call { destination, .. } = &block.terminator else { 
                return None; 
            };

            let ty = place_type(mir, destination)?;

            let mutable = match ty {
                Type::REF(_) => true,
                Type::CONST_REF(_) => false,
                _ => return None,
            };

            let id = LoanID(loans.len());

            // MIR call terminators currently have no independent source span.
            let span = block.statements.last().map(|stmt| stmt.span).unwrap_or_default();
            loans.push(Loan { 
                borrower: destination.local, 
                mutable, 
                span, 
                ambient: false 
            });

            Some(id)
        }).collect();

        let (pointer_inputs, address_taken) = Self::pointer_dependencies(mir);

        Self {
            mir, 
            loans, 
            sites, 
            call_sites, 
            parameter_sites,
            pointer_inputs, 
            address_taken
        }
    }

    pub(crate) fn check(&self, errors: &mut Vec<HydraError>) {
        if self.mir.basic_blocks.is_empty() {
            return;
        }

        let liveness = self.compute_liveness();
        let incoming = self.compute_reaching_loans();

        for (bb, block) in self.mir.basic_blocks.iter().enumerate() {
            let Some(mut state) = incoming[bb].clone() else { continue };

            for (index, stmt) in block.statements.iter().enumerate() {
                let before_state = state.clone();
                let region_before = before_state.active(&liveness[bb].before[index]);

                self.transfer_statement(&mut state, bb, index, stmt);
                let region_after = state.active(&liveness[bb].after[index]);

                self.check_accesses(
                    &before_state, &region_before, &region_after,
                    self.sites[bb][index], &Accesses::statement(stmt).0,
                    stmt.span, errors,
                );
            }

            let span = block.statements.last().map(|s| s.span).unwrap_or_default();
            let region = state.active(&liveness[bb].term_before);

            self.check_accesses(
                &state, &region, &HashSet::new(),
                None, &Accesses::terminator(&block.terminator).0,
                span, errors,
            );

            if matches!(&block.terminator, Terminator::Return) {
                self.check_return_escape(&state, span, errors);
            }
        }
    }

    fn resolve(&self, state: &LoanState, place: &Place) -> Vec<Origin> {
        let mut current = vec![Origin {
            place: Place { local: place.local, projection: Vec::new() },
            via: Vec::new(),
            external: false,
        }];

        for projection in &place.projection {
            if !matches!(projection, ProjectionElem::Deref) {
                for target in &mut current { 
                    target.place.projection.push(projection.clone()); 
                }

                continue;
            }

            let mut next = Vec::new();

            for target in current {
                let mut resolved_here = Vec::new();

                for slot in state.read(&target.place) {
                    for id in slot.loans {
                        for origin in &state.origins[id.0] {
                            let mut via = target.via.clone();

                            for ancestor in origin.via.iter().copied().chain(std::iter::once(id)) {
                                if !via.contains(&ancestor) { 
                                    via.push(ancestor); 
                                }
                            }

                            let resolved = Origin {
                                place: origin.place.clone(),
                                via,
                                external: origin.external,
                            };

                            if !resolved_here.contains(&resolved) { 
                                resolved_here.push(resolved); 
                            }
                        }
                    }
                }
                if resolved_here.is_empty() {
                    // unknown references (e.g. function parameters): stay conservative
                    let mut unknown = target;
                    unknown.place.projection.push(ProjectionElem::Deref);

                    resolved_here.push(unknown);
                }

                for resolved in resolved_here {
                    if !next.contains(&resolved) { 
                        next.push(resolved); 
                    }
                }
            }

            current = next;
        }

        current
    }

    fn pointer_dependencies(mir: &MIRFunction) -> (Vec<HashSet<LocalID>>, Vec<HashSet<LocalID>>) {
        let mut inputs = vec![HashSet::new(); mir.locals.len()];
        let mut addresses = vec![HashSet::new(); mir.locals.len()];

        for block in &mir.basic_blocks {
            for stmt in &block.statements {
                let StatementKind::Assign(dest, rhs) = &stmt.kind else { 
                    continue; 
                };

                if !dest.projection.is_empty() { 
                    continue; 
                }

                for (place, context) in Accesses::rvalue(rhs).0 {
                    if matches!(context, PlaceContext::Read | PlaceContext::Move | PlaceContext::SharedBorrow | PlaceContext::MutBorrow)
                    {
                        inputs[dest.local.0].insert(place.local);
                    }
                }

                if let Rvalue::Ref(_, target) | Rvalue::SliceRef { place: target, .. } = rhs {
                    let base_is_ref = matches!(mir.locals[target.local.0].ty, Type::REF(_) | Type::CONST_REF(_));
                    let has_deref = target.projection.iter().any(|p| matches!(p, ProjectionElem::Deref));

                    // &local and &local.field take the address of owned storage.
                    // &reference.field instead addresses the reference's referent.
                    if !has_deref && !(base_is_ref && !target.projection.is_empty()) {
                        addresses[dest.local.0].insert(target.local);
                    }
                }
            }

            if let Terminator::Call { args, destination, .. } = &block.terminator {
                if destination.projection.is_empty() {
                    for arg in args {
                        if let Operand::Copy(place) | Operand::Move(place) = arg {
                            inputs[destination.local.0].insert(place.local);
                        }
                    }
                }
            }
        }

        (inputs, addresses)
    }

    fn raw_pointer_may_refer_to_local(&self, pointer: LocalID, local: LocalID) -> bool {
        let mut pending = vec![pointer];
        let mut visited = HashSet::new();

        while let Some(current) = pending.pop() {
            if !visited.insert(current) { continue; }

            if self.address_taken[current.0].contains(&local) {
                return true;
            }

            pending.extend(self.pointer_inputs[current.0].iter().copied());
        }

        false
    }

    fn reference_parameter_referent(&self, place: &Place) -> Option<(LocalID, bool)> {
        let index = place.local.0;
        if index == 0 || index > self.mir.arg_count || place.projection.is_empty() {
            return None;
        }

        match &self.mir.locals[index].ty {
            Type::REF(_) => Some((place.local, true)),
            Type::CONST_REF(_) => Some((place.local, false)),
            _ => None,
        }
    }

    fn has_deref(place: &Place) -> bool {
        place.projection.iter().any(|p| matches!(p, ProjectionElem::Deref))
    }

    fn direct_local_storage(&self, place: &Place) -> bool {
        !Self::has_deref(place) && self.reference_parameter_referent(place).is_none()
    }

    fn raw_dereference(&self, place: &Place) -> bool {
        let Some(local) = self.mir.locals.get(place.local.0) else { return false; };
        let mut ty = local.ty.clone();

        for projection in &place.projection {
            ty = match projection {
                ProjectionElem::Deref => match ty {
                    Type::POINTER(_) | Type::CONST_POINTER(_) => return true,
                    Type::REF(inner) | Type::CONST_REF(inner) => *inner,
                    _ => return false,
                },

                ProjectionElem::Field { ty, .. } => ty.clone(),

                ProjectionElem::Index(_) => match ty {
                    Type::ARRAY(inner, _) | Type::INFERRED_ARRAY(inner) | Type::SLICE(inner) => *inner,
                    _ => return false,
                },
            };
        }

        false
    }

    fn reference_value(&self, state: &LoanState, source: &Place) -> Vec<Slot> {
        let mut values: Vec<Slot> = Vec::new();

        for origin in self.resolve(state, source) {
            for slot in state.read(&origin.place) {
                if let Some(existing) = values.iter_mut().find(|s| s.fields == slot.fields) {
                    existing.loans.extend(slot.loans);
                } else { 
                    values.push(slot); 
                }
            }
        }

        values
    }

    fn check_accesses(
        &self,
        state: &LoanState,
        before: &HashSet<LoanID>,
        after: &HashSet<LoanID>,
        new_loan: Option<LoanID>,
        accesses: &[(Place, PlaceContext)],
        span: Span,
        errors: &mut Vec<HydraError>,
    )
    {
        let mut active: Vec<_> = before.iter().copied().collect();
        active.sort_by_key(|id| id.0);

        for (place, context) in accesses {
            if self.mir.locals[place.local.0].debug_name.as_deref() == Some("$hydra_drop_flag") {
                continue;
            }

            if matches!(context, PlaceContext::SharedBorrow | PlaceContext::MutBorrow) && new_loan.is_some_and(|id| !after.contains(&id)) {
                continue;
            }

            for access in self.resolve(state, place) {
                for id in &active {
                    let loan = &self.loans[id.0];

                    if loan.ambient { 
                        continue; 
                    }

                    let direct_local = !access.place.projection.iter().any(|p| matches!(p, ProjectionElem::Deref));
                    if !state.origins[id.0].iter().any(|origin| !(origin.external && direct_local) && self.may_overlap(&access.place, &origin.place)) 
                    {
                        continue;
                    }

                    // a reference can access its own referent; reborrows may access their ancestors
                    if access.via.contains(id) 
                        && (!matches!(context, PlaceContext::Write | PlaceContext::Move | PlaceContext::Drop | PlaceContext::MutBorrow) || loan.mutable) 
                    { 
                        continue; 
                    }

                    let name = self.mir.locals.get(place.local.0).and_then(|decl| decl.debug_name.as_deref()).unwrap_or("value");

                    let kind = if loan.mutable { "mutably" } else { "immutably" };

                    let (code, message) = match context {
                        PlaceContext::Read if loan.mutable => ("BC002", format!("cannot use `{name}` because it is currently mutably borrowed")),

                        PlaceContext::Write => ("BC001", format!("cannot assign to `{name}` because it is currently {kind} borrowed")),

                        PlaceContext::Move => ("BC001", format!("cannot move `{name}` because it is currently {kind} borrowed")),

                        PlaceContext::Drop => ("BC001", format!("cannot drop `{name}` because it is currently {kind} borrowed")),

                        PlaceContext::MutBorrow if loan.mutable => ("BC004", format!("cannot borrow `{name}` as mutable more than once")),

                        PlaceContext::MutBorrow => ("BC004", format!("cannot borrow `{name}` as mutable because it is immutably borrowed")),

                        PlaceContext::SharedBorrow if loan.mutable => ("BC004", format!("cannot borrow `{name}` as immutable because it is mutably borrowed")),

                        _ => continue,
                    };

                    errors.push(HydraError::new(code, message, span).with_note("borrow created here", loan.span));

                    break;
                }
            }
        }
    }

    fn compute_reaching_loans(&self) -> Vec<Option<LoanState>> {
        let mut incoming = vec![None; self.mir.basic_blocks.len()];
        let mut pending = VecDeque::from([0usize]);

        let mut entry = LoanState::empty(self.mir.locals.len(), self.loans.len());

        for (local, id) in &self.parameter_sites {
            let source = Place { local: *local, projection: Vec::new() };
            let referent = Place { local: *local, projection: vec![ProjectionElem::Deref] };

            entry.origins[id.0].push(Origin {
                place: referent,
                via: Vec::new(),
                external: true,
            });

            entry.write(&source, vec![Slot {
                fields: Vec::new(),
                loans: HashSet::from([*id]),
            }]);
        }

        incoming[0] = Some(entry);

        while let Some(bb) = pending.pop_front() {
            let Some(mut state) = incoming[bb].clone() else { 
                continue; 
            };

            let block = &self.mir.basic_blocks[bb];

            for (index, stmt) in block.statements.iter().enumerate() {
                self.transfer_statement(&mut state, bb, index, stmt);
            }

            self.transfer_terminator(&mut state, bb, &block.terminator);

            for next in successors(&block.terminator) {
                let changed = match &mut incoming[next] {
                    Some(existing) => existing.merge(&state),
                    slot @ None => { *slot = Some(state.clone()); true },
                };

                if changed { 
                    pending.push_back(next); 
                }
            }
        }

        incoming
    }

    fn returned_origins(&self, state: &LoanState, args: &[Operand]) -> Vec<Origin> {
        let mut results = Vec::new();

        for arg in args {
            let (Operand::Copy(place) | Operand::Move(place)) = arg else { 
                continue; 
            };

            if !matches!(place_type(self.mir, place), Some(Type::REF(_) | Type::CONST_REF(_))) {
                continue;
            }

            for source in self.resolve(state, place) {
                let mut inherited = false;

                for slot in state.read(&source.place) {
                    if !slot.fields.is_empty() { continue; }

                    for id in slot.loans {
                        for origin in &state.origins[id.0] {
                            inherited = true;
                            let mut via = source.via.clone();

                            for ancestor in origin.via.iter().copied().chain(std::iter::once(id)) {
                                if !via.contains(&ancestor) { via.push(ancestor); }
                            }

                            let candidate = Origin {
                                place: origin.place.clone(),
                                via,
                                external: origin.external,
                            };
                            if !results.contains(&candidate) { results.push(candidate); }
                        }
                    }
                }

                if !inherited {
                    // An unmodelled reference value: preserve its indirect
                    // referent rather than claiming its slot is the referent
                    let mut referent = source.place;
                    referent.projection.push(ProjectionElem::Deref);

                    let candidate = Origin {
                        place: referent,
                        via: source.via,
                        external: source.external,
                    };
                    if !results.contains(&candidate) { results.push(candidate); }
                }
            }
        }

        results
    }

    fn transfer_statement(&self, state: &mut LoanState, bb: usize, index: usize, stmt: &Statement) {
        match &stmt.kind {
            StatementKind::Assign(dest, value) => {
                let assigned = if let Some(id) = self.sites[bb][index] {
                    let target = match value {
                        Rvalue::Ref(_, target) | Rvalue::SliceRef { place: target, .. } => target,
                        _ => unreachable!(),
                    };

                    for origin in self.resolve(state, target) {
                        if !state.origins[id.0].contains(&origin) { 
                            state.origins[id.0].push(origin); 
                        }
                    }

                    vec![Slot { fields: Vec::new(), loans: HashSet::from([id]) }]
                } else {
                    match value {
                        Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) => self.reference_value(state, src),

                        Rvalue::Aggregate(_, operands) => operands.iter().enumerate().flat_map(|(i, operand)| {
                            let (Operand::Copy(src) | Operand::Move(src)) = operand else { 
                                return Vec::new(); 
                            };

                            self.reference_value(state, src).into_iter().map(|mut slot| {
                                slot.fields.insert(0, i); slot
                            }).collect::<Vec<_>>()

                        }).collect(),

                        _ => Vec::new(),
                    }
                };

                self.kill_moves(state, &Accesses::rvalue(value).0);
                self.write_destination(state, dest, assigned);
            }

            StatementKind::Drop { place, .. } => {
                let targets = self.resolve(state, place);
                if targets.len() == 1 { state.remove(&targets[0].place); }
            },
        }
    }

    fn write_destination(&self, state: &mut LoanState, dest: &Place, values: Vec<Slot>) {
        if !dest.projection.iter().any(|p| matches!(p, ProjectionElem::Deref)) {
            state.write(dest, values);
            return;
        }

        let targets = self.resolve(state, dest);

        if targets.len() == 1 && field_path(&targets[0].place).is_some() {
            state.write(&targets[0].place, values);
        } else {
            for target in targets { state.weak_write(&target.place, &values); }
        }
    }

    fn transfer_terminator(&self, state: &mut LoanState, bb: usize, term: &Terminator) {
        // Capture return origins before arguments are consumed by moves.
        let returned = match term {
            Terminator::Call { args, .. } if self.call_sites[bb].is_some() =>
            self.returned_origins(state, args),
            _ => Vec::new(),
        };

        self.kill_moves(state, &Accesses::terminator(term).0);

        if let Terminator::Call { destination, .. } = term {
            let values = if let Some(id) = self.call_sites[bb] {
                if returned.is_empty() {
                    // A reference-returning function without safe-reference
                    // inputs may return a global/static reference.
                    let mut referent = destination.clone();
                    referent.projection.push(ProjectionElem::Deref);

                    let external = Origin {
                        place: referent,
                        via: Vec::new(),
                        external: true,
                    };
                    if !state.origins[id.0].contains(&external) {
                        state.origins[id.0].push(external);
                    }
                }

                for origin in returned {
                    if !state.origins[id.0].contains(&origin) {
                        state.origins[id.0].push(origin);
                    }
                }

                vec![Slot { fields: Vec::new(), loans: HashSet::from([id]) }]
            } else {
                Vec::new()
            };

            self.write_destination(state, destination, values);
        }
    }

    fn kill_moves(&self, state: &mut LoanState, accesses: &[(Place, PlaceContext)]) {
        for (place, context) in accesses {
            if *context == PlaceContext::Move {
                let targets = self.resolve(state, place);
                if targets.len() == 1 { state.remove(&targets[0].place); }
            }
        }
    }

    fn compute_liveness(&self) -> Vec<BlockLiveness> {
        let n = self.mir.basic_blocks.len();
        let mut live_in = vec![HashSet::new(); n];
        let mut live_out = vec![HashSet::new(); n];

        loop {
            let mut changed = false;

            for i in (0..n).rev() {
                let block = &self.mir.basic_blocks[i];
                let out: HashSet<_> = successors(&block.terminator)
                    .into_iter()
                    .flat_map(|next| live_in[next].iter().cloned())
                    .collect();

                let mut inside = out.clone();

                Self::transfer_liveness(&mut inside, &Accesses::terminator(&block.terminator).0);

                for stmt in block.statements.iter().rev() {
                    Self::transfer_liveness(&mut inside, &Accesses::statement(stmt).0);
                }

                changed |= live_in[i] != inside || live_out[i] != out;

                live_in[i] = inside;
                live_out[i] = out;
            }

            if !changed { 
                break; 
            }
        }

        self.mir.basic_blocks.iter().enumerate().map(|(i, block)| {
            let mut live = live_out[i].clone();

            Self::transfer_liveness(&mut live, &Accesses::terminator(&block.terminator).0);

            let term_before = live.clone();
            let mut before = vec![HashSet::new(); block.statements.len()];
            let mut after = before.clone();

            for index in (0..block.statements.len()).rev() {
                after[index] = live.clone();

                Self::transfer_liveness(&mut live, &Accesses::statement(&block.statements[index]).0);

                before[index] = live.clone();
            }

            BlockLiveness { 
                before, 
                after, 
                term_before 
            }
        }).collect()
    }

    fn transfer_liveness(live: &mut HashSet<LivePlace>, accesses: &[(Place, PlaceContext)]) {
        let mut reads = HashSet::new();
        let mut kills = Vec::new();

        for (place, context) in accesses {
            match context {
                PlaceContext::Write | PlaceContext::Drop if field_path(place).is_some() => {
                    kills.push(LivePlace::new(place));

                    if *context == PlaceContext::Drop {
                        reads.insert(LivePlace::new(place));
                    }
                }
                _ => {
                    reads.insert(LivePlace::new(place));
                }
            }
        }

        live.retain(|place| !kills.iter().any(|kill| {
            kill.local == place.local && place.fields.starts_with(&kill.fields)
        }));

        live.extend(reads);
    }

    fn check_return_escape(&self, state: &LoanState, span: Span, errors: &mut Vec<HydraError>) {
        let mut reported = HashSet::new();

        for slot in &state.slots[0] {
            for id in &slot.loans {
                if !reported.insert(*id) {
                    continue;
                }

                let loan = &self.loans[id.0];

                if state.origins[id.0].iter().any(|origin| !origin.external && !self.external_origin(&origin.place)) {
                    errors.push(
                        HydraError::new(
                            "BC005",
                            "cannot return a reference to storage owned by this function",
                            span,
                        ).with_note("reference created here", loan.span)
                    );
                }
            }
        }
    }

    fn safe_reference_slot_is_disjoint(&self, a: &Place, b: &Place) -> bool {
        if a.local != b.local { 
            return false; 
        }

        let (shorter, longer) = if a.projection.len() <= b.projection.len() {
            (a, b)
        } else {
            (b, a)
        };

        if !longer.projection.starts_with(&shorter.projection) || !matches!(longer.projection.get(shorter.projection.len()), Some(ProjectionElem::Deref))
        {
            return false;
        }

        let ty = match shorter.projection.last() {
            Some(ProjectionElem::Field { ty, .. }) => Some(ty),
            None => self.mir.locals.get(shorter.local.0).map(|local| &local.ty),

            _ => None,
        };

        matches!(ty, Some(Type::REF(_) | Type::CONST_REF(_)))
    }

    fn may_overlap(&self, access: &Place, origin: &Place) -> bool {
        if access.local == origin.local {
            return !self.safe_reference_slot_is_disjoint(access, origin)
            && places_overlap(access, origin);
        }

        // Distinct reference parameters are disjoint if either one is exclusive.
        // Otherwise the caller could not have supplied both valid references.
        if let (Some((a, a_mut)), Some((b, b_mut))) = (
            self.reference_parameter_referent(access),
            self.reference_parameter_referent(origin),
        ) {
            if a != b && (a_mut || b_mut) { return false; }
        }

        // An opaque raw pointer is not assumed to address every stack local.
        // Retain conflicts when its construction depends on that local's address.
        if self.raw_dereference(origin) && self.direct_local_storage(access) {
            return self.raw_pointer_may_refer_to_local(origin.local, access.local);
        }

        if self.raw_dereference(access) && self.direct_local_storage(origin) {
            return self.raw_pointer_may_refer_to_local(access.local, origin.local);
        }

        if !Self::has_deref(access) && !Self::has_deref(origin) {
            return false;
        }

        if self.external_origin(access) && !Self::has_deref(origin) {
            return false;
        }
        if self.external_origin(origin) && !Self::has_deref(access) {
            return false;
        }

        // unknown indirect accesses may still alias
        true
    }

    fn external_origin(&self, place: &Place) -> bool {
        let index = place.local.0;

        index > 0 && index <= self.mir.arg_count
        && matches!(&self.mir.locals[index].ty, Type::REF(_) | Type::CONST_REF(_))
        && matches!(place.projection.first(), Some(ProjectionElem::Deref))
    }

    fn overlaps_origin(&self, access: &Place, origin: &Place) -> bool {
        let accesses_indirect_storage = access.projection.iter()
            .any(|p| matches!(p, ProjectionElem::Deref));

        // storage reached through an incoming borrowed parameter cannot alias
        // this function's own local storage 
        //
        // the reference variable and its
        // pointee are also distinct storage locations
        if !accesses_indirect_storage && self.external_origin(origin) {
            return false;
        }

        places_overlap(access, origin)
    }
}

fn place_type(mir: &MIRFunction, place: &Place) -> Option<Type> {
    let mut ty = mir.locals.get(place.local.0)?.ty.clone();

    for projection in &place.projection {
        ty = match projection {
            ProjectionElem::Field { ty, .. } => ty.clone(),

            ProjectionElem::Deref => match ty {
                Type::REF(inner) | Type::CONST_REF(inner)
                | Type::POINTER(inner) | Type::CONST_POINTER(inner) => *inner,
                _ => return None,
            },

            ProjectionElem::Index(_) => match ty {
                Type::ARRAY(inner, _) | Type::INFERRED_ARRAY(inner)
                | Type::SLICE(inner) => *inner,
                _ => return None,
            },
        };
    }

    Some(ty)
}

fn successors(term: &Terminator) -> Vec<usize> {
    match term {
        Terminator::Goto { target } | Terminator::Call { target, .. } => vec![target.0],

        Terminator::SwitchInt { true_target, false_target, .. } => vec![true_target.0, false_target.0],

        Terminator::Return | Terminator::Unreachable => Vec::new(),
    }
}
