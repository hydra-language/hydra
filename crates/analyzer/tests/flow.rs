use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};

use ir::{
    context::{DefID, HIRContext},
    hir::{
        HIRBinOp,
        HIRExpr,
        HIRExprKind,
        HIRProgram,
        HIRStmt,
    },
};

use parser::module::{
    ModuleTree,
    SourceMap,
};


static NEXT_TEST_ID: AtomicUsize =
    AtomicUsize::new(0);


// ============================================================================
// TEST PROJECT
// ============================================================================

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id =
            NEXT_TEST_ID.fetch_add(
                1,
                Ordering::Relaxed,
            );

        let dir =
            std::env::temp_dir().join(format!(
                "hydrac-analyzer-test-{}-{}",
                std::process::id(),
                id,
            ));

        fs::create_dir_all(&dir)
            .expect(
                "failed to create temporary test directory",
            );

        let entry =
            dir.join("main.hydra");

        fs::write(
            &entry,
            source,
        )
        .expect(
            "failed to write temporary Hydra source",
        );

        Self {
            dir,
            entry,
        }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ =
            fs::remove_dir_all(
                &self.dir,
            );
    }
}


// ============================================================================
// ANALYZER PIPELINE
// ============================================================================

fn analyze(source: &str) -> HIRProgram {
    let project =
        TestProject::new(source);

    let mut source_map =
        SourceMap::new();

    let mut module_tree =
        ModuleTree::build(
            Path::new(
                &project.entry,
            ),
            project.dir.clone(),
            &mut source_map,
        )
        .unwrap_or_else(|errors| {
            panic!(
                "module construction failed:\n{errors:#?}"
            );
        });

    module_tree
        .resolve_imports(
            &mut source_map,
        )
        .unwrap_or_else(|errors| {
            panic!(
                "import resolution failed:\n{errors:#?}"
            );
        });

    module_tree
        .parse_bodies(
            &source_map,
        )
        .unwrap_or_else(|errors| {
            panic!(
                "body parsing failed:\n{errors:#?}"
            );
        });

    let mut context =
        HIRContext::default();

    let resolver =
        Resolver::new(
            &module_tree,
            &mut context,
            &source_map,
        );

    let (
        name_resolver,
        global_symbols,
    ) = resolver
        .resolve()
        .unwrap_or_else(|errors| {
            panic!(
                "name resolution failed:\n{errors:#?}"
            );
        });

    let analyzer =
        Analyzer::new(
            &module_tree,
            &mut context,
            &source_map,
            name_resolver,
            global_symbols,
        );

    analyzer
        .analyze()
        .unwrap_or_else(|errors| {
            panic!(
                "semantic analysis failed:\n{errors:#?}"
            );
        })
}


fn main_body(
    program: &HIRProgram,
) -> &[HIRStmt] {
    let function =
        program
            .functions
            .iter()
            .find(|function| {
                function.name == "main"
            })
            .expect(
                "expected main function",
            );

    &function.body.stmts
}


// ============================================================================
// FOR-LOOP EXTRACTION
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
struct ForLowering {
    start: i64,
    end: i64,

    //
    // descending = start > end
    //
    direction_op: HIRBinOp,

    //
    // exclusive:
    //     descending: i <= end
    //     ascending:  i >= end
    //
    // inclusive:
    //     descending: i < end
    //     ascending:  i > end
    //
    descending_break_op: HIRBinOp,
    ascending_break_op: HIRBinOp,

    //
    // descending => i - 1
    // ascending  => i + 1
    //
    descending_step_op: HIRBinOp,
    descending_step: i64,

    ascending_step_op: HIRBinOp,
    ascending_step: i64,
}


fn expect_var_ref(
    expr: &HIRExpr,
    expected: DefID,
    context: &str,
) {
    let HIRExprKind::VarRef(actual) =
        &expr.kind
    else {
        panic!(
            "{context}: expected variable reference, found {:#?}",
            expr.kind,
        );
    };

    assert_eq!(
        *actual,
        expected,
        "{context}: unexpected variable reference",
    );
}


