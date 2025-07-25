mod hydrac;

use hydrac::parse::lexer::Lexer;

use std::env;
use std::fs;
use std::process;
use std::path::Path;

fn print_help(program_name: &str) {
    println!(
        "\
Usage: {} <input.hydra> [-o output] [--tokens]

Flags:
  -o <file>       Specify output filename (default: <input>_output)
  --tokens        Save all tokens generated from <input.hydra> to a .tokens file
  --help          Show this help message

Examples:
  {} source.hydra
  {} source.hydra -o result.hydra
  {} source.hydra --tokens
",
        program_name, program_name, program_name, program_name
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 || args.iter().any(|a| a == "--help") {
        print_help(&args[0]);
        
        return;
    }

    if args.len() < 2 {
        eprintln!("Error: Missing input file");
        eprintln!("Run '{} --help' for help.", args[0]);

        process::exit(1);
    }

    let filename = &args[1];
    let filename_path = Path::new(filename);
    match filename_path.extension().and_then(|ext| ext.to_str()) {
        Some("hydra") => {}
        _ => {
            eprintln!("Error: '{}' is not a hydra file", filename);
            eprintln!("Run '{} --help' for help.", args[0]);

            process::exit(1);
        }
    }

    let mut output_file = None;
    let mut emit_tokens = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: -o flag requires an output filename");
                    eprintln!("Run '{} --help' for help.", args[0]);

                    process::exit(1);
                }
                output_file = Some(args[i + 1].clone());
                i += 2;
            }
            "--tokens" => {
                emit_tokens = true;
                i += 1;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Run '{} --help' for help.", args[0]);

                process::exit(1);
            }
        }
    }

    let contents = match fs::read_to_string(filename) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("Error reading file '{}': {}", filename, err);

            process::exit(1);
        }
    };

    let mut lexer = Lexer::new(&contents);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(err) => {
            eprintln!("Lexer error: {}", err);

            process::exit(1);
        }
    };

    if emit_tokens {
        let token_output = tokens
            .iter()
            .map(|t| format!("{:?}", t))
            .collect::<Vec<_>>()
            .join("\n");

        let token_filename = filename_path
            .with_extension("tokens")
            .to_string_lossy()
            .to_string();

        match fs::write(&token_filename, token_output) {
            Ok(_) => println!("Tokens written to: {}", token_filename),
            Err(err) => {
                eprintln!("Error writing tokens file '{}': {}", token_filename, err);

                process::exit(1);
            }
        }

        return; // Exit here since we only wanted to emit tokens
    }

    // If not --tokens, proceed with normal compile output
    let mut output = String::new();
    output.push_str("=== HYDRA COMPILER ===\n");
    output.push_str(&format!("Compiling file: {}\n\n", filename));
    output.push_str("=== TOKENIZING ===\n");
    output.push_str("Tokenization successful\n");
    output.push_str(&format!("Tokens found: {}\n", tokens.len()));

    let output_file = output_file.unwrap_or_else(|| {
        let input_stem = Path::new(filename)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap();

        format!("{}_output", input_stem)
    });

    match fs::write(&output_file, &output) {
        Ok(_) => println!("Output written to: {}", output_file),
        Err(err) => {
            eprintln!("Error writing output file '{}': {}", output_file, err);

            process::exit(1);
        }
    }
}
