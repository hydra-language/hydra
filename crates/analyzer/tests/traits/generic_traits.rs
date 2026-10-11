use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::{DefID, DefKind, SemanticContext},
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
            "hydrac-generic-trait-test-{}-{id}",
            std::process::id(),
        ));

        fs::create_dir_all(dir.join("core/fmt")).expect("failed to create test directory");
        fs::write(dir.join("core/fmt/mod.hydra"), "").expect("failed to write test module");

        let entry = dir.join("main.hydra");
        fs::write(&entry, source).expect("failed to write Hydra source");
        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn analyze_result(source: &str) -> Result<SemanticContext, Vec<HydraError>> {
    let project = TestProject::new(source);
    let mut source_map = SourceMap::new();
    let mut modules = ModuleTree::build(
        &project.entry,
        project.dir.clone(),
        &mut source_map,
    )?;

    modules.resolve_imports(&mut source_map)?;
    modules.parse_bodies(&source_map)?;

    let mut context = SemanticContext::new();
    let (name_resolver, global_symbols) =
        Resolver::new(&modules, &mut context, &source_map).resolve()?;

    let analyzed = Analyzer::new(
        &modules,
        context,
        &source_map,
        name_resolver,
        global_symbols,
    ).analyze()?;

    Ok(analyzed.context)
}

fn analyze(source: &str) -> SemanticContext {
    analyze_result(source).unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    })
}

fn find_definition(context: &SemanticContext, name: &str) -> DefID {
    context.definitions.iter()
        .find_map(|(id, info)| (info.name == name).then_some(*id))
        .unwrap_or_else(|| panic!("definition `{name}` should exist"))
}

#[test]
fn trait_generic_is_available_to_contract_methods() {
    let context = analyze(r#"
trait Deref<Target> {
    fn deref(&self) -> &Target;
}

fn main() -> void {}
"#);

    let deref_id = find_definition(&context, "Deref");
    let deref = context.get_def(deref_id).expect("Deref should exist");

    let DefKind::Trait { methods, .. } = &deref.kind else {
        panic!("Deref should be a trait");
    };
    assert_eq!(methods.len(), 1);

    let method = context.get_def(methods[0]).expect("Deref::deref should exist");
    let DefKind::Function {
        params,
        return_type,
        generic_params,
        owner_generic_count,
        ..
    } = &method.kind else {
        panic!("Deref::deref should be a function");
    };

    assert_eq!(generic_params, &vec!["Target".to_string()]);
    assert_eq!(*owner_generic_count, 1);
    assert_eq!(
        params,
        &vec![Type::CONST_REF(Box::new(Type::GENERIC("Self".into())))],
    );
    assert_eq!(
        return_type,
        &Type::CONST_REF(Box::new(Type::GENERIC("Target".into()))),
    );
}

#[test]
fn generic_trait_application_is_registered_with_arguments() {
    let context = analyze(r#"
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

fn main() -> void {}
"#);

    let deref_id = find_definition(&context, "Deref");
    let trait_impl = context.trait_impls.iter()
        .find(|implementation| implementation.trait_def_id == deref_id)
        .expect("Deref implementation should be registered");

    assert_eq!(trait_impl.trait_args, vec![Type::GENERIC("T".into())]);

    let Type::GENERIC_INSTANCE(_, target_args) = &trait_impl.target_type else {
        panic!("expected generic instance, found {:?}", trait_impl.target_type);
    };

    assert_eq!(target_args, &vec![Type::GENERIC("T".into())]);
    assert!(
        trait_impl.methods.contains_key("deref"),
        "Deref implementation should register its method",
    );
}