fn extract_break_op(
    expr: &HIRExpr,
    loop_def: DefID,
    end_def: DefID,
    context: &str,
) -> HIRBinOp {
    let HIRExprKind::If {
        cond,
        then_block,
        else_block,
    } = &expr.kind
    else {
        panic!(
            "{context}: expected break-check if expression, found {:#?}",
            expr.kind,
        );
    };

    assert!(
        else_block.is_none(),
        "{context}: break check should not have an else block",
    );

    assert_eq!(
        then_block.stmts.len(),
        1,
        "{context}: break branch should contain exactly one statement",
    );

    assert!(
        matches!(
            &then_block.stmts[0],
            HIRStmt::Expr(
                HIRExpr {
                    kind: HIRExprKind::Break,
                    ..
                }
            )
        ),
        "{context}: expected break statement",
    );

    let HIRExprKind::Binary {
        op,
        lhs,
        rhs,
    } = &cond.kind
    else {
        panic!(
            "{context}: expected binary break condition, found {:#?}",
            cond.kind,
        );
    };

    expect_var_ref(
        lhs,
        loop_def,
        &format!(
            "{context} lhs",
        ),
    );

    expect_var_ref(
        rhs,
        end_def,
        &format!(
            "{context} rhs",
        ),
    );

    *op
}


fn extract_step(
    stmt: &HIRStmt,
    loop_def: DefID,
    context: &str,
) -> (HIRBinOp, i64) {
    let HIRStmt::Expr(expr) =
        stmt
    else {
        panic!(
            "{context}: expected expression statement",
        );
    };

    let HIRExprKind::Assign {
        target,
        value,
    } = &expr.kind
    else {
        panic!(
            "{context}: expected assignment, found {:#?}",
            expr.kind,
        );
    };

    expect_var_ref(
        target,
        loop_def,
        &format!(
            "{context} assignment target",
        ),
    );

    let HIRExprKind::Binary {
        op,
        lhs,
        rhs,
    } = &value.kind
    else {
        panic!(
            "{context}: expected binary step expression, found {:#?}",
            value.kind,
        );
    };

    expect_var_ref(
        lhs,
        loop_def,
        &format!(
            "{context} step lhs",
        ),
    );

    let HIRExprKind::IntLiteral(step) =
        &rhs.kind
    else {
        panic!(
            "{context}: expected integer step, found {:#?}",
            rhs.kind,
        );
    };

    (
        *op,
        *step,
    )
}


