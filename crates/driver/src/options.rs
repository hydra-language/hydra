use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileMode {
    Build,
    Check,
    NoLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmitKind {
    Ast,
    Hir,
    Mir,
    MirOpt,
    LlvmIr,
    LlvmIrOpt,
    Asm,
    Object,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptLevel {
    O0,
    O1,
    O2,
    O3,
}

impl OptLevel {
    pub fn from_u8(level: u8) -> Option<Self> {
        match level {
            0 => Some(Self::O0),
            1 => Some(Self::O1),
            2 => Some(Self::O2),
            3 => Some(Self::O3),
            _ => None,
        }
    }

    pub fn optimizes_mir(self) -> bool {
        self != Self::O0
    }

    pub fn optimizes_llvm(self) -> bool {
        self != Self::O0
    }
}

#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub sysroot: Option<PathBuf>,
    pub linker: Option<String>,
    pub emit: Vec<EmitKind>,
    pub mode: CompileMode,
    pub opt_level: OptLevel,
    pub time_passes: bool,
    pub verbosity: u8,
}

impl CompilerOptions {

    pub fn verbose(&self, level: u8) -> bool {
        self.verbosity >= level
    }

    pub fn emits(&self, kind: EmitKind) -> bool {
        self.emit.contains(&kind)
    }

    pub fn should_optimize_mir(&self) -> bool {
        self.opt_level.optimizes_mir()
    }

    pub fn should_optimize_llvm(&self) -> bool {
        self.opt_level.optimizes_llvm()
    }

    pub fn output_path(&self) -> PathBuf {
        self.output.clone().unwrap_or_else(|| {
            self.input
                .file_stem()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("output"))
        })
    }

    pub fn module_name(&self) -> String {
        self.output_path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("output")
            .to_string()
    }

    pub fn keeps_object(&self) -> bool {
        self.mode == CompileMode::NoLink || self.emits(EmitKind::Object)
    }

    pub fn runtime_profile(&self) -> &'static str {
        if self.opt_level == OptLevel::O0 {
            "debug"
        } else {
            "release"
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.input.extension().and_then(|extension| extension.to_str()) != Some("hydra") {
            return Err(format!("'{}' is not a hydra file", self.input.display()));
        }

        if self.mode == CompileMode::Check && self.emit.iter().any(|kind| matches!(kind, EmitKind::LlvmIr | EmitKind::LlvmIrOpt | EmitKind::Asm | EmitKind::Object))
        {
            return Err("`--check` cannot be combined with backend emit stages".to_string());
        }

        if self.opt_level == OptLevel::O0 {
            if self.emits(EmitKind::MirOpt) {
                return Err(
                    "`--emit mir-opt` requires optimization to be enabled; \
                        pass `-O1`, `-O2`, `-O3`, or `--release`"
                        .to_string(),
                );
            }

            if self.emits(EmitKind::LlvmIrOpt) {
                return Err(
                    "`--emit llvm-ir-opt` requires optimization to be enabled; \
                        pass `-O1`, `-O2`, `-O3`, or `--release`"
                        .to_string(),
                );
            }
        }

        Ok(())
    }
}
