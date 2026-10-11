use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::SemanticContext,
    hir::{HIRExpr, HIRExprKind, HIRProgram, HIRStmt},
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn analyze(source: &str) -> Result<HIRProgram, Vec<HydraError>> {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "hydrac-generic-call-test-{}-{id}",
        std::process::id(),
    ));
    fs::create_dir_all(&dir).expect("failed to create test directory");

    let result = (|| {
        let entry = dir.join("main.hydra");
        fs::write(&entry, source).expect("failed to write test source");

        let mut source_map = SourceMap::new();
        let mut modules = ModuleTree::build(&entry, dir.clone(), &mut source_map)?;
        modules.resolve_imports(&mut source_map)?;
        modules.parse_bodies(&source_map)?;

        let mut context = SemanticContext::new();
        let (resolver, symbols) =
            Resolver::new(&modules, &mut context, &source_map).resolve()?;

        Analyzer::new(&modules, context, &source_map, resolver, symbols)
            .analyze()
            .map(|analyzed| analyzed.hir)
    })();

    let _ = fs::remove_dir_all(dir);
    result
}

fn main_init_call(hir: &HIRProgram, index: usize) -> &HIRExpr {
    let main = hir.functions.iter()
        .find(|function| function.name == "main")
        .expect("main should exist");

    let HIRStmt::VarDecl { init: Some(expr), .. } = &main.body.stmts[index] else {
        panic!("expected initialized variable declaration");
    };

    expr
}

#[test]
fn owner_and_function_generics_flatten_in_declaration_order() {
    let hir = analyze(r#"
struct Owner<T> {
    value: T;
}

extension<T> Owner<T> {
    fn token() -> usize {
        return 0;
    }

    fn convert<U>(value: U) -> U {
        return value;
    }

    fn map<U>(&self, value: U) -> U {
        return value;
    }
}

fn identity<T>(value: T) -> T {
    return value;
}

fn use_owner(owner: Owner<i32>) -> u64 {
    return owner::map::<u64>(7);
}

fn main() -> void {
    const owner = Owner::<i32>::token();
    const both = Owner::<i32>::convert::<u64>(7);
    const free = identity::<i32>(9);
}
"#).expect("owner/function generic calls should analyze");

    let owner_call = main_init_call(&hir, 0);
    let HIRExprKind::Call { generic_args, .. } = &owner_call.kind else {
        panic!("expected associated call, found {:#?}", owner_call.kind);
    };
    assert_eq!(generic_args, &[Type::I32]);
    assert_eq!(owner_call.ty, Type::USIZE);

    let both_call = main_init_call(&hir, 1);
    let HIRExprKind::Call { generic_args, .. } = &both_call.kind else {
        panic!("expected associated generic call, found {:#?}", both_call.kind);
    };
    assert_eq!(generic_args, &[Type::I32, Type::U64]);
    assert_eq!(both_call.ty, Type::U64);

    let free_call = main_init_call(&hir, 2);
    let HIRExprKind::Call { generic_args, .. } = &free_call.kind else {
        panic!("expected free generic call, found {:#?}", free_call.kind);
    };
    assert_eq!(generic_args, &[Type::I32]);
    assert_eq!(free_call.ty, Type::I32);

    let use_owner = hir.functions.iter()
        .find(|function| function.name == "use_owner")
        .expect("use_owner should exist");

    let HIRStmt::Expr(return_expr) = &use_owner.body.stmts[0] else {
        panic!("expected return expression");
    };
    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!("expected returned value");
    };
    let HIRExprKind::Call { generic_args, .. } = &value.kind else {
        panic!("expected instance method call, found {:#?}", value.kind);
    };

    assert_eq!(
        generic_args,
        &[Type::I32, Type::U64],
        "instance methods must infer owner generics before function generics",
    );
    assert_eq!(value.ty, Type::U64);
}

#[test]
fn rejects_function_fishtail_for_owner_only_generic() {
    let errors = analyze(r#"
struct Owner<T> {
    value: T;
}

extension<T> Owner<T> {
    fn token() -> usize {
        return 0;
    }
}

fn main() -> void {
    const value = Owner::token::<i32>();
}
"#).expect_err("token has no function generic parameters");

    assert!(
        errors.iter().any(|error| {
            error.code == "S004"
                && error.message.contains(
                    "function expected at most 0 generic arguments, found 1"
                )
        }),
        "expected S004 for postfix generics on a nongeneric function, got {errors:#?}",
    );
}