fn extract_for_lowering(
    program: &HIRProgram,
) -> ForLowering {
    let body =
        main_body(program);

    assert_eq!(
        body.len(),
        1,
        "expected main to contain exactly one top-level statement",
    );

    //
    // for i in start..end {
    //     ...
    // }
    //
    // lowers to:
    //
    // {
    //     let i = start;
    //     let end_tmp = end;
    //     let descending = i > end_tmp;
    //
    //     loop {
    //         if descending {
    //             if i <=/< end_tmp {
    //                 break;
    //             }
    //         } else {
    //             if i >=/> end_tmp {
    //                 break;
    //             }
    //         }
    //
    //         ...
    //
    //         if descending {
    //             i = i - 1;
    //         } else {
    //             i = i + 1;
    //         }
    //     }
    // }
    //
    let HIRStmt::Expr(expr) =
        &body[0]
    else {
        panic!(
            "expected lowered for-loop block",
        );
    };

    let HIRExprKind::Block(block) =
        &expr.kind
    else {
        panic!(
            "expected for-loop to lower into a block, found {:#?}",
            expr.kind,
        );
    };

    assert_eq!(
        block.stmts.len(),
        4,
        "expected for-loop block to contain loop variable, end value, direction flag, and loop",
    );


    // ------------------------------------------------------------------------
    // let i = start;
    // ------------------------------------------------------------------------

    let HIRStmt::VarDecl {
        def_id: loop_def,
        init: Some(start_expr),
        ..
    } = &block.stmts[0]
    else {
        panic!(
            "expected loop-variable initializer",
        );
    };

    let HIRExprKind::IntLiteral(start) =
        &start_expr.kind
    else {
        panic!(
            "expected integer loop start, found {:#?}",
            start_expr.kind,
        );
    };


    // ------------------------------------------------------------------------
    // let end_tmp = end;
    // ------------------------------------------------------------------------

    let HIRStmt::VarDecl {
        def_id: end_def,
        init: Some(end_expr),
        ..
    } = &block.stmts[1]
    else {
        panic!(
            "expected cached loop-end initializer",
        );
    };

    let HIRExprKind::IntLiteral(end) =
        &end_expr.kind
    else {
        panic!(
            "expected integer loop end, found {:#?}",
            end_expr.kind,
        );
    };


    // ------------------------------------------------------------------------
    // let descending = i > end_tmp;
    // ------------------------------------------------------------------------

    let HIRStmt::VarDecl {
        def_id: descending_def,
        init: Some(direction_expr),
        ..
    } = &block.stmts[2]
    else {
        panic!(
            "expected loop-direction initializer",
        );
    };

    let HIRExprKind::Binary {
        op: direction_op,
        lhs: direction_lhs,
        rhs: direction_rhs,
    } = &direction_expr.kind
    else {
        panic!(
            "expected binary direction check, found {:#?}",
            direction_expr.kind,
        );
    };

    expect_var_ref(
        direction_lhs,
        *loop_def,
        "direction check lhs",
    );

    expect_var_ref(
        direction_rhs,
        *end_def,
        "direction check rhs",
    );


    // ------------------------------------------------------------------------
    // loop { ... }
    // ------------------------------------------------------------------------

    let HIRStmt::Expr(loop_expr) =
        &block.stmts[3]
    else {
        panic!(
            "expected loop expression",
        );
    };

    let HIRExprKind::Loop(loop_body) =
        &loop_expr.kind
    else {
        panic!(
            "expected HIR loop, found {:#?}",
            loop_expr.kind,
        );
    };

    assert!(
        loop_body.stmts.len() >= 2,
        "expected loop to contain range check and step",
    );


    // ------------------------------------------------------------------------
    // if descending {
    //     descending break check
    // } else {
    //     ascending break check
    // }
    // ------------------------------------------------------------------------

    let HIRStmt::Expr(range_check_expr) =
        &loop_body.stmts[0]
    else {
        panic!(
            "expected range-check expression",
        );
    };

    let HIRExprKind::If {
        cond: range_direction,
        then_block: descending_block,
        else_block: Some(ascending_block),
    } = &range_check_expr.kind
    else {
        panic!(
            "expected direction-dependent range check, found {:#?}",
            range_check_expr.kind,
        );
    };

    expect_var_ref(
        range_direction,
        *descending_def,
        "range direction",
    );

    assert_eq!(
        descending_block.stmts.len(),
        1,
        "descending range branch should contain exactly one break check",
    );

    assert_eq!(
        ascending_block.stmts.len(),
        1,
        "ascending range branch should contain exactly one break check",
    );

    let HIRStmt::Expr(descending_break_expr) =
        &descending_block.stmts[0]
    else {
        panic!(
            "expected descending break expression",
        );
    };

    let HIRStmt::Expr(ascending_break_expr) =
        &ascending_block.stmts[0]
    else {
        panic!(
            "expected ascending break expression",
        );
    };

    let descending_break_op =
        extract_break_op(
            descending_break_expr,
            *loop_def,
            *end_def,
            "descending break",
        );

    let ascending_break_op =
        extract_break_op(
            ascending_break_expr,
            *loop_def,
            *end_def,
            "ascending break",
        );


    // ------------------------------------------------------------------------
    // if descending {
    //     i = i - 1;
    // } else {
    //     i = i + 1;
    // }
    // ------------------------------------------------------------------------

    let HIRStmt::Expr(step_expr) =
        loop_body
            .stmts
            .last()
            .expect(
                "expected loop step",
            )
    else {
        panic!(
            "expected step expression",
        );
    };

    let HIRExprKind::If {
        cond: step_direction,
        then_block: descending_step_block,
        else_block: Some(ascending_step_block),
    } = &step_expr.kind
    else {
        panic!(
            "expected direction-dependent loop step, found {:#?}",
            step_expr.kind,
        );
    };

    expect_var_ref(
        step_direction,
        *descending_def,
        "step direction",
    );

    assert_eq!(
        descending_step_block.stmts.len(),
        1,
        "descending step branch should contain exactly one statement",
    );

    assert_eq!(
        ascending_step_block.stmts.len(),
        1,
        "ascending step branch should contain exactly one statement",
    );

    let (
        descending_step_op,
        descending_step,
    ) = extract_step(
        &descending_step_block.stmts[0],
        *loop_def,
        "descending step",
    );

    let (
        ascending_step_op,
        ascending_step,
    ) = extract_step(
        &ascending_step_block.stmts[0],
        *loop_def,
        "ascending step",
    );


    ForLowering {
        start: *start,
        end: *end,

        direction_op: *direction_op,

        descending_break_op,
        ascending_break_op,

        descending_step_op,
        descending_step,

        ascending_step_op,
        ascending_step,
    }
}


// ============================================================================
// RANGE LOWERING
// ============================================================================

