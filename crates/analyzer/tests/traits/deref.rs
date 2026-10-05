use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{traits, Analyzer, Resolver};
use ir::{
    context::HIRContext,
    hir::{HIRExpr, HIRExprKind, HIRFunction, HIRProgram, HIRStmt},
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);

        let dir = std::env::temp_dir().join(format!(
            "hydrac-deref-lowering-test-{}-{}",
            std::process::id(),
            id,
        ));

        fs::create_dir_all(dir.join("core/fmt"))
            .expect("failed to create core::fmt test module");

        fs::create_dir_all(dir.join("core/ops"))
            .expect("failed to create core::ops test module");

        fs::write(
            dir.join("core/fmt/mod.hydra"),
            "",
        )
        .expect("failed to write core::fmt test module");

        fs::write(
            dir.join("core/ops/deref.hydra"),
            r#"
pub trait Deref<Target> {
    fn deref(&self) -> &Target;
}

pub trait DerefMut<Target> {
    fn deref_mut(&mut self) -> &mut Target;
}
"#,
        )
        .expect("failed to write Deref test module");

        let entry = dir.join("main.hydra");

        fs::write(&entry, source)
            .expect("failed to write temporary Hydra source");

        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn analyze(source: &str) -> (HIRProgram, HIRContext) {
    let project = TestProject::new(source);
    let mut source_map = SourceMap::new();

    let mut module_tree = ModuleTree::build(
        Path::new(&project.entry),
        project.dir.clone(),
        &mut source_map,
    )
    .unwrap_or_else(|errors| {
        panic!("module construction failed:\n{errors:#?}");
    });

    module_tree
        .resolve_imports(&mut source_map)
        .unwrap_or_else(|errors| {
            panic!("import resolution failed:\n{errors:#?}");
        });

    module_tree
        .parse_bodies(&source_map)
        .unwrap_or_else(|errors| {
            panic!("body parsing failed:\n{errors:#?}");
        });

    let mut context = HIRContext::default();

    let resolver = Resolver::new(
        &module_tree,
        &mut context,
        &source_map,
    );

    let (name_resolver, global_symbols) = resolver
        .resolve()
        .unwrap_or_else(|errors| {
            panic!("name resolution failed:\n{errors:#?}");
        });

    let hir = Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze()
    .unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    });

    (hir, context)
}

fn find_function<'a>(
    hir: &'a HIRProgram,
    name: &str,
) -> &'a HIRFunction {
    hir.functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| {
            panic!("function `{name}` should exist");
        })
}

fn returned_value(function: &HIRFunction) -> &HIRExpr {
    let stmt = function
        .body
        .stmts
        .first()
        .expect("function should contain a return statement");

    let HIRStmt::Expr(return_expr) = stmt else {
        panic!(
            "expected expression statement, found {stmt:#?}"
        );
    };

    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!(
            "expected return expression, found {:#?}",
            return_expr.kind,
        );
    };

    value
}

#[test]
fn deref_trait_lowers_to_trait_call_followed_by_dereference() {
    let (hir, context) = analyze(
        r#"
include core::ops::deref::Deref;

struct Wrapper<T> {
    value: T;
}

extension<T> Deref<T> on Wrapper<T> {
    fn deref(&self) -> &T {
        return &self.value;
    }
}

fn read(wrapper: Wrapper<i32>) -> i32 {
    return *wrapper;
}

fn main() -> void {
}
"#,
    );

    let function = find_function(
        &hir,
        "read",
    );

    let value = returned_value(function);

    assert_eq!(
        value.ty,
        Type::I32,
    );

    let HIRExprKind::Dereference { target } = &value.kind else {
        panic!(
            "expected dereference, found {:#?}",
            value.kind,
        );
    };

    assert_eq!(
        target.ty,
        Type::CONST_REF(Box::new(Type::I32)),
    );

    let HIRExprKind::Call {
        callee,
        args,
        trait_dispatch,
        ..
    } = &target.kind
    else {
        panic!(
            "custom dereference should call Deref::deref, found {:#?}",
            target.kind,
        );
    };

    assert_eq!(
        args.len(),
        1,
    );

    assert!(
        trait_dispatch.is_none(),
        "a concrete Deref implementation should not use deferred trait dispatch",
    );

    let HIRExprKind::Borrow {
        is_mut,
        ..
    } = &args[0].kind
    else {
        panic!(
            "Deref receiver should be borrowed, found {:#?}",
            args[0].kind,
        );
    };

    assert!(
        !is_mut,
        "Deref::deref should receive a shared borrow",
    );

    let deref_trait_def_id = context
        .find_trait_by_path(traits::DEREF_TRAIT_PATH)
        .expect("Deref trait should exist");

    let deref_impl = context
        .trait_impls
        .iter()
        .find(|trait_impl| {
            trait_impl.trait_def_id == deref_trait_def_id
        })
        .expect("Deref implementation should exist");

    assert_eq!(
        deref_impl.methods.get("deref"),
        Some(callee),
        "the dereference operator should call the compiler-recognized Deref implementation",
    );
}

