use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use analyzer::{
    Analyzer,
    Resolver,
};

use errors::error::HydraError;

use ir::{
    context::HIRContext,
    hir::HIRProgram,
};

use parser::module::{
    ModuleTree,
    SourceMap,
};


static NEXT_TEST_ID: AtomicUsize =
    AtomicUsize::new(0);


struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}


impl TestProject {

    fn new(
        source: &str,
    ) -> Self {
        let id =
            NEXT_TEST_ID.fetch_add(
                1,
                Ordering::Relaxed,
            );

        let dir =
            std::env::temp_dir().join(
                format!(
                    "hydrac-field-visibility-test-{}-{}",
                    std::process::id(),
                    id,
                ),
            );

        fs::create_dir_all(
            &dir,
        )
        .expect(
            "failed to create temporary test directory",
        );

        let entry =
            dir.join(
                "main.hydra"
            );

        fs::write(
            &entry,
            source,
        )
        .expect(
            "failed to write entry source",
        );

        Self {
            dir,
            entry,
        }
    }


    fn write(
        &self,
        relative:
            &str,
        source:
            &str,
    ) {
        let path =
            self.dir.join(
                relative
            );

        if let Some(parent) =
            path.parent()
        {
            fs::create_dir_all(
                parent,
            )
            .expect(
                "failed to create module directory",
            );
        }

        fs::write(
            path,
            source,
        )
        .expect(
            "failed to write module source",
        );
    }
}


impl Drop for TestProject {

    fn drop(
        &mut self
    ) {
        let _ =
            fs::remove_dir_all(
                &self.dir
            );
    }
}


fn analyze_project(
    project:
        &TestProject,
) -> Result<
    HIRProgram,
    Vec<HydraError>,
> {
    let mut source_map =
        SourceMap::new();

    let mut module_tree =
        ModuleTree::build(
            Path::new(
                &project.entry
            ),
            project.dir.clone(),
            &mut source_map,
        )
        .unwrap_or_else(
            |errors| {
                panic!(
                    "module construction failed:\n{errors:#?}"
                );
            },
        );

    module_tree
        .resolve_imports(
            &mut source_map,
        )
        .unwrap_or_else(
            |errors| {
                panic!(
                    "import resolution failed:\n{errors:#?}"
                );
            },
        );

    module_tree
        .parse_bodies(
            &source_map,
        )
        .unwrap_or_else(
            |errors| {
                panic!(
                    "body parsing failed:\n{errors:#?}"
                );
            },
        );

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
    ) =
        resolver
            .resolve()
            .unwrap_or_else(
                |errors| {
                    panic!(
                        "name resolution failed:\n{errors:#?}"
                    );
                },
            );

    Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze()
}


fn analyze(
    source: &str,
) -> Result<
    HIRProgram,
    Vec<HydraError>,
> {
    let project =
        TestProject::new(
            source
        );

    analyze_project(
        &project
    )
}


#[test]
fn private_field_is_accessible_inside_defining_module() {
    analyze(
        r#"
struct Value {
    hidden: i32;
}

fn read() -> i32 {
    const value = Value {
        .hidden = 42
    };

    return value.hidden;
}

fn main() -> void {
    println("{}", read());
}
"#,
    )
    .expect(
        "private field should be accessible in its defining module",
    );
}


#[test]
fn public_field_is_accessible_from_another_module() {
    let project =
        TestProject::new(
            r#"
include data::Value;

fn main() -> void {
    const value = Value {
        .visible = 42
    };

    println(
        "{}",
        value.visible
    );
}
"#,
        );

    project.write(
        "data.hydra",
        r#"
pub struct Value {
    pub visible: i32;
}
"#,
    );

    analyze_project(
        &project
    )
    .expect(
        "public field should be accessible externally",
    );
}


#[test]
fn private_field_read_is_rejected_from_another_module() {
    let project =
        TestProject::new(
            r#"
include data::{
    Value,
    make,
};

fn main() -> void {
    const value =
        make(42);

    println(
        "{}",
        value.hidden
    );
}
"#,
        );

    project.write(
        "data.hydra",
        r#"
pub struct Value {
    hidden: i32;
}

pub fn make(
    value: i32
) -> Value {
    return Value {
        .hidden = value
    };
}
"#,
    );

    let errors =
        analyze_project(
            &project
        )
        .expect_err(
            "external access to private field must fail",
        );

    assert!(
        errors.iter().any(
            |error| {
                error.code
                    == "S019"
                    && error.message.contains(
                        "field `hidden`"
                    )
                    && error.message.contains(
                        "is private"
                    )
            }
        ),
        "expected private-field error, got {errors:#?}",
    );
}


#[test]
fn private_field_construction_is_rejected_from_another_module() {
    let project =
        TestProject::new(
            r#"
include data::Value;

fn main() -> void {
    const value = Value {
        .hidden = 42
    };

    println(
        "{}",
        value
    );
}
"#,
        );

    project.write(
        "data.hydra",
        r#"
pub struct Value {
    hidden: i32;
}
"#,
    );

    let errors =
        analyze_project(
            &project
        )
        .expect_err(
            "external construction through private field must fail",
        );

    assert!(
        errors.iter().any(
            |error| {
                error.code
                    == "S019"
                    && error.message.contains(
                        "field `hidden`"
                    )
                    && error.message.contains(
                        "is private"
                    )
            }
        ),
        "expected private-field construction error, got {errors:#?}",
    );
}