#[test]
fn lowers_ascending_exclusive_range() {
    let hir =
        analyze(
            r#"
            fn main() -> void {
                for i in 0..5 {
                    println("{}", i);
                }
            }
            "#,
        );

    assert_eq!(
        extract_for_lowering(
            &hir,
        ),
        ForLowering {
            start: 0,
            end: 5,

            direction_op:
                HIRBinOp::Gt,

            descending_break_op:
                HIRBinOp::Le,

            ascending_break_op:
                HIRBinOp::Ge,

            descending_step_op:
                HIRBinOp::Sub,

            descending_step:
                1,

            ascending_step_op:
                HIRBinOp::Add,

            ascending_step:
                1,
        },
    );
}


#[test]
fn lowers_ascending_inclusive_range() {
    let hir =
        analyze(
            r#"
            fn main() -> void {
                for i in 0..=5 {
                    println("{}", i);
                }
            }
            "#,
        );

    assert_eq!(
        extract_for_lowering(
            &hir,
        ),
        ForLowering {
            start: 0,
            end: 5,

            direction_op:
                HIRBinOp::Gt,

            descending_break_op:
                HIRBinOp::Lt,

            ascending_break_op:
                HIRBinOp::Gt,

            descending_step_op:
                HIRBinOp::Sub,

            descending_step:
                1,

            ascending_step_op:
                HIRBinOp::Add,

            ascending_step:
                1,
        },
    );
}


#[test]
fn lowers_descending_exclusive_range() {
    let hir =
        analyze(
            r#"
            fn main() -> void {
                for i in 5..0 {
                    println("{}", i);
                }
            }
            "#,
        );

    //
    // 5..0:
    //
    //     5, 4, 3, 2, 1
    //
    // Descending execution therefore:
    //
    //     breaks when i <= end
    //     steps with i - 1
    //
    assert_eq!(
        extract_for_lowering(
            &hir,
        ),
        ForLowering {
            start: 5,
            end: 0,

            direction_op:
                HIRBinOp::Gt,

            descending_break_op:
                HIRBinOp::Le,

            ascending_break_op:
                HIRBinOp::Ge,

            descending_step_op:
                HIRBinOp::Sub,

            descending_step:
                1,

            ascending_step_op:
                HIRBinOp::Add,

            ascending_step:
                1,
        },
    );
}


#[test]
fn lowers_descending_inclusive_range() {
    let hir =
        analyze(
            r#"
            fn main() -> void {
                for i in 5..=0 {
                    println("{}", i);
                }
            }
            "#,
        );

    //
    // 5..=0:
    //
    //     5, 4, 3, 2, 1, 0
    //
    // Descending execution therefore:
    //
    //     breaks when i < end
    //     steps with i - 1
    //
    assert_eq!(
        extract_for_lowering(
            &hir,
        ),
        ForLowering {
            start: 5,
            end: 0,

            direction_op:
                HIRBinOp::Gt,

            descending_break_op:
                HIRBinOp::Lt,

            ascending_break_op:
                HIRBinOp::Gt,

            descending_step_op:
                HIRBinOp::Sub,

            descending_step:
                1,

            ascending_step_op:
                HIRBinOp::Add,

            ascending_step:
                1,
        },
    );
}


// ============================================================================
// EMPTY BODIES
// ============================================================================

#[test]
fn analyzes_empty_while_body() {
    let hir =
        analyze(
            r#"
            fn main() -> void {
                while true {
                }
            }
            "#,
        );

    let body =
        main_body(
            &hir,
        );

    assert_eq!(
        body.len(),
        1,
    );

    let HIRStmt::Expr(expr) =
        &body[0]
    else {
        panic!(
            "expected while expression",
        );
    };

    assert!(
        matches!(
            expr.kind,
            HIRExprKind::Loop(_)
        ),
        "while should lower into a HIR loop",
    );
}


#[test]
fn analyzes_empty_for_body() {
    let hir =
        analyze(
            r#"
            fn main() -> void {
                for i in 0..5 {
                }
            }
            "#,
        );

    assert_eq!(
        extract_for_lowering(
            &hir,
        ),
        ForLowering {
            start: 0,
            end: 5,

            direction_op:
                HIRBinOp::Gt,

            descending_break_op:
                HIRBinOp::Le,

            ascending_break_op:
                HIRBinOp::Ge,

            descending_step_op:
                HIRBinOp::Sub,

            descending_step:
                1,

            ascending_step_op:
                HIRBinOp::Add,

            ascending_step:
                1,
        },
    );
}
