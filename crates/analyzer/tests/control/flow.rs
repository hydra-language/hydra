use std::{fs, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

use analyzer::{Analyzer, Resolver};
use ir::{
    context::{DefID, SemanticContext},
    hir::{HIRBinOp, HIRExpr, HIRExprKind, HIRProgram, HIRStmt},
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct Project {
    root: PathBuf,
    entry: PathBuf,
}

impl Project {
    fn new(source: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "hydra-control-flow-{}-{id}", std::process::id()
        ));
        fs::create_dir_all(&root).expect("create test directory");
        let entry = root.join("main.hydra");
        fs::write(&entry, source).expect("write source");
        Self { root, entry }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn analyze(source: &str) -> HIRProgram {
    let project = Project::new(source);
    let mut source_map = SourceMap::new();
    let mut modules = ModuleTree::build(&project.entry, project.root.clone(), &mut source_map)
        .expect("module tree should build");
    modules.resolve_imports(&mut source_map).expect("imports should resolve");
    modules.parse_bodies(&source_map).expect("bodies should parse");

    let mut context = SemanticContext::new();
    let (resolver, symbols) = Resolver::new(&modules, &mut context, &source_map)
        .resolve()
        .expect("name resolution should succeed");

    Analyzer::new(&modules, context, &source_map, resolver, symbols)
        .analyze()
        .expect("semantic analysis should succeed")
        .hir
}

fn main_body(program: &HIRProgram) -> &[HIRStmt] {
    &program.functions.iter()
        .find(|function| function.name == "main")
        .expect("main should exist")
        .body.stmts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForLowering {
    start: i64,
    end: i64,
    direction_op: HIRBinOp,
    descending_break_op: HIRBinOp,
    ascending_break_op: HIRBinOp,
    descending_step_op: HIRBinOp,
    descending_step: i64,
    ascending_step_op: HIRBinOp,
    ascending_step: i64,
}

fn expect_var_ref(expr: &HIRExpr, expected: DefID) {
    assert!(
        matches!(&expr.kind, HIRExprKind::VarRef(id) if *id == expected),
        "expected variable reference {expected:?}, found {:#?}",
        expr.kind,
    );
}

fn expect_expr(stmt: &HIRStmt) -> &HIRExpr {
    let HIRStmt::Expr(expr) = stmt else {
        panic!("expected expression statement, found {stmt:#?}");
    };
    expr
}

fn expect_int(expr: &HIRExpr) -> i64 {
    let HIRExprKind::IntLiteral(value) = &expr.kind else {
        panic!("expected integer literal, found {:#?}", expr.kind);
    };
    *value
}

fn expect_decl(stmt: &HIRStmt) -> (DefID, &HIRExpr) {
    let HIRStmt::VarDecl { local, init: Some(init), .. } = stmt else {
        panic!("expected initialized local declaration, found {stmt:#?}");
    };
    (local.def_id, init)
}

fn break_operator(stmt: &HIRStmt, loop_id: DefID, end_id: DefID) -> HIRBinOp {
    let expr = expect_expr(stmt);
    let HIRExprKind::If { cond, then_block, else_block } = &expr.kind else {
        panic!("expected break-check if, found {:#?}", expr.kind);
    };

    assert!(else_block.is_none(), "break check should not have else");
    assert_eq!(then_block.stmts.len(), 1);
    assert!(matches!(expect_expr(&then_block.stmts[0]).kind, HIRExprKind::Break));

    let HIRExprKind::Binary { op, lhs, rhs } = &cond.kind else {
        panic!("expected binary break condition");
    };
    expect_var_ref(lhs, loop_id);
    expect_var_ref(rhs, end_id);
    *op
}

fn step_operator(stmt: &HIRStmt, loop_id: DefID) -> (HIRBinOp, i64) {
    let expr = expect_expr(stmt);
    let HIRExprKind::Assign { target, value } = &expr.kind else {
        panic!("expected loop-step assignment, found {:#?}", expr.kind);
    };
    expect_var_ref(target, loop_id);

    let HIRExprKind::Binary { op, lhs, rhs } = &value.kind else {
        panic!("expected binary step expression");
    };
    expect_var_ref(lhs, loop_id);
    (*op, expect_int(rhs))
}

fn extract_for_lowering(program: &HIRProgram) -> ForLowering {
    let body = main_body(program);
    assert_eq!(body.len(), 1, "expected one top-level for expression");

    let HIRExprKind::Block(block) = &expect_expr(&body[0]).kind else {
        panic!("for should lower to a block");
    };
    assert_eq!(block.stmts.len(), 4, "for should initialize three locals and a loop");

    let (loop_id, start_expr) = expect_decl(&block.stmts[0]);
    let (end_id, end_expr) = expect_decl(&block.stmts[1]);
    let (direction_id, direction_expr) = expect_decl(&block.stmts[2]);

    let HIRExprKind::Binary { op: direction_op, lhs, rhs } = &direction_expr.kind else {
        panic!("expected direction comparison");
    };
    expect_var_ref(lhs, loop_id);
    expect_var_ref(rhs, end_id);

    let HIRExprKind::Loop(loop_body) = &expect_expr(&block.stmts[3]).kind else {
        panic!("expected HIR loop");
    };
    assert!(loop_body.stmts.len() >= 2, "loop needs bounds checks and a step");

    let HIRExprKind::If {
        cond: range_direction,
        then_block: descending,
        else_block: Some(ascending),
    } = &expect_expr(&loop_body.stmts[0]).kind else {
        panic!("expected direction-dependent bounds check");
    };

    expect_var_ref(range_direction, direction_id);
    assert_eq!(descending.stmts.len(), 1);
    assert_eq!(ascending.stmts.len(), 1);

    let descending_break_op = break_operator(&descending.stmts[0], loop_id, end_id);
    let ascending_break_op = break_operator(&ascending.stmts[0], loop_id, end_id);

    let HIRExprKind::If {
        cond: step_direction,
        then_block: descending_step,
        else_block: Some(ascending_step),
    } = &expect_expr(loop_body.stmts.last().expect("expected loop step")).kind else {
        panic!("expected direction-dependent loop step");
    };

    expect_var_ref(step_direction, direction_id);
    assert_eq!(descending_step.stmts.len(), 1);
    assert_eq!(ascending_step.stmts.len(), 1);

    let (descending_step_op, descending_step) =
        step_operator(&descending_step.stmts[0], loop_id);
    let (ascending_step_op, ascending_step) =
        step_operator(&ascending_step.stmts[0], loop_id);

    ForLowering {
        start: expect_int(start_expr),
        end: expect_int(end_expr),
        direction_op: *direction_op,
        descending_break_op,
        ascending_break_op,
        descending_step_op,
        descending_step,
        ascending_step_op,
        ascending_step,
    }
}

fn expected_range(start: i64, end: i64, inclusive: bool) -> ForLowering {
    ForLowering {
        start,
        end,
        direction_op: HIRBinOp::Gt,
        descending_break_op: if inclusive { HIRBinOp::Lt } else { HIRBinOp::Le },
        ascending_break_op: if inclusive { HIRBinOp::Gt } else { HIRBinOp::Ge },
        descending_step_op: HIRBinOp::Sub,
        descending_step: 1,
        ascending_step_op: HIRBinOp::Add,
        ascending_step: 1,
    }
}

#[test]
fn lowers_ascending_exclusive_range() {
    let hir = analyze(r#"
        fn main() -> void {
            for i in 0..5 { i; }
        }
    "#);
    assert_eq!(extract_for_lowering(&hir), expected_range(0, 5, false));
}

#[test]
fn lowers_ascending_inclusive_range() {
    let hir = analyze(r#"
        fn main() -> void {
            for i in 0..=5 { i; }
        }
    "#);
    assert_eq!(extract_for_lowering(&hir), expected_range(0, 5, true));
}

#[test]
fn lowers_descending_exclusive_range() {
    let hir = analyze(r#"
        fn main() -> void {
            for i in 5..0 { i; }
        }
    "#);
    assert_eq!(extract_for_lowering(&hir), expected_range(5, 0, false));
}

#[test]
fn lowers_descending_inclusive_range() {
    let hir = analyze(r#"
        fn main() -> void {
            for i in 5..=0 { i; }
        }
    "#);
    assert_eq!(extract_for_lowering(&hir), expected_range(5, 0, true));
}

#[test]
fn analyzes_empty_while_body() {
    let hir = analyze(r#"
        fn main() -> void {
            while true {}
        }
    "#);

    let body = main_body(&hir);
    assert_eq!(body.len(), 1);
    assert!(
        matches!(expect_expr(&body[0]).kind, HIRExprKind::Loop(_)),
        "while should lower to an HIR loop",
    );
}

#[test]
fn analyzes_empty_for_body() {
    let hir = analyze(r#"
        fn main() -> void {
            for i in 0..5 {}
        }
    "#);

    assert_eq!(extract_for_lowering(&hir), expected_range(0, 5, false));
}
