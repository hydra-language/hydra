pub mod arrays;
pub mod builtins;
pub mod conditionals;
pub mod expressions;
pub mod functions;
pub mod loops;
pub mod scope;
pub mod stmts;
pub mod types;
pub mod variables;

use std::collections::HashMap;
use std::path::Path;

use inkwell::OptimizationLevel;
use inkwell::basic_block::BasicBlock;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::builder::Builder;
use inkwell::passes::PassManager;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetData, TargetMachine, TargetTriple};

use ir::Program;
use ir::types::Type;
use crate::types::compile_type;

pub struct CodeGen<'c> {
    pub context: &'c Context,
    pub module: Module<'c>,
    pub builder: Builder<'c>,
    pub variables: HashMap<String, PointerValue<'c>>,
    pub string_constants: HashMap<String, PointerValue<'c>>,
    pub current_fn: Option<FunctionValue<'c>>,
    pub loop_stack: Vec<(BasicBlock<'c>, BasicBlock<'c>)>,
    pub target_data: TargetData,
    pub triple: TargetTriple,
}

impl<'c> CodeGen<'c> {

    pub fn new(context: &'c Context, module_name: &str) -> Self {
        Target::initialize_native(&InitializationConfig::default()).unwrap();

        // 2. Setup Target Machine (Default to host for now)
        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).unwrap();
        let machine = target.create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            inkwell::targets::RelocMode::PIC,
            inkwell::targets::CodeModel::Default
        ).unwrap();

        // 3. Get Data Layout from the machine
        let target_data = machine.get_target_data();
        
        let module = context.create_module(module_name);
        module.set_data_layout(&target_data.get_data_layout());
        module.set_triple(&triple);

        let builder = context.create_builder();

        Self {
            context,
            module,
            builder,
            variables: HashMap::new(),
            string_constants: HashMap::new(),
            current_fn: None,
            loop_stack: Vec::new(),
            target_data,
            triple
        }
    }

    pub fn generate(&mut self, program: &Program) -> Result<(), String> {
        for (name, fields) in &program.structs {
            let struct_type = self.context.opaque_struct_type(name);
            let field_types: Vec<BasicTypeEnum> = fields.iter()
                .map(|(_, ty)| compile_type(self.context, &self.target_data, ty)).collect();
            struct_type.set_body(&field_types, false);
        }

        for (name, ty, _) in &program.globals {
            let llvm_ty = compile_type(self.context, &self.target_data, ty);
            let global = self.module.add_global(llvm_ty, None, name);

            global.set_constant(true);
        }

        for (name, _, init_expr) in &program.globals {
            let global = self.module.get_global(name).unwrap();

            if global.get_initializer().is_none() {
                let val = self.compile_const_expr(init_expr, &program.globals)?;

                global.set_initializer(&val);
            }
        }

        for function in &program.functions {
            self.generate_function_prototype(function);
        }

        for function in &program.functions {
            self.generate_function_body(function)?;
        }

        Ok(())
    }
    pub fn ir_to_string(&self) -> String {
        self.module.print_to_string().to_string()
    }

    fn create_entry_block_alloca(&self, name: &str, ty: &Type) -> PointerValue<'c> {
        let builder = self.context.create_builder();
        let entry = self.current_fn.unwrap().get_first_basic_block().unwrap();

        match entry.get_first_instruction() {
            Some(first) => builder.position_before(&first),
            None => builder.position_at_end(entry),
        }

        let llvm_type = compile_type(self.context, &self.target_data, ty);

        builder.build_alloca(llvm_type, name)
    }

    pub fn get_or_create_string_literal(&mut self, s: &str) -> PointerValue<'c> {
        if let Some(ptr) = self.string_constants.get(s) {
            return *ptr;
        }

        let ptr = self.builder.build_global_string_ptr(s, "str_lit").as_pointer_value();
        self.string_constants.insert(s.to_string(), ptr);
        ptr
    }

    pub fn is_val_const(&self, val: &BasicValueEnum<'c>) -> bool {
        match val {
            BasicValueEnum::IntValue(v) => v.is_const(),
            BasicValueEnum::FloatValue(v) => v.is_const(),
            BasicValueEnum::PointerValue(v) => v.is_const(),
            BasicValueEnum::ArrayValue(v) => v.is_const(),
            BasicValueEnum::StructValue(v) => v.as_instruction().is_none(),
            BasicValueEnum::VectorValue(v) => v.is_const(),
        }
    }

    pub fn get_variable_pointer(&self, name: &str) -> PointerValue<'c> {
        self.variables.get(name).copied()
            .unwrap_or_else(|| panic!("ICE: variable could not be found")) 
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
