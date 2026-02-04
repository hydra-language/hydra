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

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::builder::Builder;
use inkwell::values::{FunctionValue, PointerValue};
use inkwell::targets::{InitializationConfig, Target, TargetData, TargetMachine};

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
    pub target_data: TargetData,
    pub machine: TargetMachine,
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
            OptimizationLevel::Default,
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
            target_data,
            machine,
        }
    }

    pub fn generate(&mut self, program: &Program) -> Result<(), String> {
        
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

    fn declare_printf(&self) {
        let i32_type = self.context.i32_type();
        let str_type = self.context.i8_type().ptr_type(inkwell::AddressSpace::default());
        let printf_type = i32_type.fn_type(&[str_type.into()], true);
        
        if self.module.get_function("printf").is_none() {
            self.module.add_function("printf", printf_type, None);
        }
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
}
