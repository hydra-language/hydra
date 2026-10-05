use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::HIRContext,
    hir::HIRProgram,
};
use parser::module::{
    ModuleTree,
    SourceMap,
};

static NEXT_PROJECT_ID: AtomicUsize = AtomicUsize::new(0);

struct TestProject {
    root: PathBuf,
    entry: PathBuf,
    stdlib: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id = NEXT_PROJECT_ID.fetch_add(
            1,
            Ordering::Relaxed,
        );

        let root = std::env::temp_dir().join(format!(
            "hydra-match-exhaustiveness-{}-{id}",
            std::process::id(),
        ));

        let entry = root.join("main.hydra");
        let stdlib = root.join("stdlib");
        let fmt = stdlib
            .join("core")
            .join("fmt");

        fs::create_dir_all(&fmt)
            .expect("failed to create test stdlib");

        fs::write(
            &entry,
            source,
        )
        .expect("failed to write test source");

        fs::write(
            fmt.join("mod.hydra"),
            "",
        )
        .expect("failed to write fmt module");

        Self {
            root,
            entry,
            stdlib,
        }
    }

    fn build(&self) -> (ModuleTree, SourceMap) {
        let mut source_map = SourceMap::new();

        let mut program = ModuleTree::build(
            &self.entry,
            self.stdlib.clone(),
            &mut source_map,
        )
        .expect("module tree should build");

        program
            .resolve_imports(&mut source_map)
            .expect("imports should resolve");

        (
            program,
            source_map,
        )
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(
            &self.root,
        );
    }
}

fn analyze(
    source: &str,
) -> Result<HIRProgram, Vec<HydraError>> {
    let project = TestProject::new(source);
    let (program, source_map) = project.build();

    let mut context = HIRContext::default();

    let resolver = Resolver::new(
        &program,
        &mut context,
        &source_map,
    );

    let (name_resolver, global_symbols) = resolver
        .resolve()
        .expect("name resolution should succeed");

    Analyzer::new(
        &program,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze()
}

fn m001(
    source: &str,
) -> HydraError {
    analyze(source)
        .expect_err(
            "match should be non-exhaustive",
        )
        .into_iter()
        .find(|error| error.code == "M001")
        .expect("expected M001")
}

#[test]
fn non_exhaustive_i32_gets_the_important_diagnostic() {
    let error = m001(
        r#"
        fn test(x: i32) -> void {
            match x {
                1 => {}
            }
        }

        fn main() -> void {
        }
        "#,
    );

    assert_eq!(
        error.message,
        "non-exhaustive match on `i32`",
    );

    assert!(
        error.help.iter().any(|help| {
            help ==
                "handle all 4,294,967,296 possible `i32` values or add an `else` block"
        }),
        "missing integer-domain help: {error:#?}",
    );

    assert!(
        error.notes.iter().any(|note| {
            note.message ==
                "unless you have ambitious plans for this match, consider `else`"
        }),
        "the important part of M001 disappeared: {error:#?}",
    );
}

#[test]
fn non_exhaustive_u8_reports_256_values() {
    let error = m001(
        r#"
        fn test(x: u8) -> void {
            match x {
                0 => {}
            }
        }

        fn main() -> void {
        }
        "#,
    );

    assert!(
        error.help.iter().any(|help| {
            help ==
                "handle all 256 possible `u8` values or add an `else` block"
        }),
        "unexpected u8 exhaustiveness help: {error:#?}",
    );
}

#[test]
fn optional_u8_reports_257_states() {
    let error = m001(
        r#"
        fn test(x: u8?) -> void {
            match x {
                None => {}
                0 => {}
            }
        }

        fn main() -> void {
        }
        "#,
    );

    assert!(
        error.help.iter().any(|help| {
            help ==
                "handle all 257 possible states of `u8?` or add an `else` block"
        }),
        "unexpected optional u8 exhaustiveness help: {error:#?}",
    );
}

#[test]
fn bool_can_be_exhaustive_without_else() {
    analyze(
        r#"
        fn select(flag: bool) -> i32 {
            let value = match flag {
                true => 69,
                false => 420,
            };

            return value;
        }

        fn main() -> void {
        }
        "#,
    )
    .expect(
        "true and false should exhaust bool",
    );
}

#[test]
fn full_u8_range_can_be_exhaustive_without_else() {
    analyze(
        r#"
        fn test(x: u8) -> void {
            match x {
                0..=255 => {}
            }
        }

        fn main() -> void {
        }
        "#,
    )
    .expect(
        "0..=255 should exhaust u8",
    );
}

#[test]
fn optional_u8_requires_none_in_addition_to_full_value_range() {
    analyze(
        r#"
        fn test(x: u8?) -> void {
            match x {
                None => {}
                0..=255 => {}
            }
        }

        fn main() -> void {
        }
        "#,
    )
    .expect(
        "None plus 0..=255 should exhaust u8?",
    );
}
