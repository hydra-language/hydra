use std::collections::{HashSet, VecDeque};

use errors::error::{HydraError, Span};
use ir::types::Type;
use mir::{LocalID, MIRFunction, Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator};
use mir::visit::{self, PlaceContext, Visitor};

// a move path is a local plus its statically known field projections
//
// dynamic indexing and dereferencing do not designate owned subobjects in this analysis
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Path { 
    local: LocalID, 
    fields: Vec<usize> 
}

impl Path {

    fn root(local: LocalID) -> Self { Self { local, fields: Vec::new() } }

    fn access(place: &Place) -> Option<Self> {
        let mut fields = Vec::new();

        for projection in &place.projection {
            match projection {
                ProjectionElem::Field { index, .. } => fields.push(*index),
                ProjectionElem::Index(_) => return Some(Self { local: place.local, fields }),
                ProjectionElem::Deref => return None,
            }
        }

        Some(Self { local: place.local, fields })
    }

    fn of(place: &Place) -> Option<Self> {
        let fields = place.projection.iter().map(|p| match p {
            ProjectionElem::Field { index, .. } => Some(*index),
            ProjectionElem::Deref | ProjectionElem::Index(_) => None,
        }).collect::<Option<Vec<_>>>()?;

        Some(Self { 
            local: place.local, 
            fields 
        })
    }

    fn contains(&self, other: &Self) -> bool {
        self.local == other.local && other.fields.starts_with(&self.fields)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InitState { 
    assigned: HashSet<Path>, 
    invalid: HashSet<Path> 
}

impl InitState {

    fn entry(args: usize) -> Self {
        Self {
            assigned: (1..=args).map(|i| Path::root(LocalID(i))).collect(),
            invalid: HashSet::new(),
        }
    }

    fn merge(&mut self, other: &Self) -> bool {
        let old = self.clone();

        self.assigned.retain(|p| other.assigned.contains(p));
        self.invalid.extend(other.invalid.iter().cloned());

        *self != old
    }

    fn available(&self, path: &Path) -> bool {
        let assigned_depth = self.assigned.iter().filter(|p| p.contains(path)).map(|p| p.fields.len()).max();
        let invalid_depth = self.invalid.iter().filter(|p| p.contains(path)).map(|p| p.fields.len()).max();

        let Some(depth) = assigned_depth else { 
            return false; 
        };

        if invalid_depth.is_some_and(|invalid| invalid >= depth) { 
            return false; 
        }

        // a whole aggregate cannot be used while any part remains moved
        !self.invalid.iter().any(|missing| path.contains(missing) &&
            !self.assigned.iter().any(|assigned| missing.contains(assigned) && assigned.fields.len() > missing.fields.len()))
    }

    fn write(&mut self, path: Path) {
        self.assigned.retain(|p| !path.contains(p));
        self.invalid.retain(|p| !path.contains(p));

        self.assigned.insert(path);
    }

    fn move_out(&mut self, path: Path) {
        self.assigned.retain(|p| !path.contains(p));
        self.invalid.retain(|p| !path.contains(p));

        self.invalid.insert(path);
    }

    fn was_moved(&self, path: &Path) -> bool {
        self.invalid.iter().any(|p| p.contains(path) || path.contains(p))
    }
}

#[derive(Default)]
struct Events(Vec<(Place, PlaceContext)>);

impl Visitor for Events {
    fn visit_place(&mut self, place: &Place, context: PlaceContext) {
        self.0.push((place.clone(), context));
        visit::walk_place(self, place);
    }
}

pub(crate) struct InitChecker<'a> { 
    mir: &'a MIRFunction 
}

impl<'a> InitChecker<'a> {

    pub(crate) fn new(mir: &'a MIRFunction) -> Self { 
        Self { 
            mir 
        } 
    }

    pub(crate) fn check(&self, errors: &mut Vec<HydraError>) {
        let incoming = self.incoming();

        for (bb, block) in self.mir.basic_blocks.iter().enumerate() {
            let Some(mut state) = incoming[bb].clone() else { continue; };

            for statement in &block.statements {
                self.statement(&mut state, statement, Some(&mut *errors));
            }

            let span = block.statements.last().map(|s| s.span).unwrap_or_default();
            self.terminator(&mut state, &block.terminator, span, Some(&mut *errors));
        }
    }

