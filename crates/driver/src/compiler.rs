use std::{collections::HashMap, fs, path::PathBuf, time::Instant};

use analyzer::{Analyzer, Resolver, monomorphizer::Monomorphizer};

use borrowcheck::borrowcheck::BorrowChecker;

use codegen::CodeGen;

use ir::context::{DefID, SemanticContext};
use errors::error::HydraError;

use mir::{
    MIRProgram, MIRStruct, bounds::BoundsChecker, builder::MIRBuilder, optimizer::{OptimizationLevel as MirOptimizationLevel, Optimizer},
};

use parser::module::{
    ModuleTree,
    SourceMap,
};

use inkwell::{OptimizationLevel, context::Context, targets::TargetMachine};

use crate::{
    emit, error::DriverError, linker, options::{
        CompileMode, CompilerOptions, EmitKind, OptLevel, TargetMode,
    }, session::Session, style::{GREEN, RESET}, sysroot::Sysroot
};

pub struct Compiler {
    session: Session,
}

fn attach_function_source(errors: &mut [HydraError], def_id: DefID, function_sources: &HashMap<DefID, u32>, source_map: &SourceMap) {
    let Some(source_id) = function_sources.get(&def_id) else { 
        return; 
    };

    let Some(path) = source_map.path_for_id(*source_id) else { 
        return; 
    };

    let Some(source) = source_map.get_source(path) else { 
        return; 
    };

    let filename = path.display().to_string();

    for error in errors {
        if error.filepath.is_none() { 
            error.filepath = Some(filename.clone()); 
        }

        if error.source_text.is_none() { 
            error.source_text = Some(source.to_owned()); 
        }
    }
}

impl Compiler {

    pub fn new(options: CompilerOptions) -> Result<Self, DriverError> {
        options.validate().map_err(DriverError::message)?;

        let sysroot = Sysroot::discover(options.sysroot.clone()).map_err(DriverError::message)?;

        Ok(Self {
            session: Session::new(
                options,
                sysroot,
            ),
        })
    }

    pub fn compile(&mut self) -> Result<(), DriverError> {
        let result = self.compile_inner();

        self.session.timings.print();

        result
    }

