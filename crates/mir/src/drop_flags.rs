use crate::{
    BasicBlock, BasicBlockID, LocalDecl, LocalID, MIRFunction, Operand, Place,
    ProjectionElem, Rvalue, Statement, StatementKind, Terminator
};
use crate::visit::{self, PlaceContext, Visitor};
use ir::{types::Type, Constant};
use errors::error::Span;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DropPath { 
    local: LocalID, 
    fields: Vec<usize> 
}

impl DropPath {

    fn of(place: &Place) -> Option<Self> {
        let fields = place.projection.iter().map(|p| match p {
            ProjectionElem::Field { index, .. } => Some(*index),
            ProjectionElem::Deref | ProjectionElem::Index(_) => None,
        }).collect::<Option<Vec<_>>>()?;

        Some(Self { local: place.local, fields })
    }

    fn contains(&self, other: &Self) -> bool {
        self.local == other.local && other.fields.starts_with(&self.fields)
    }
}

#[derive(Default)]
struct Moves(Vec<Place>);

impl Visitor for Moves {
    fn visit_place(&mut self, place: &Place, context: PlaceContext) {
        if context == PlaceContext::Move { self.0.push(place.clone()); }
        visit::walk_place(self, place);
    }
}

fn assignment(flag: LocalID, value: bool, span: Span) -> Statement {
    Statement {
        kind: StatementKind::Assign(
            Place { local: flag, projection: Vec::new() },
            Rvalue::Use(Operand::Const(Constant::Bool(value))),
        ),
        span,
    }
}

fn new_block(mir: &mut MIRFunction) -> BasicBlockID {
    let id = BasicBlockID(mir.basic_blocks.len());

    mir.basic_blocks.push(BasicBlock {
        statements: Vec::new(),
        terminator: Terminator::Unreachable,
    });
    id
}

fn moved_flags(place: &Place, flags: &[(DropPath, LocalID)], span: Span) -> Vec<Statement> {
    if let Some(path) = DropPath::of(place) {
        flags.iter().filter(|(tracked, _)| path.contains(tracked) || tracked.contains(&path))
            .map(|(_, flag)| assignment(*flag, false, span)).collect()
    } else if !place.projection.iter().any(|p| matches!(p, ProjectionElem::Deref)) {
        // A dynamic array element may be any element: invalidate the owning root.
        flags.iter().filter(|(tracked, _)| tracked.local == place.local)
            .map(|(_, flag)| assignment(*flag, false, span)).collect()
    } else {
        Vec::new()
    }
}

fn written_flags(place: &Place, flags: &[(DropPath, LocalID)], span: Span) -> Vec<Statement> {
    let Some(path) = DropPath::of(place) else { return Vec::new(); };

    flags.iter().filter(|(tracked, _)| path.contains(tracked))
        .map(|(_, flag)| assignment(*flag, true, span)).collect()
}

/// Elaborate compiler-generated cleanup into runtime-guarded drops. This pass
/// is run exactly once, after MIR construction and before borrow checking.
pub fn elaborate(mir: &mut MIRFunction) {
    let mut paths = HashMap::<DropPath, LocalID>::new();

    for block in &mir.basic_blocks {
        for stmt in &block.statements {
            if let StatementKind::Drop { place, .. } = &stmt.kind {
                if let Some(path) = DropPath::of(place) {
                    paths.entry(path).or_insert(LocalID(usize::MAX));
                }
            }
        }
    }

    if paths.is_empty() { 
        return; 
    }

    let mut flags: Vec<_> = paths.into_keys().collect();
    flags.sort_by(|a, b| (a.local.0, &a.fields).cmp(&(b.local.0, &b.fields)));

    let flags: Vec<_> = flags.into_iter().map(|path| {
        let id = LocalID(mir.locals.len());

        mir.locals.push(LocalDecl {
            ty: Type::BOOL,
            is_mutable: true,
            debug_def_id: None,
            debug_name: Some("$hydra_drop_flag".into()),
            debug_span: None,
        });

        (path, id)
    }).collect();

    let original_blocks = mir.basic_blocks.len();

    for bb in 0..original_blocks {
        let original = std::mem::take(&mut mir.basic_blocks[bb].statements);

        let mut term = std::mem::replace(
            &mut mir.basic_blocks[bb].terminator,
            Terminator::Unreachable,
        );

        let mut current = BasicBlockID(bb);

        if bb == 0 {
            for (path, flag) in &flags {
                mir.basic_blocks[bb].statements.push(assignment(
                    *flag,
                    path.local.0 > 0 && path.local.0 <= mir.arg_count,
                    Span::default(),
                ));
            }
        }

        for stmt in original {
            match &stmt.kind {
                StatementKind::Assign(destination, rhs) => {
                    let mut moves = Moves::default();
                    moves.visit_rvalue(rhs);

                    let updates: Vec<_> = moves.0.iter()
                        .flat_map(|p| moved_flags(p, &flags, stmt.span))
                        .chain(written_flags(destination, &flags, stmt.span))
                        .collect();

                    mir.basic_blocks[current.0].statements.push(stmt);
                    mir.basic_blocks[current.0].statements.extend(updates);
                }

                StatementKind::Drop { place, .. } => {
                    let Some(path) = DropPath::of(place) else {
                        mir.basic_blocks[current.0].statements.push(stmt);
                        continue;
                    };

                    let flag = flags.iter().find(|(p, _)| *p == path).unwrap().1;
                    let do_drop = new_block(mir);
                    let resume = new_block(mir);

                    mir.basic_blocks[current.0].terminator = Terminator::SwitchInt {
                        discriminant: Operand::Copy(Place {
                            local: flag,
                            projection: Vec::new(),
                        }),
                        true_target: do_drop,
                        false_target: resume,
                    };

                    mir.basic_blocks[do_drop.0].statements.push(
                        assignment(flag, false, stmt.span)
                    );
                    mir.basic_blocks[do_drop.0].statements.push(stmt);
                    mir.basic_blocks[do_drop.0].terminator = Terminator::Goto { target: resume };

                    current = resume;
                }
            }
        }

        if let Terminator::Call { args, destination, target, .. } = &mut term {
            let mut moves = Moves::default();
            for arg in args { moves.visit_operand(arg); }

            for place in &moves.0 {
                mir.basic_blocks[current.0].statements.extend(
                    moved_flags(place, &flags, Span::default())
                );
            }

            let written = written_flags(destination, &flags, Span::default());
            if !written.is_empty() {
                let after_call = new_block(mir);
                mir.basic_blocks[after_call.0].statements = written;
                mir.basic_blocks[after_call.0].terminator = Terminator::Goto { target: *target };
                *target = after_call;
            }
        } else if let Terminator::SwitchInt { discriminant, .. } = &term {
            let mut moves = Moves::default();
            moves.visit_operand(discriminant);

            for place in &moves.0 {
                mir.basic_blocks[current.0].statements.extend(
                    moved_flags(place, &flags, Span::default())
                );
            }
        }

        mir.basic_blocks[current.0].terminator = term;
    }
}
