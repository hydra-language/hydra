pub mod builtins;
pub mod expressions;
pub mod functions;
pub mod stmts;
pub mod types;

use std::collections::HashMap;
use std::path::Path;

use inkwell::OptimizationLevel;
use inkwell::basic_block::BasicBlock;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::builder::Builder;
use inkwell::passes::PassManager;
use inkwell::values::{FunctionValue, PointerValue};
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetData, TargetMachine, TargetTriple};
use inkwell::types::BasicTypeEnum;

use ir::context::{DefKind, HIRContext};
use mir::{MIRProgram, BasicBlockID, LocalID};

pub struct CodeGen<'c> {
    pub context: &'c Context,
    pub hir_context: &'c HIRContext,
    pub module: Module<'c>,
    pub builder: Builder<'c>,
    pub string_constants: HashMap<String, PointerValue<'c>>,
    
    // MIR-specific state:
    pub current_fn: Option<FunctionValue<'c>>,
    pub blocks: HashMap<BasicBlockID, BasicBlock<'c>>,
    pub locals: HashMap<LocalID, PointerValue<'c>>,
    
    pub target_data: TargetData,
    pub triple: TargetTriple,
}

impl<'c> CodeGen<'c> {

    pub fn new(context: &'c Context, hir_context: &'c HIRContext, module_name: &str) -> Self {
        Target::initialize_native(&InitializationConfig::default()).unwrap();

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).unwrap();
        let machine = target.create_target_machine(
            &triple, "generic", "", OptimizationLevel::None,
            RelocMode::PIC, CodeModel::Default
        ).unwrap();

        let target_data = machine.get_target_data();
        let module = context.create_module(module_name);

        module.set_data_layout(&target_data.get_data_layout());
        module.set_triple(&triple);

        Self {
            context,
            hir_context,
            module,
            builder: context.create_builder(),
            string_constants: HashMap::new(),
            current_fn: None,
            blocks: HashMap::new(),
            locals: HashMap::new(),
            target_data,
            triple
        }
    }

    pub fn generate(&mut self, program: &MIRProgram) -> Result<(), String> {
        let mut struct_defs = Vec::new();

        for (def_id, info) in &self.hir_context.definitions {
            if let DefKind::Struct { fields, .. } = &info.kind {
                eprintln!("DEBUG struct in context: {} fields={:?}", info.name, fields);
                struct_defs.push((info.name.clone(), *def_id, fields.clone()));
            }
        }

        for (name, _def_id, fields) in struct_defs {
            if fields.iter().any(|(_, ty, _)| matches!(ty, ir::types::Type::GENERIC(_))) {
                continue;
            }
            let struct_ty = self.context.opaque_struct_type(&name);

            // Convert field definitions (String, Type, bool) to just Type
            let field_types: Vec<BasicTypeEnum> = fields.iter()
                .map(|(_, ty, _)| crate::types::compile_type(self.context, &self.target_data, ty))
                .collect();

            struct_ty.set_body(&field_types, false);
        }        
        // 1. Declare all functions first
        for function in &program.functions {
            self.generate_function_prototype(function);
        }

        // 2. Build bodies
        for function in &program.functions {
            self.generate_function_body(function)?;
        }

        Ok(())
    }

    pub fn ir_to_string(&self) -> String {
        self.module.print_to_string().to_string()
    }

    pub fn get_global_string_ptr(&mut self, value: &str) -> PointerValue<'c> {
        if let Some(ptr) = self.string_constants.get(value) { 
            return *ptr; 
        }

        let ptr = self.builder.build_global_string_ptr(value, "str").as_pointer_value();
        self.string_constants.insert(value.to_string(), ptr);

        ptr
    }

    pub fn run_ir_passes(module: &Module) {
        let fpm = PassManager::create(module);

        fpm.add_promote_memory_to_register_pass();
        fpm.add_instruction_combining_pass();
        fpm.add_reassociate_pass();
        fpm.add_gvn_pass();
        fpm.add_cfg_simplification_pass();
        fpm.add_basic_alias_analysis_pass();
        fpm.add_tail_call_elimination_pass();
        fpm.add_loop_vectorize_pass();
        fpm.add_slp_vectorize_pass();

        fpm.initialize();

        for func in module.get_functions() {
            fpm.run_on(&func);
        }
    }

    pub fn emit_asm(module: &Module, triple: &TargetTriple, opt: OptimizationLevel, path: &Path) 
    {
        Target::initialize_native(&InitializationConfig::default()).unwrap();

        let target = Target::from_triple(triple).unwrap();

        let machine = target.create_target_machine(
            triple,
            "generic",
            "",
            opt,
            RelocMode::PIC,
            CodeModel::Default,
        ).unwrap();

        machine.write_to_file(module, FileType::Assembly, path).unwrap();
    }

    pub fn emit_object(module: &Module, triple: &TargetTriple, opt: OptimizationLevel, path: &Path) {
        Target::initialize_native(&InitializationConfig::default()).unwrap();

        let target = Target::from_triple(triple).unwrap();
        let machine = target.create_target_machine(
            triple,
            "generic",
            "",
            opt,
            RelocMode::PIC,
            CodeModel::Default
        ).unwrap();

        machine.write_to_file(module, FileType::Object, path).unwrap();
    }
}