    fn compile_inner(&mut self) -> Result<(), DriverError> {
        let input = self.session.options.input.clone();

        let contents = fs::read_to_string(&input).map_err(|error| {
            DriverError::io("failed while reading", &input, error)
        })?;

        let mut source_map = SourceMap::new();

        let timer = self.session.timings.start(
            "parse + imports"
        );

        let mut program = ModuleTree::build(&input, self.session.sysroot.stdlib(), &mut source_map).map_err(|errors| {
            DriverError::diagnostics(errors, contents.clone(), input.clone())
        })?;

        program.resolve_imports(&mut source_map).map_err(|errors| {
            DriverError::diagnostics(errors, contents.clone(), input.clone())
        })?;

        program.parse_bodies(&source_map).map_err(|errors| {
            DriverError::diagnostics(errors, contents.clone(), input.clone())
        })?;

        self.session.timings.finish(timer);

        if self.session.options.emits(EmitKind::Ast) {
            let mut all_asts = String::new();

            for (filepath, (_, items)) in &program.parsed_files {
                all_asts.push_str(
                    &format!(
                        "--- FILE: {} ---\n",
                        filepath.display(),
                    )
                );

                for node in items {
                    all_asts.push_str(
                        &format!("{:#?}\n\n", node)
                    );
                }
            }

            emit::write_text(
                &input.with_extension("nodes"),
                &all_asts,
                "AST nodes",
            )?;
        }

        let mut context = SemanticContext::default();

        let timer = self.session.timings.start(
            "name resolution"
        );

        let resolver = Resolver::new(
            &program,
            &mut context,
            &source_map,
        );

        let (name_resolver, global_symbols) = resolver.resolve().map_err(|errors| {
            DriverError::diagnostics(errors, contents.clone(), input.clone())
        })?;

        self.session.timings.finish(timer);

        let timer = self.session.timings.start(
            "semantic analysis"
        );

        let analyzer = Analyzer::new(
            &program,
            context,
            &source_map,
            name_resolver,
            global_symbols,
        );

        let analyzed = analyzer.analyze().map_err(|errors| {
            DriverError::diagnostics(
                errors,
                contents.clone(),
                input.clone(),
            )
        })?;

        self.session.timings.finish(timer);

        if self.session.options.verbose(1) {
            emit::info(
                format!(
                    "{} HIR functions",
                    analyzed.hir.functions.len(),
                )
            );
        }

        if self.session.options.emits(EmitKind::Hir) {
            let hir_text = format!("{}", analyzed.hir.display());

            emit::write_text(
                &input.with_extension("hir"),
                &hir_text,
                "HIR",
            )?;
        }

        let timer = self.session.timings.start(
            "monomorphization"
        );

        let monomorphizer = Monomorphizer::new(analyzed);
        let specialized_program = monomorphizer.run();

        self.session.timings.finish(timer);

        if self.session.options.verbose(1) {
            emit::info(
                format!(
                    "{} reachable specialized functions",
                    specialized_program.functions.len(),
                )
            );
        }

        let timer = self.session.timings.start(
            "MIR lowering"
        );

        let mut mir_functions = Vec::with_capacity(
            specialized_program.functions.len()
        );

        let mut drop_cache = HashMap::new();
        let mut function_sources = HashMap::new();

        for hir_fn in &specialized_program.functions {
            let function_start = Instant::now();

            let builder = MIRBuilder::new(
                &specialized_program.type_info,
                &mut drop_cache,
            );

            let mir_fn = builder.build_function(hir_fn.clone());
            function_sources.insert(mir_fn.def_id, hir_fn.source_id);
            mir_functions.push(mir_fn);

            let function_name = hir_fn.display_name.as_deref().unwrap_or(&hir_fn.name).to_string();

            self.session.timings.record_item(
                "MIR lowering",
                function_name,
                function_start.elapsed(),
            );
        }

        let mir_structs = specialized_program.type_info.structs.values()
            .map(|info| MIRStruct {
                type_ref: info.type_ref.clone(),
                fields: info.fields.clone(),
            })
            .collect();

        let mut mir_program = MIRProgram {
            functions: mir_functions,
            structs: mir_structs,
        };

        self.session.timings.finish(timer);

        if self.session.options.verbose(1) {
            emit::info(
                format!(
                    "{} MIR functions",
                    mir_program.functions.len(),
                )
            );
        }

        if self.session.options.emits(EmitKind::Mir) {
            emit::write_text(
                &input.with_extension("mir"),
                &format_mir(&mir_program),
                "MIR",
            )?;
        }

        let timer = self.session.timings.start(
            "bounds checking"
        );

        let mut bounds_errors = Vec::new();

        for mir_fn in &mir_program.functions {
            let function_start = Instant::now();
            let checker = BoundsChecker::new(mir_fn);
            let result = checker.check();

            self.session.timings.record_item(
                "bounds checking",
                mir_fn.display_name.as_deref().unwrap_or(&mir_fn.name),
                function_start.elapsed(),
            );

            if let Err(mut errors) = result {
                attach_function_source(&mut errors, mir_fn.def_id, &function_sources, &source_map);
                bounds_errors.append(&mut errors);
            }
        }

        self.session.timings.finish(timer);

        if !bounds_errors.is_empty() {
            return Err(
                DriverError::diagnostics(
                    bounds_errors,
                    contents,
                    input,
                )
            );
        }

        let timer = self.session.timings.start(
            "borrow checking"
        );

        let mut borrow_errors = Vec::new();

        for mir_fn in &mir_program.functions {
            let function_start = Instant::now();
            let mut checker = BorrowChecker::new(mir_fn);
            let result = checker.check();

            self.session.timings.record_item(
                "borrow checking",
                mir_fn.display_name.as_deref().unwrap_or(&mir_fn.name),
                function_start.elapsed(),
            );

            if let Err(mut errors) = result {
                attach_function_source(&mut errors, mir_fn.def_id, &function_sources, &source_map);
                borrow_errors.append(&mut errors);
            }
        }

        self.session.timings.finish(timer);

        if !borrow_errors.is_empty() {
            return Err(
                DriverError::diagnostics(
                    borrow_errors,
                    contents,
                    input,
                )
            );
        }

        if self.session.options.should_optimize_mir() {
            let timer = self.session.timings.start(
                "MIR optimization"
            );

            Optimizer::optimize(&mut mir_program, mir_opt_level(self.session.options.opt_level));

            self.session.timings.finish(timer);
        }

        if self.session.options.emits(EmitKind::MirOpt) {
            emit::write_text(
                &input.with_extension("opt.mir"),
                &format_mir(&mir_program),
                "optimized MIR",
            )?;
        }

        if self.session.options.mode == CompileMode::Check {
            return Ok(());
        }

        let module_name = self.session.options.module_name();
        let llvm_context = Context::create();

        let (cpu, features) = llvm_target(
            self.session.options.target,
        );

        let timer = self.session.timings.start(
            "LLVM codegen"
        );

        let mut codegen = CodeGen::new(
            &llvm_context,
            &module_name,
            cpu,
            features,
        );

        codegen.generate(&mir_program).map_err(DriverError::message)?;

        self.session.timings.finish(timer);

        if self.session.options.emits(EmitKind::LlvmIr) {
            let path = input.with_extension("pre.ll");

            codegen
                .module
                .print_to_file(&path)
                .map_err(|error| {
                    DriverError::message(
                        format!(
                            "failed to emit LLVM IR: {}",
                            error,
                        )
                    )
                })?;

            println!(
                "{}[INFO]{} LLVM IR written to: {}",
                GREEN,
                RESET,
                path.display(),
            );
        }

        if self.session.options.should_optimize_llvm() {
            let timer = self.session.timings.start(
                "LLVM optimization"
            );

            CodeGen::run_ir_passes(&codegen.module, llvm_opt_level(self.session.options.opt_level));

            self.session.timings.finish(timer);
        }

        if self.session.options.emits(EmitKind::LlvmIrOpt) {
            let path = input.with_extension("opt.ll");

            codegen
                .module
                .print_to_file(&path)
                .map_err(|error| {
                    DriverError::message(
                        format!(
                            "failed to emit optimized LLVM IR: {}",
                            error,
                        )
                    )
                })?;

            println!(
                "{}[INFO]{} optimized LLVM IR written to: {}",
                GREEN,
                RESET,
                path.display(),
            );
        }

        let optimization_level = llvm_opt_level(self.session.options.opt_level);

        if self.session.options.emits(EmitKind::Asm) {
            let path = input.with_extension("s");

            CodeGen::emit_asm(
                &codegen.module,
                &codegen.triple,
                optimization_level,
                &path,
            );

            println!(
                "{}[INFO]{} assembly written to: {}",
                GREEN,
                RESET,
                path.display(),
            );
        }

        let object = object_path(
            &self.session.options
        );

        let timer = self.session.timings.start(
            "object emission"
        );

        codegen.emit_object(
            optimization_level,
            &object,
        );

        self.session.timings.finish(timer);

        if self.session.options.keeps_object() {
            println!(
                "{}[INFO]{} object written to: {}",
                GREEN,
                RESET,
                object.display(),
            );
        }

        if self.session.options.mode == CompileMode::NoLink {
            return Ok(());
        }

        let executable = executable_path(
            &self.session.options
        );

        let timer = self.session.timings.start("link");

        linker::link(
            &self.session,
            &object,
            &executable,
        )?;

        self.session.timings.finish(timer);

        if !self.session.options.keeps_object() {
            let _ = fs::remove_file(&object);
        }

        Ok(())
    }
}

