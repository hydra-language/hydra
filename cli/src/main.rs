use std::{env, fs, path::{Path, PathBuf}, process::{self, Command}};

use clap::{ArgAction, CommandFactory, Parser as ClapParser};
use inkwell::context::Context;

use lexer::Lexer;
use parser::parser::Parser;
use analyzer::Analyzer;
use codegen::CodeGen;

#[derive(ClapParser, Debug)]
#[command(name = "hydrac", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[arg(value_name = "INPUT")]
    input: Option<String>,

    #[arg(long, action = ArgAction::SetTrue, help = "emit tokens to a .tokens file")]
    tokens: bool,

    #[arg(long, action = ArgAction::SetTrue, help = "emit ast nodes to a .nodes file")]
    ast: bool,

    #[arg(long, action = ArgAction::SetTrue, help = "emit typed ir to a .hir file")]
    hir: bool,

    #[arg(long, action = ArgAction::SetTrue, help = "emit llvm ir to a .ll file")]
    ir: bool,

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

    let mut lexer = Lexer::new(&contents);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lexer error: {}", e);
            process::exit(1);
        }
    };

    if cli.tokens {
        let token_output = tokens.iter()
            .map(|t| format!("{:?}", t))
            .collect::<Vec<_>>()
            .join("\n");

        let token_filename = input_path.with_extension("tokens").to_string_lossy().into_owned();
        if let Err(e) = fs::write(&token_filename, token_output) {
            eprintln!("error: write to tokens file '{}' failed: {}", token_filename, e);
            process::exit(1);
        }

        println!("info: tokens written to: {}", token_filename);
    }

    let mut parser = Parser::new(tokens);
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(errors) => {
            for error in errors {
                error.report(&contents, &input);
            }

            process::exit(1);
        }
    };

    if cli.ast {
        let ast_ouput = ast.iter()
            .map(|node| format!("{:#?}", node))
            .collect::<Vec<_>>()
            .join("\n\n");

        let ast_filename = input_path.with_extension("nodes").to_string_lossy().into_owned();
        if let Err(e) = fs::write(&ast_filename, ast_ouput) {
            eprintln!("error: writing nodes to ast file '{}' failed: {}", ast_filename, e);
            process::exit(1);
        }

        println!("info: ast nodes written to: {}", ast_filename);
    }

    let mut analyzer = Analyzer::new();
    let ir = match analyzer.analyze(ast) {
        Ok(ir) => ir,
        Err(errors) => {
            for error in errors {
                error.report(&contents, &input);
            }

            process::exit(1);
        }
    };

    if cli.hir {
        let ir_output = ir.functions.iter()
            .map(|func| format!("{}", func)) // Changed 'stmt' to 'func' for clarity
            .collect::<Vec<_>>()
            .join("\n\n");

        let filename = input_path.with_extension("hir").to_string_lossy().into_owned();

        if let Err(e) = fs::write(&filename, ir_output) {
            eprintln!("error: writing ir to file '{}' failed: {}", filename, e);
            process::exit(1);
        }

        println!("info: hydra ir written to: {}", filename);
    }

    let context = Context::create();
    let mut codegen = CodeGen::new(&context, &module_name);

    if let Err(e) = codegen.generate(&ir) {
        eprintln!("codegen error: {}", e);
        process::exit(1);
    }

    let ll_file = input_path.with_extension("ll");
    let ir_output = codegen.ir_to_string();
    fs::write(&ll_file, ir_output).unwrap_or_else(|e| {
        eprintln!("error: writing ir to file '{}' failed: {}", ll_file.display(), e);
        process::exit(1);
    });

    if cli.ir {
        println!("info: ir written to file: {}", ll_file.display());
        return;
    }

    let obj_file = PathBuf::from(format!("{}.o", module_name));
    let clang_status = Command::new("clang")
        .args([ll_file.to_str().unwrap(), "-c", "-o", obj_file.to_str().unwrap(), "-O2", "-Wno-override-module"])
        .status()
        .expect("error: failed to run clang\nhelp: try installing a clang compiler");

    if !clang_status.success() {
        eprintln!("error: failed to run clang\nhelp: try installing a clang compiler");
        process::exit(1);
    }

    let exe_path = env::current_exe().expect("error: failed to get current executable path");

    let runtime_dir = exe_path.parent()
        .expect("error: failed to locate directory hydrac is located in")
        .join("../../runtime/arch");

    let arch = env::consts::ARCH;

    let start_s = match arch {
        "x86_64" => runtime_dir.join("x86_64/start.s"),
        "aarch64" => runtime_dir.join("arm/start.s"),
        _ => {
            eprintln!("error: unsupported architecture: '{}'", arch);
            process::exit(1);
        }
    };

    let start_o = start_s.with_extension("o");

    if !start_o.exists() || start_o.metadata().unwrap().modified().unwrap() 
        < start_s.metadata().unwrap().modified().unwrap() 
    {
        let mut assemble_cmd = Command::new("as");
        if arch == "x86_64" {
            assemble_cmd.arg("--64");
        }

        let status = assemble_cmd.arg(&start_s)
            .arg("-o")
            .arg(&start_o)
            .status()
            .expect("error: runtime linking failed");

        if !status.success() {
            eprintln!("error: runtime linking failed");
            process::exit(1);
        }
    }

    let dynamic_linker = match arch {
        "x86_64" => "/lib64/ld-linux-x86-64.so.2",
        "aarch64" => "/lib/ld-linux-aarch64.so.1",
        _ => unreachable!()
    };

    let linker_status = Command::new("ld")
        .arg("--pie")
        .arg("-o")
        .arg(&module_name)
        .arg(&start_o)
        .arg(&obj_file)
        .arg("-dynamic-linker")
        .arg(dynamic_linker)
        .arg("-lc")
        .status()
        .expect("error: linking against libc failed");


    if !linker_status.success() {
        eprintln!("error: linking against libc failed");
        process::exit(1);
    }

    for file in [&obj_file, &ll_file, &start_o] {
        if let Err(e) = fs::remove_file(file) {
            eprintln!("warning: could not remove object file '{}': {}", file.display(), e);
        }
    }
}
