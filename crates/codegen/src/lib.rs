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
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetData, TargetMachine, TargetTriple};
use inkwell::values::{ BasicValueEnum, FunctionValue, PointerValue };
use inkwell::AddressSpace;
use inkwell::types::{BasicType, BasicTypeEnum};

use ir::context::{DefKind, HIRContext};
use ir::intrinsic::IntrinsicKind;
use ir::types::Type;
use mir::{BasicBlockID, LocalID, MIRFunction, MIRProgram, Operand};

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
                // use the same canonical name Type::STRUCT carries.
                let canonical_name = if info.absolute_path.is_empty() {
                    info.name.clone()
                } else {
                    info.absolute_path.join("::")
                };

                struct_defs.push((
                    canonical_name,
                    *def_id,
                    fields.clone(),
                ));
            }
        }

        // PASS 1:
        // declare every concrete struct as opaque first.
        //
        // this matters not only for qualified names, but also for:
        //
        //     struct A { b: B }
        //     struct B { ... }
        //
        // HashMap iteration order must not decide whether B exists
        // when compiling A's field types.
        for (name, _def_id, fields) in &struct_defs {
            if fields.iter().any(|(_, ty, _)| ty.contains_generic()) {
                continue;
            }

            if self.context.get_struct_type(name).is_none() {
                self.context.opaque_struct_type(name);
            }
        }

        // PASS 2:
        // now that every struct name exists, populate the fields.
        for (name, _def_id, fields) in &struct_defs {
            if fields.iter().any(|(_, ty, _)| ty.contains_generic()) {
                continue;
            }

            let struct_ty = self
                .context
                .get_struct_type(name)
                .ok_or_else(|| {
                    format!(
                        "ICE: LLVM struct type '{}' was not registered",
                        name
                    )
                })?;

            let field_types: Vec<BasicTypeEnum> = fields
                .iter()
                .map(|(_, ty, _)| {
                    crate::types::compile_type(
                        self.context,
                        &self.target_data,
                        ty,
                    )
                })
                .collect();

            struct_ty.set_body(&field_types, false);
        }

        // declare all functions first.
        for function in &program.functions {
            self.generate_function_prototype(function);
        }

        // then build bodies.
        for function in &program.functions {
            self.generate_function_body(function)?;
        }

        Ok(())
    }

    fn compile_intrinsic(&mut self, kind: IntrinsicKind, type_args: &[Type], args: &[Operand], mir_fn: &MIRFunction) 
        -> Result<BasicValueEnum<'c>, String> 
    {
        match kind {
            IntrinsicKind::SizeOf => {
                let ty = type_args
                    .first()
                    .ok_or("size_of requires one type argument")?;

                let llvm_ty = crate::types::compile_type(
                    self.context,
                    &self.target_data,
                    ty,
                );

                let size = self.target_data
                    .get_abi_size(&llvm_ty);

                let usize_ty = self.context
                    .ptr_sized_int_type(
                        &self.target_data,
                        None,
                    );

                Ok(
                    usize_ty
                        .const_int(size, false)
                        .into()
                )
            }

            IntrinsicKind::AlignOf => {
                let ty = type_args
                    .first()
                    .ok_or("align_of requires one type argument")?;

                let llvm_ty = crate::types::compile_type(
                    self.context,
                    &self.target_data,
                    ty,
                );

                let align = self.target_data
                    .get_abi_alignment(&llvm_ty);

                let usize_ty = self.context
                    .ptr_sized_int_type(
                        &self.target_data,
                        None,
                    );

                Ok(
                    usize_ty
                        .const_int(align as u64, false)
                        .into()
                )
            }

            IntrinsicKind::PtrRead => {
                let src = args
                    .first()
                    .ok_or("ptr_read requires one argument")?;

                let ptr = self.compile_operand(src, mir_fn)?.into_pointer_value();
                let value = self.builder.build_load(ptr, "ptr_read");

                Ok(value)
            }

            IntrinsicKind::PtrWrite => {
                if args.len() != 2 {
                    return Err(
                        "ptr_write requires two arguments".into()
                    );
                }

                let dst = self.compile_operand(&args[0], mir_fn)?.into_pointer_value();
                let value = self.compile_operand(&args[1], mir_fn)?;
                self.builder.build_store(dst, value);

                Ok(self.context.i8_type().const_zero().into())
            }

            IntrinsicKind::PtrOffset => {
                if args.len() != 2 {
                    return Err(
                        "ptr_offset requires two arguments".into()
                    );
                }

                let ptr = self
                    .compile_operand(&args[0], mir_fn)?
                    .into_pointer_value();

                let count = self
                    .compile_operand(&args[1], mir_fn)?
                    .into_int_value();

                let result = unsafe {
                    self.builder.build_gep(
                        ptr,
                        &[count],
                        "ptr_offset",
                    )
                };

                Ok(result.into())
            }

            IntrinsicKind::Alloc => {
                if args.len() != 2 {
                    return Err(
                        "alloc requires two arguments".into()
                    );
                }

                let size = self.compile_operand(
                    &args[0],
                    mir_fn,
                )?;

                let align = self.compile_operand(
                    &args[1],
                    mir_fn,
                )?;

                let alloc_fn = self.get_or_declare_alloc();

                let call = self.builder.build_call(
                    alloc_fn,
                    &[
                        size.into(),
                        align.into(),
                    ],
                    "hydra_alloc",
                );

                call.try_as_basic_value()
                    .left()
                    .ok_or_else(|| {
                        "hydra_alloc unexpectedly returned void".to_string()
                    })
            }

            IntrinsicKind::Dealloc => {
                if args.len() != 3 {
                    return Err(
                        "dealloc requires three arguments".into()
                    );
                }

                let ptr = self.compile_operand(&args[0], mir_fn)?;
                let size = self.compile_operand(&args[1], mir_fn)?;
                let align = self.compile_operand(&args[2], mir_fn)?;

                let dealloc_fn = self.get_or_declare_dealloc();
                self.builder.build_call(dealloc_fn, &[ptr.into(), size.into(), align.into()], "");

                // MIR currently represents void intrinsics as an Rvalue,
                // so return a dummy LLVM value just like ptr_write.
                Ok(self.context.i8_type().const_zero().into())
            }

            IntrinsicKind::SliceLen => {
                let slice = args.first().ok_or("slice_len requires one argument")?;
                let slice_value = self.compile_operand(slice, mir_fn)?.into_struct_value();

                let len = self.builder.build_extract_value(slice_value, 1,"slice_len")
                    .ok_or("ICE: slice value has no length field")?
                    .into_int_value();

                Ok(len.into())
            }
        }
    }

    fn get_or_declare_alloc(&self) -> FunctionValue<'c> {
        if let Some(function) = self.module.get_function("hydra_alloc") {
            return function;
        }

        let usize_ty = self.context.ptr_sized_int_type(
            &self.target_data,
            None,
        );

        let ptr_ty = self
            .context
            .i8_type()
            .ptr_type(AddressSpace::default());

        let fn_ty = ptr_ty.fn_type(
            &[
                usize_ty.into(),
                usize_ty.into(),
            ],
            false,
        );

        self.module.add_function(
            "hydra_alloc",
            fn_ty,
            None,
        )
    }

    fn get_or_declare_dealloc(&self) -> FunctionValue<'c> {
        if let Some(function) = self.module.get_function("hydra_dealloc") {
            return function;
        }

        let usize_ty = self.context.ptr_sized_int_type(
            &self.target_data,
            None,
        );

        let ptr_ty = self
            .context
            .i8_type()
            .ptr_type(AddressSpace::default());

        let fn_ty = self.context
            .void_type()
            .fn_type(
                &[
                    ptr_ty.into(),
                    usize_ty.into(),
                    usize_ty.into(),
                ],
                false,
            );

        self.module.add_function(
            "hydra_dealloc",
            fn_ty,
            None,
        )
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