fn format_mir(program: &MIRProgram) -> String {
    program.functions.iter().map(|function| format!("{}", function)).collect::<Vec<_>>().join("\n\n")
}

fn llvm_opt_level(level: OptLevel) -> OptimizationLevel {
    match level {
        OptLevel::O0 => OptimizationLevel::None,
        OptLevel::O1 => OptimizationLevel::Less,
        OptLevel::O2 => OptimizationLevel::Default,
        OptLevel::O3 => OptimizationLevel::Aggressive,
    }
}

fn llvm_target(target: TargetMode) -> (String, String) {
    match target {
        TargetMode::Generic => {
            ("generic".to_string(), String::new())
        }

        TargetMode::Native => {
            (
                TargetMachine::get_host_cpu_name().to_string(),
                TargetMachine::get_host_cpu_features().to_string(),
            )
        }
    }
}

fn mir_opt_level(level: OptLevel) -> MirOptimizationLevel {
    match level {
        OptLevel::O0 => MirOptimizationLevel::O0,
        OptLevel::O1 => MirOptimizationLevel::O1,
        OptLevel::O2 => MirOptimizationLevel::O2,
        OptLevel::O3 => MirOptimizationLevel::O3,
    }
}

fn object_path(options: &CompilerOptions) -> PathBuf {
    options.output_path().with_extension("o")
}

fn executable_path(options: &CompilerOptions) -> PathBuf {
    let mut path = options.output_path();

    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }

    path
}
