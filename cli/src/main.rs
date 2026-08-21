use std::{env, fs, path::{Path, PathBuf}, process::{self, Command}};
use clap::{CommandFactory, Parser as ClapParser, ValueEnum};
use inkwell::{OptimizationLevel, context::Context};

use parser::program::Program;

// NEW: Import Resolver and HIRContext
use analyzer::{Analyzer, Resolver, monomorphizer::Monomorphizer};
use ir::context::HIRContext;

use mir::{builder::MIRBuilder, MIRProgram, optimizer::Optimizer};
use borrowcheck::borrowcheck::BorrowChecker;
use codegen::CodeGen;

const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum EmitStage {
    AST,
    HIR,
    MIR,
    MIROpt,
    IR,
    IROpt,
    ASM,
}

#[derive(ClapParser, Debug)]
#[command(name = "hydrac", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[arg(value_name = "INPUT")]
    input: Option<String>,

    #[arg(long, value_enum, value_delimiter = ',', help = "comma separated list of stages to emit")]
    emit: Option<Vec<EmitStage>>,

    #[arg(long, help = "build with maximum optimizations")]
    release: bool,

    #[arg(short, long, value_name = "OUTPUT", help = "specify name of output file")]
    output: Option<String>
}

fn main() {
    let cli = Cli::parse();

    if cli.input.is_none() {
        Cli::command().print_help().unwrap();
        println!();
        process::exit(1);
    }

    let emit_list: Vec<EmitStage> = cli.emit.clone().unwrap_or_default();

    let opt_level = if cli.release {
        OptimizationLevel::Aggressive
    } else {
        OptimizationLevel::None
    };

    let input = cli.input.unwrap();
    let input_path = Path::new(&input);

    let module_name = cli.output.clone().unwrap_or_else(|| {
        input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string()
    });

    match input_path.extension().and_then(|e| e.to_str()) {
        Some("hydra") => {}
        _ => {
            eprintln!("{}[ERROR]{} '{}' is not a hydra file", RED, RESET, input);
            process::exit(1);
        }
    }

    let contents = match fs::read_to_string(&input) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}[ERROR]{} failed while reading '{}': {}", RED, RESET, input, e);
            process::exit(1);
        }
    };

    // --- PHASE 1: PARSER ---
    let program = Program::build(input_path).unwrap_or_else(|e| {
        eprintln!("{}[ERROR]{} build error: {}", RED, RESET, e.message);
        process::exit(1);
    });
        
    if emit_list.contains(&EmitStage::AST) {
        let fname = input_path.with_extension("nodes");
        let mut all_asts = String::new();
        for (name, module) in &program.modules {
            all_asts.push_str(&format!("--- MODULE: {} ---\n", name.join("::")));
            for node in &module.1 {
                all_asts.push_str(&format!("{:#?}\n\n", node));
            }
        }
        fs::write(&fname, all_asts).unwrap();
        println!("{}[INFO]{} AST nodes written to: {}", GREEN, RESET, fname.display());
    }

    // --- PHASE 2: RESOLUTION & SEMANTIC ANALYSIS ---
    
    // We instantiate the context here so it lives for the entire compilation
    let mut context = HIRContext::default(); 

    // Pass 2a: Name Resolution
    let resolver = Resolver::new(&program, &mut context);
    let (name_resolver, global_symbols) = resolver.resolve().unwrap_or_else(|errors| {
        for e in errors {
            e.report(&contents, input_path.to_str().unwrap());
        }
        process::exit(1);
    });

    // Pass 2b: Semantic Analysis (Type Checking)
    let analyzer = Analyzer::new(&program, &mut context, name_resolver, global_symbols);
    let hir = analyzer.analyze().unwrap_or_else(|errors| {
        for e in errors {
            e.report(&contents, input_path.to_str().unwrap());
        }
        process::exit(1);
    });

    if emit_list.contains(&EmitStage::HIR) {
        let fname = input_path.with_extension("hir");
        fs::write(
            &fname,
            hir.functions.iter().map(|f| format!("{}", f)).collect::<Vec<_>>().join("\n\n"),
        ).unwrap();
        println!("{}[INFO]{} HIR written to: {}", GREEN, RESET, fname.display());
    }

    // --- PHASE 3: MONOMORPHIZATION ---
    let monomorphizer = Monomorphizer::new(&mut context, hir);
    let specialized_program = monomorphizer.run();

    // --- PHASE 4: MIR LOWERING ---
    let mut mir_functions = Vec::new();
    for hir_fn in &specialized_program.functions {
        let builder = MIRBuilder::new(&context);
        mir_functions.push(builder.build_function(hir_fn.clone()));
    }
    
    let mut mir_program = MIRProgram { functions: mir_functions };

    if emit_list.contains(&EmitStage::MIR) {
        let fname = input_path.with_extension("mir");
        fs::write(
            &fname,
            mir_program.functions.iter().map(|f| format!("{}", f)).collect::<Vec<_>>().join("\n\n"),
        ).unwrap();
        println!("{}[INFO]{} MIR written to: {}", GREEN, RESET, fname.display());
    }

    // --- PHASE 5: BORROW CHECKING ---
    let mut has_borrow_errors = false;
    for mir_fn in &mir_program.functions {
        let mut checker = BorrowChecker::new(mir_fn, &context);
        if let Err(errors) = checker.check() {
            has_borrow_errors = true;
            for error in errors {
                error.report(&contents, input_path.to_str().unwrap());
            }
        }
    }

    if has_borrow_errors {
        process::exit(1);
    }

    // --- PHASE 6: MIR OPTIMIZATION ---
    if cli.release || emit_list.contains(&EmitStage::MIROpt) {
        Optimizer::optimize(&mut mir_program);
    }

    if emit_list.contains(&EmitStage::MIROpt) {
        let fname = input_path.with_extension("opt.mir");
        fs::write(
            &fname,
            mir_program.functions.iter().map(|f| format!("{}", f)).collect::<Vec<_>>().join("\n\n"),
        ).unwrap();
        println!("{}[INFO]{} Optimized MIR written to: {}", GREEN, RESET, fname.display());
    }

    // --- STOP CHECK (FRONTEND ONLY) ---
    let needs_backend = emit_list.contains(&EmitStage::IR) 
                     || emit_list.contains(&EmitStage::IROpt) 
                     || emit_list.contains(&EmitStage::ASM)
                     || cli.emit.is_none(); 

    if !needs_backend {
        return;
    }

    // --- PHASE 7: CODEGEN (LLVM IR) ---
    let llvm_context = Context::create();
    let mut codegen = CodeGen::new(&llvm_context, &context, &module_name);
    
    codegen.generate(&mir_program).unwrap_or_else(|e| {
        eprintln!("{}[ERROR]{} codegen error: {}", RED, RESET, e);
        process::exit(1);
    });

    if emit_list.contains(&EmitStage::IR) {
        let fname = input_path.with_extension("pre.ll");
        codegen.module.print_to_file(&fname).unwrap();
        println!("{}[INFO]{} LLVM ir written to: {}", GREEN, RESET, fname.display());
    }

    // --- PHASE 8: IR OPTIMIZATION ---
    let mut ir_optimized = false;
    if cli.release || emit_list.contains(&EmitStage::IROpt) {
        CodeGen::run_ir_passes(&codegen.module);
        ir_optimized = true;
    }

    if emit_list.contains(&EmitStage::IROpt) {
        let fname = input_path.with_extension("opt.ll");
        codegen.module.print_to_file(&fname).unwrap();
        println!("{}[INFO]{} optimized LLVM ir written to: {}", GREEN, RESET, fname.display());
    }

    // --- PHASE 9: ASSEMBLY ---
    if emit_list.contains(&EmitStage::ASM) {
        let fname = input_path.with_extension("s");
        CodeGen::emit_asm(&codegen.module, &codegen.triple, opt_level, &fname);
        println!("{}[INFO]{} assembly written to: {}", GREEN, RESET, fname.display());
    }

    // --- FINAL STOP CHECK ---
    if cli.emit.is_some() {
        return;
    }

    // --- PHASE 10: OBJECT COMPILATION & LINKING ---
    let obj_file = PathBuf::from(format!("{}.o", module_name));
    
    if cli.release && !ir_optimized {
        CodeGen::run_ir_passes(&codegen.module);
    }
    
    CodeGen::emit_object(&codegen.module, &codegen.triple, opt_level, &obj_file);

    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", module_name)
    } else {
        module_name.clone()
    };

    if cfg!(target_os = "windows") {
        Command::new("clang")
            .arg(&obj_file)
            .arg("-o")
            .arg(&exe_name)
            .status()
            .expect("failed to link");
    } else {
        let runtime_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/arch");
        let start_s = match env::consts::ARCH {
            "x86_64" => runtime_dir.join("x86_64/start.s"),
            "aarch64" => runtime_dir.join("aarch64/start.s"),
            _ => unreachable!(),
        };

        if !start_s.exists() {
            eprintln!("{}[ERROR]{} runtime start file not found: {}", RED, RESET, start_s.display());
            process::exit(1);
        }

        let start_o = start_s.with_extension("o");
        let needs_rebuild = !start_o.exists()
            || start_o.metadata().unwrap().modified().unwrap() < start_s.metadata().unwrap().modified().unwrap();

        if needs_rebuild {
            Command::new("as")
                .arg(&start_s)
                .arg("-o")
                .arg(&start_o)
                .status()
                .expect("failed to assemble start.s");
        }

        Command::new("ld")
            .arg("-o")
            .arg(&exe_name)
            .arg(&start_o)
            .arg(&obj_file)
            .status()
            .unwrap();

        let _ = fs::remove_file(&obj_file);
    }
}
