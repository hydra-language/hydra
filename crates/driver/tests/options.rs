use std::path::PathBuf;

use driver::{
    CompileMode,
    CompilerOptions,
    EmitKind,
    OptLevel,
};

fn options() -> CompilerOptions {
    CompilerOptions {
        input: PathBuf::from("main.hydra"),
        output: None,
        sysroot: None,
        emit: vec![],
        mode: CompileMode::Build,
        opt_level: OptLevel::O0,
        time_passes: false,
    }
}

#[test]
fn optimized_emit_stages_enable_their_corresponding_passes() {
    let mut options = options();

    assert!(!options.should_optimize_mir());
    assert!(!options.should_optimize_llvm());

    options.emit.push(EmitKind::MirOpt);

    assert!(options.should_optimize_mir());
    assert!(!options.should_optimize_llvm());

    options.emit.push(EmitKind::LlvmIrOpt);

    assert!(options.should_optimize_llvm());
}

#[test]
fn optimization_level_enables_mir_and_llvm_optimization() {
    let mut options = options();

    options.opt_level = OptLevel::O2;

    assert!(options.should_optimize_mir());
    assert!(options.should_optimize_llvm());
    assert_eq!(options.runtime_profile(), "release");
}

#[test]
fn check_mode_rejects_backend_emit_stages() {
    let mut options = options();

    options.mode = CompileMode::Check;
    options.emit = vec![EmitKind::LlvmIr];

    let error = options
        .validate()
        .expect_err(
            "check mode should reject backend emission"
        );

    assert!(error.contains("--check"));
}

#[test]
fn no_link_mode_keeps_the_object_file() {
    let mut options = options();

    options.mode = CompileMode::NoLink;

    assert!(options.keeps_object());
}
