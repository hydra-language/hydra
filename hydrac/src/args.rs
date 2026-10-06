use std::{env, path::PathBuf};

use clap::{
    ArgAction, Parser, ValueEnum,
};
use driver::{
    CompileMode,
    CompilerOptions,
    EmitKind,
    OptLevel,
    options::TargetMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TargetArg {
    #[value(name = "generic")]
    Generic,

    #[value(name = "native")]
    Native,
}

impl From<TargetArg> for TargetMode {
    fn from(target: TargetArg) -> Self {
        match target {
            TargetArg::Generic => Self::Generic,
            TargetArg::Native => Self::Native,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EmitStage {
    #[value(name = "ast")]
    Ast,

    #[value(name = "hir")]
    Hir,

    #[value(name = "mir")]
    Mir,

    #[value(name = "mir-opt")]
    MirOpt,

    #[value(name = "llvm-ir", alias = "ir")]
    LlvmIr,

    #[value(name = "llvm-ir-opt", alias = "ir-opt")]
    LlvmIrOpt,

    #[value(name = "asm")]
    Asm,

    #[value(name = "obj", alias = "object")]
    Object,

}

impl From<EmitStage> for EmitKind {
    fn from(stage: EmitStage) -> Self {
        match stage {
            EmitStage::Ast => Self::Ast,
            EmitStage::Hir => Self::Hir,
            EmitStage::Mir => Self::Mir,
            EmitStage::MirOpt => Self::MirOpt,
            EmitStage::LlvmIr => Self::LlvmIr,
            EmitStage::LlvmIrOpt => Self::LlvmIrOpt,
            EmitStage::Asm => Self::Asm,
            EmitStage::Object => Self::Object,
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "hydrac",
    version = env!("CARGO_PKG_VERSION")
)]
pub struct Cli {
    #[arg(value_name = "INPUT")]
    pub input: Option<PathBuf>,

    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        help = "emit additional compiler artifacts"
    )]
    pub emit: Vec<EmitStage>,

    #[arg(
        long,
        conflicts_with = "no_link",
        help = "check the program without running LLVM or linking"
    )]
    pub check: bool,

    #[arg(
        long,
        help = "compile through object code without linking an executable"
    )]
    pub no_link: bool,

    #[arg(
        long,
        conflicts_with = "opt_level",
        help = "build with -O3 optimizations"
    )]
    pub release: bool,

    #[arg(
        short = 'O',
        long = "opt-level",
        value_name = "LEVEL",
        value_parser = clap::value_parser!(u8).range(0..=3),
        help = "set optimization level (0-3)"
    )]
    pub opt_level: Option<u8>,

    #[arg(
        long,
        value_enum,
        value_name = "TARGET",
        help = "select code generation target (generic or native)"
    )]
    pub target: Option<TargetArg>,

    #[arg(
        short,
        long,
        value_name = "OUTPUT",
        help = "specify output file"
    )]
    pub output: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        help = "override the Hydra sysroot"
    )]
    pub sysroot: Option<PathBuf>,

    #[arg(
        long,
        value_name = "LINKER",
        help = "use the specified linker instead of the system default"
    )]
    pub linker: Option<String>,

    #[arg(
        long,
        help = "print compiler phase timings"
    )]
    pub time_passes: bool,

    #[arg(
        short = 'v',
        long = "verbose",
        action = ArgAction::Count,
        help = "increase compiler verbosity (-vv for per-function details)"
    )]
    pub verbose: u8,
}

impl Cli {

    pub fn into_options(self) -> Result<Option<CompilerOptions>, String> {
        let Some(input) = self.input else {
            return Ok(None);
        };

        let opt_level = if self.release {
            OptLevel::O3
        } else {
            OptLevel::from_u8(
                self.opt_level.unwrap_or(0)
            ).expect(
                "clap validated optimization level"
            )
        };

        let mode = if self.check {
            CompileMode::Check
        } else if self.no_link {
            CompileMode::NoLink
        } else {
            CompileMode::Build
        };

        let linker = Cli::resolve_linker(self.linker, env::var("HYDRA_LINKER").ok());
        let target = Cli::resolve_target(self.target, env::var("HYDRA_TARGET").ok())?;

        Ok(Some(CompilerOptions {
            input,
            output: self.output,
            sysroot: self.sysroot,
            linker,
            emit: self.emit.into_iter().map(EmitKind::from).collect(),
            mode,
            opt_level,
            target,
            time_passes: self.time_passes,
            verbosity: self.verbose
        }))
    }

    pub(crate) fn resolve_linker(cli: Option<String>, environment: Option<String>) -> Option<String> {
        cli.or_else(|| environment.filter(|linker| !linker.is_empty()))
    }

    fn resolve_target(cli: Option<TargetArg>, environment: Option<String>) -> Result<TargetMode, String> {
        if let Some(target) = cli {
            return Ok(target.into());
        }

        let Some(environment) = environment.filter(|target| !target.is_empty()) else {
            return Ok(TargetMode::Generic);
        };

        let target = <TargetArg as ValueEnum>::from_str(&environment, true).map_err(|_| {
            format!(
                "invalid HYDRA_TARGET `{}`; expected `generic` or `native`",
                environment,
            )
        })?;

        Ok(target.into())
    }
}