#[test]
fn deref_mut_trait_lowers_assignment_target_to_mutable_trait_call() {
    let (hir, context) = analyze(
        r#"
include core::ops::deref::{Deref, DerefMut};

struct Wrapper<T> {
    value: T;
}

extension<T> Deref<T> on Wrapper<T> {
    fn deref(&self) -> &T {
        return &self.value;
    }
}

extension<T> DerefMut<T> on Wrapper<T> {
    fn deref_mut(&mut self) -> &mut T {
        return &mut self.value;
    }
}

fn write(wrapper: Wrapper<i32>, value: i32) -> void {
    *wrapper = value;
}

fn main() -> void {
}
"#,
    );

    let function = find_function(
        &hir,
        "write",
    );

    let stmt = function
        .body
        .stmts
        .first()
        .expect("write should contain an assignment");

    let HIRStmt::Expr(assign_expr) = stmt else {
        panic!(
            "expected expression statement, found {stmt:#?}"
        );
    };

    let HIRExprKind::Assign {
        target,
        ..
    } = &assign_expr.kind
    else {
        panic!(
            "expected assignment, found {:#?}",
            assign_expr.kind,
        );
    };

    assert_eq!(
        target.ty,
        Type::I32,
    );

    let HIRExprKind::Dereference {
        target: deref_target,
    } = &target.kind
    else {
        panic!(
            "assignment target should be a dereference, found {:#?}",
            target.kind,
        );
    };

    assert_eq!(
        deref_target.ty,
        Type::REF(Box::new(Type::I32)),
    );

    let HIRExprKind::Call {
        callee,
        args,
        trait_dispatch,
        ..
    } = &deref_target.kind
    else {
        panic!(
            "mutable dereference should call DerefMut::deref_mut, found {:#?}",
            deref_target.kind,
        );
    };

    assert_eq!(
        args.len(),
        1,
    );

    assert!(
        trait_dispatch.is_none(),
        "a concrete DerefMut implementation should not use deferred trait dispatch",
    );

    let HIRExprKind::Borrow {
        is_mut,
        ..
    } = &args[0].kind
    else {
        panic!(
            "DerefMut receiver should be borrowed, found {:#?}",
            args[0].kind,
        );
    };

    assert!(
        *is_mut,
        "DerefMut::deref_mut should receive a mutable borrow",
    );

    let deref_mut_trait_def_id = context
        .find_trait_by_path(traits::DEREF_MUT_TRAIT_PATH)
        .expect("DerefMut trait should exist");

    let deref_mut_impl = context
        .trait_impls
        .iter()
        .find(|trait_impl| {
            trait_impl.trait_def_id == deref_mut_trait_def_id
        })
        .expect("DerefMut implementation should exist");

    assert_eq!(
        deref_mut_impl.methods.get("deref_mut"),
        Some(callee),
        "assignment through `*` should call the compiler-recognized DerefMut implementation",
    );
}

#[test]
fn native_reference_dereference_does_not_call_deref_trait() {
    let (hir, _) = analyze(
        r#"
fn read(value: &i32) -> i32 {
    return *value;
}

fn main() -> void {
}
"#,
    );

    let function = find_function(
        &hir,
        "read",
    );

    let value = returned_value(function);

    assert_eq!(
        value.ty,
        Type::I32,
    );

    let HIRExprKind::Dereference { target } = &value.kind else {
        panic!(
            "expected native dereference, found {:#?}",
            value.kind,
        );
    };

    assert!(
        !matches!(
            target.kind,
            HIRExprKind::Call { .. }
        ),
        "native reference dereference should not invoke Deref",
    );
}