    fn incoming(&self) -> Vec<Option<InitState>> {
        let n = self.mir.basic_blocks.len();

        let mut states = vec![None; n];

        if n == 0 { 
            return states; 
        }

        states[0] = Some(InitState::entry(self.mir.arg_count));
        let mut pending = VecDeque::from([0]);

        while let Some(bb) = pending.pop_front() {
            let Some(mut state) = states[bb].clone() else { continue; };
            let block = &self.mir.basic_blocks[bb];

            for stmt in &block.statements {
                self.statement(&mut state, stmt, None);
            }

            self.terminator(&mut state, &block.terminator, Span::default(), None);

            for next in successors(&block.terminator) {
                let changed = match &mut states[next] {
                    Some(previous) => previous.merge(&state),
                    slot @ None => { *slot = Some(state.clone()); true }
                };

                if changed { pending.push_back(next); }
            }
        }

        states
    }

    fn statement(&self, state: &mut InitState, stmt: &Statement, mut errors: Option<&mut Vec<HydraError>>) {
        match &stmt.kind {
            StatementKind::Assign(dest, rhs) => {
                let mut events = Events::default();
                events.visit_rvalue(rhs);

                self.apply_events(state, &events.0, stmt.span, errors.as_deref_mut());

                // resolve the destination address before the assignment initializes it
                let mut address = Events::default();
                visit::walk_place(&mut address, dest);
                self.apply_events(state, &address.0, stmt.span, errors);

                if let Some(path) = Path::of(dest) { 
                    state.write(path); 
                }
            }

            StatementKind::Drop { place, .. } => {
                // compiler-generated destruction is guarded by MIR drop flags
                // it still consumes the value on the taken edge
                if let Some(path) = Path::of(place) { 
                    state.move_out(path); 
                }
            }
        }
    }

    fn terminator(&self, state: &mut InitState, term: &Terminator, span: Span, mut errors: Option<&mut Vec<HydraError>>) {
        let mut events = Events::default();
        events.visit_terminator(term);

        // destination writes and return-value reads are handled separately
        events.0.retain(|(p, context)| !(*context == PlaceContext::Write && matches!(term, Terminator::Call { .. })) &&
            !(matches!(term, Terminator::Return) && p.local == LocalID(0)));

        self.apply_events(state, &events.0, span, errors.as_deref_mut());

        match term {
            Terminator::Call { destination, .. } => {
                if let Some(path) = Path::of(destination) { state.write(path); }
            }

            Terminator::Return if self.mir.return_type != Type::VOID => {
                let return_place = Path::root(LocalID(0));

                if !state.available(&return_place) {
                    if let Some(out) = errors {
                        out.push(HydraError::new("BC003", "return value is uninitialized or moved", span));
                    }
                }
            }

            _ => {}
        }
    }

    fn apply_events(&self, state: &mut InitState, events: &[(Place, PlaceContext)], span: Span, mut errors: Option<&mut Vec<HydraError>>) {
        for (place, context) in events {
            let Some(path) = Path::access(place) else { continue; };

            if matches!(context, PlaceContext::Read | PlaceContext::Move | PlaceContext::SharedBorrow | PlaceContext::MutBorrow) && !state.available(&path)
            {
                if let Some(out) = errors.as_deref_mut() {
                    let name = self.mir.locals[path.local.0].debug_name.as_deref().unwrap_or("value");

                    let message = if state.was_moved(&path) {
                        format!("use of moved value `{name}`")
                    } else {
                        format!("use of uninitialized value `{name}`")
                    };

                    out.push(HydraError::new("BC003", message, span));
                }
            }

            if *context == PlaceContext::Move { 
                state.move_out(path); 
            }
        }
    }
}

fn successors(term: &Terminator) -> Vec<usize> {
    match term {
        Terminator::Goto { target } | Terminator::Call { target, .. } => vec![target.0],
        Terminator::SwitchInt { true_target, false_target, .. } => vec![true_target.0, false_target.0],
        Terminator::Return | Terminator::Unreachable => Vec::new(),
    }
}
