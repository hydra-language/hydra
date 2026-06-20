use std::{env, fs, path::{Path, PathBuf}, process::{self, Command}};
use clap::{CommandFactory, Parser as ClapParser, ValueEnum};
use inkwell::{OptimizationLevel, context::Context};
use parser::program::Program;
use analyzer::Analyzer;
use codegen::CodeGen;
use mir::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum EmitStage {
    TOKENS,
    AST,
    HIR,
    MIR,
    IR,
    IROpt,
    ASM,
}

#[derive(ClapParser, Debug)]
#[command(name = "hydrac", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[arg(value_name = "INPUT")]
    input: Option<String>,

    #[arg(long, value_enum, value_delimiter = ',', help = "emit up to a specific compilation stage")]
    emit: Option<Vec<EmitStage>>,

    #[arg(long, help = "build with maximum optimizations")]
    release: bool,

    #[arg(short, long, value_name = "OUTPUT", help = "specify name of output file")]
    output: Option<String>
}

fn main() {
    let cli = Cli::parse();
    let emit_list: Vec<EmitStage> = cli.emit.clone().unwrap_or_default();

    let opt_level = if cli.release {
        OptimizationLevel::Aggressive
    } else {
        OptimizationLevel::None
    };

    if cli.input.is_none() {
        Cli::command().print_help().unwrap();
        println!();

        process::exit(1);
    }

    let input = cli.input.unwrap();
    let input_path = Path::new(&input);

    let module_name = cli.output.clone().unwrap_or_else(|| {
        input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string()
    });

    match input_path.extension().and_then(|e| e.to_str()) {
        Some("hydra") => {}
        _ => {
            eprintln!("error: '{}' is not a hydra file", input);
            process::exit(1);
        }
    }

    let contents = match fs::read_to_string(&input) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed while reading '{}': {}", input, e);
            process::exit(1);
        }
    };

    // ---------------- LOAD & PARSE PROGRAM ----------------

    let program = Program::build(input_path).unwrap_or_else(|e| {
        // FIX 1: Access the .message property of the returned HydraError
        eprintln!("build error: {}", e.message);
        process::exit(1);
    });

    if emit_list.contains(&EmitStage::TOKENS) {
        println!("info: skipping token dump (tokens are now handled internally by modules).");

        if cli.emit.is_some() { 
            return; 
        }
    }
        
    if emit_list.contains(&EmitStage::AST) {
        let fname = input_path.with_extension("nodes");
        let mut all_asts = String::new();
        
        for (name, module) in &program.modules {
            // FIX 2: Join the Vec<String> into a proper path string
            all_asts.push_str(&format!("--- MODULE: {} ---\n", name.join("::")));
            
            // FIX 3: Access the second element of the tuple (.1) to get the AST
            for node in &module.1 {
                all_asts.push_str(&format!("{:#?}\n\n", node));
            }
        }

        fs::write(&fname, all_asts).unwrap();
        println!("info: AST written to: {}", fname.display());

        if cli.emit.is_some() { 
            return; 
        }
    }

    // ---------------- ANALYZE ----------------

    let mut analyzer = Analyzer::new();
    
    let hir = analyzer.analyze(&program).unwrap_or_else(|errors| {
        for e in errors {
            // TODO: In the future, grab the correct source text from program.modules based on the error's module
            e.report(&contents, input_path.to_str().unwrap());
        }

        process::exit(1);
    });

    if emit_list.contains(&EmitStage::HIR) {
        let fname = input_path.with_extension("hir");
        fs::write(
            &fname,
            hir.functions
                .iter()
                .map(|f| format!("{}", f))
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
            .unwrap();

        println!("info: HIR written to: {}", fname.display());

        if cli.emit.is_some() { 
        }
    }

    let mut mir_functions = Vec::new();
    for hir_fn in &hir.functions {
        let builder = mir::builder::MIRBuilder::new(&analyzer.context);
        mir_functions.push(builder.build_function(hir_fn.clone()));
    }

    if emit_list.contains(&EmitStage::MIR) {
        let fname = input_path.with_extension("mir");
        fs::write(
            &fname,
            mir_functions
                .iter()
                .map(|f| format!("{}", f))
                .collect::<Vec<_>>()
                .join("\n\n"),
        ).unwrap();

        println!("info: MIR written to: {}", fname.display());

        if cli.emit.is_some() { 
        }
    }

    // ---------------- CODEGEN ----------------

//     let context = Context::create();
//     let mut codegen = CodeGen::new(&context, &module_name);
// 
//     codegen.generate(&hir).unwrap_or_else(|e| {
//         eprintln!("codegen error: {}", e);
//         process::exit(1);
//     });
// 
//     if emit_list.contains(&EmitStage::IR) {
//         let fname = input_path.with_extension("pre.ll");
// 
//         codegen.module.print_to_file(&fname).unwrap();
//         println!("info: LLVM ir written to: {}", fname.display());
// 
//         if cli.emit.is_some() { 
//             return; 
//         }
//     }
// 
//     if emit_list.contains(&EmitStage::IROpt) {
//         CodeGen::run_ir_passes(&codegen.module);
// 
//         let fname = input_path.with_extension("opt.ll");
//         codegen.module.print_to_file(&fname).unwrap();
//         println!("info: optimized LLVM ir written to: {}", fname.display());
// 
//         if cli.emit.is_some() {
//             return;
//         }
//     }
// 
//     if emit_list.contains(&EmitStage::ASM) {
//         let fname = input_path.with_extension("s");
// 
//         CodeGen::emit_asm(&codegen.module, &codegen.triple, OptimizationLevel::None, &fname);
//         println!("info: assembly written to: {}", fname.display());
// 
//         if cli.emit.is_some() {
//             return;
//         }
//     }
// 
//     let obj_file = PathBuf::from(format!("{}.o", module_name));
// 
//     if cli.release {
//         CodeGen::run_ir_passes(&codegen.module);
//     }
// 
//     CodeGen::emit_object(&codegen.module, &codegen.triple, opt_level, &obj_file);
// 
//     let exe_name = if cfg!(target_os = "windows") {
//         format!("{}.exe", module_name)
//     } else {
//         module_name.clone()
//     };
// 
//     if cfg!(target_os = "windows") {
//         Command::new("clang")
//             .arg(&obj_file)
//             .arg("-o")
//             .arg(&exe_name)
//             .status()
//             .expect("failed to link");
//     } else {
//         let runtime_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
//             .join("../runtime/arch");        
// 
//         let start_s = match env::consts::ARCH {
//             "x86_64" => runtime_dir.join("x86_64/start.s"),
//             "aarch64" => runtime_dir.join("aarch64/start.s"),
//             _ => unreachable!(),
//         };
// 
//         if !start_s.exists() {
//             eprintln!("runtime start file not found: {}", start_s.display());
//             process::exit(1);
//         }
// 
//         let start_o = start_s.with_extension("o");
// 
//         let needs_rebuild = !start_o.exists()
//         || start_o.metadata().unwrap().modified().unwrap() 
//         < start_s.metadata().unwrap().modified().unwrap();
// 
//         if needs_rebuild {
//             Command::new("as")
//                 .arg(&start_s)
//                 .arg("-o")
//                 .arg(&start_o)
//                 .status()
//                 .expect("failed to assemble start.s");
//         }
// 
//         let dynamic_linker = if env::consts::ARCH == "x86_64" { 
//             "/lib64/ld-linux-x86-64.so.2" 
//         } else { 
//             "/lib/ld-linux-aarch64.so.1" 
//         };
// 
//         Command::new("ld")
//             .arg("--pie")
//             .arg("-o")
//             .arg(&exe_name)
//             .arg(&start_o)
//             .arg(&obj_file)
//             .arg("-dynamic-linker")
//             .arg(dynamic_linker).arg("-lc")
//             .status()
//             .unwrap();
// 
//         for file in [&obj_file] {
//             if let Err(e) = fs::remove_file(file) {
//                 eprintln!("warning: could not remove file '{}': {}", file.display(), e);
//             }
//         }
//     }
}
