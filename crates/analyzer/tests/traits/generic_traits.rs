use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::{DefID, DefKind, HIRContext},
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
            "hydrac-generic-trait-test-{}-{}",
            std::process::id(),
            id,
        ));

        fs::create_dir_all(dir.join("core/fmt"))
            .expect("failed to create core::fmt test module");

        fs::write(
            dir.join("core/fmt/mod.hydra"),
            "",
        )
        .expect("failed to write core::fmt test module");

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

fn analyze_result(source: &str) -> Result<HIRContext, Vec<HydraError>> {
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

    let result = Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze();

    match result {
        Ok(_) => Ok(context),
        Err(errors) => Err(errors),
    }
}

fn analyze(source: &str) -> HIRContext {
    analyze_result(source).unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    })
}

fn find_definition(context: &HIRContext, name: &str) -> DefID {
    context
        .definitions
        .iter()
        .find_map(|(def_id, info)| {
            if info.name == name {
                Some(*def_id)
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            panic!("definition `{name}` should exist");
        })
}

#[test]
fn trait_generic_is_available_to_contract_methods() {
    let context = analyze(
        r#"
trait Deref<Target> {
    fn deref(&self) -> &Target;
}

fn main() -> void {
}
"#,
    );

    let deref_def_id = find_definition(&context, "Deref");

    let deref = context
        .get_def(deref_def_id)
        .expect("Deref definition should exist");

    let DefKind::Trait { methods, .. } = &deref.kind else {
        panic!("Deref should be a trait");
    };

    assert_eq!(methods.len(), 1);

    let method = context
        .get_def(methods[0])
        .expect("Deref::deref should exist");

    let DefKind::Function {
        params,
        return_type,
        generic_params,
        owner_generic_count,
        ..
    } = &method.kind
    else {
        panic!("Deref::deref should be a function");
    };

    assert_eq!(
        generic_params,
        &vec!["Target".to_string()],
    );

    assert_eq!(*owner_generic_count, 1);

    assert_eq!(
        params,
        &vec![Type::CONST_REF(Box::new(
            Type::GENERIC("Self".to_string()),
        ))],
    );

    assert_eq!(
        return_type,
        &Type::CONST_REF(Box::new(
            Type::GENERIC("Target".to_string()),
        )),
    );
}

#[test]
fn generic_trait_application_is_registered_with_arguments() {
    let context = analyze(
        r#"
trait Deref<Target> {
    fn deref(&self) -> &Target;
}

struct Box<T> {
    value: T;
}

extension<T> Deref<T> on Box<T> {
    fn deref(&self) -> &T {
        return &self.value;
    }
}

fn main() -> void {
}
"#,
    );

    let deref_def_id = find_definition(&context, "Deref");

    let trait_impl = context
        .trait_impls
        .iter()
        .find(|trait_impl| trait_impl.trait_def_id == deref_def_id)
        .expect("Deref implementation should be registered");

    assert_eq!(
        trait_impl.trait_args,
        vec![Type::GENERIC("T".to_string())],
    );

    let Type::GENERIC_INSTANCE(_, target_args) = &trait_impl.target_type else {
        panic!(
            "Deref target should be a generic instance, found {:?}",
            trait_impl.target_type,
        );
    };

    assert_eq!(
        target_args,
        &vec![Type::GENERIC("T".to_string())],
    );

    assert!(
        trait_impl.methods.contains_key("deref"),
        "Deref implementation should register its `deref` method",
    );
}
