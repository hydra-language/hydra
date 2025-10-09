use std::collections::HashMap;

use inkwell::targets::{InitializationConfig, Target, TargetMachine};
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue, BasicMetadataValueEnum};
use inkwell::types::{BasicType, BasicTypeEnum};

use lexer::{Token, TokenType};
use parser::ast::ASTNode;

pub struct CodeGen<'ctx> {
    context: &'ctx Context,
    builder: Builder<'ctx>,
    module: Module<'ctx>,
    named_values: HashMap<String, PointerValue<'ctx>>,
    current_function: Option<FunctionValue<'ctx>>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let builder = context.create_builder();
        let module = context.create_module(module_name);

        Self {
            context,
            builder,
            module,
            named_values: HashMap::new(),
            current_function: None,
        }
    }

    pub fn generate(&mut self, ast:&[ASTNode]) -> Result<(), String> {
        for node in ast {
            self.generate_node(node)?;
        }

        Ok(())
    }

    fn generate_node(&mut self, node: &ASTNode) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        match node {
            ASTNode::FunctionDeclaration { name, parameters, return_type, body } => {
                self.generate_function_declaration(name, parameters, return_type, body)
            }
            ASTNode::VariableDeclaration { is_const: _, name, type_annotation: _, initializer } => {
                self.generate_variable_declaration(name, initializer)
            }
            ASTNode::ReturnStatement { value } => {
                self.generate_return(value)
            }
            ASTNode::Expression { token } => {
                Ok(Some(self.generate_literal(token)))
            }
            ASTNode::VariableExpression { name } => {
                self.generate_variable_load(name)
            }
            ASTNode::FunctionCallExpression { name, arguments } => {
                self.generate_function_call(name, arguments)
            }
            _ => Err("error: unsupported AST node for codegen".to_string()),
        }
    }

    fn get_type(&self, type_str: &str) -> BasicTypeEnum<'ctx> {
        match type_str {
            "i32" => self.context.i32_type().into(),
            "f64" => self.context.f64_type().into(),
            _ => panic!("error: unsupported type {}", type_str),
        }
    }

    fn generate_literal(&self, token: &Token) -> BasicValueEnum<'ctx> {
        match &token.token_type {
            TokenType::IntLiteral(val) => self.context.i32_type().const_int(*val as u64, false).into(),
            TokenType::FloatLiteral(val) => self.context.f64_type().const_float(*val).into(),
            TokenType::CharLiteral(val) => self.context.i8_type().const_int(*val as u64, false).into(),
            // theres something wrong about casting the char to a u64 when im trying to print that
            // out
            _ => panic!("error: unsupported literal type"),
        }
    }

    fn generate_function_declaration(&mut self, name: &Token, params: &[(Token, Token)],
                                    return_type: &Token, body: &[ASTNode]) -> 
                                    Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        let fn_name = name.lexeme;
        
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = params.iter()
            .map(|(_, param_type)| self.get_type(param_type.lexeme).into())
            .collect();

        let function = if return_type.lexeme == "void" {
            let fn_type = self.context.void_type().fn_type(&param_types, false);
            self.module.add_function(fn_name, fn_type, None)
        } else {
            let ret_type = self.get_type(return_type.lexeme);
            let fn_type = ret_type.fn_type(&param_types, false);
            self.module.add_function(fn_name, fn_type, None)
        };

        let entry = self.context.append_basic_block(function, "entry");

        self.builder.position_at_end(entry);
        self.current_function = Some(function);
        self.named_values.clear();

        for (i, param) in function.get_param_iter().enumerate() {
            let param_name = params[i].0.lexeme;
            let alloca = self.create_entry_block_alloca(param_name, param.get_type());
            self.builder.build_store(alloca, param);
            self.named_values.insert(param_name.to_string(), alloca);
        }

        for node in body {
            self.generate_node(node)?;
        }

        if return_type.lexeme == "void" && self.builder.get_insert_block().and_then(|b| b.get_terminator()).is_none() {
                self.builder.build_return(None);
        }

        Ok(Some(function.as_global_value().as_basic_value_enum()))
    }
    
    fn generate_variable_declaration(&mut self, name: &Token, initializer: &ASTNode) -> 
                                    Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        let var_name = name.lexeme;
        let initial_value = self.generate_node(initializer)?.unwrap();
        
        let alloca = self.create_entry_block_alloca(var_name, initial_value.get_type());
        self.builder.build_store(alloca, initial_value);
        self.named_values.insert(var_name.to_string(), alloca);

        Ok(None)
    }

    fn generate_variable_load(&mut self, name: &Token) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        let var_name = name.lexeme;
        match self.named_values.get(var_name) {
            Some(var_ptr) => {
                let loaded_val = self.builder.build_load(*var_ptr, var_name);
                Ok(Some(loaded_val))
            }
            None => Err(format!("Unknown variable: {}", var_name)),
        }
    }

    fn generate_return(&mut self, value: &ASTNode) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        let return_value = self.generate_node(value)?.unwrap();
        self.builder.build_return(Some(&return_value));
        Ok(None)
    }

    fn get_printf_declaration(&mut self) -> FunctionValue<'ctx> {
        if let Some(function) = self.module.get_function("printf") {
            return function;
        }

        let i32_type = self.context.i32_type();
        let i8_ptr_type = self.context.i8_type().ptr_type(inkwell::AddressSpace::default());
        let printf_type = i32_type.fn_type(&[i8_ptr_type.into()], true); 

        self.module.add_function("printf", printf_type, None)
    }

    fn generate_function_call(&mut self, name: &Token, args: &[ASTNode]) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        if name.lexeme == "println" {
            return self.generate_println_call(args);
        }
        
        Err(format!("Unknown function call: {}", name.lexeme))
    }

    fn generate_println_call(&mut self, args: &[ASTNode]) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        let printf = self.get_printf_declaration();

        let format_str_node = args.first().ok_or("println requires a format string.")?;
        let format_str_literal = match format_str_node {
            ASTNode::Expression { token } => match &token.token_type {
                TokenType::StringLiteral(s) => s,
                _ => return Err("First argument to println must be a string literal.".to_string()),
            },
            _ => return Err("Invalid first argument to println.".to_string()),
        };

        
        // TODO: map the hydra types to the c formatters
        // isize, usize, i/u8-64 all map to %d
        // f32 and f64 map to %.2f for now
        // char maps to %c and prints the char value NOT ascii like %d
        // bool maps to %b
        let c_format_str = format_str_literal.replace("{}", "%d") + "\n";
        let format_str_ptr = self.builder
            .build_global_string_ptr(&c_format_str, "format_str")
            .as_pointer_value();

        let mut printf_args: Vec<BasicMetadataValueEnum<'ctx>> = vec![format_str_ptr.into()];

        for arg_node in args.iter().skip(1) {
            let arg_val = self.generate_node(arg_node)?.unwrap();
            printf_args.push(arg_val.into());
        }
        
        self.builder.build_call(printf, &printf_args, "printf_call");

        Ok(None)
    }

    fn create_entry_block_alloca<T: inkwell::types::BasicType<'ctx>>(&self, name: &str, ty: T) -> PointerValue<'ctx> {
        let builder = self.context.create_builder();
        let entry = self.current_function.unwrap().get_first_basic_block().unwrap();

        match entry.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry),
        }

        builder.build_alloca(ty, name)
    }

    pub fn ir_to_string(&self) -> String {
        self.module.print_to_string().to_string()
    }

    pub fn write_to_object_file(&self, output_path: &std::path::Path) -> Result<(), String> {
        Target::initialize_native(&InitializationConfig::default()).map_err(|e| e.to_string())?;

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
        let target_machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                inkwell::targets::RelocMode::PIC,
                inkwell::targets::CodeModel::Default,
            )
            .ok_or_else(|| "Could not create target machine".to_string())?;

        self.module.set_triple(&triple);
        self.module
            .set_data_layout(&target_machine.get_target_data().get_data_layout());

        target_machine
            .write_to_file(
                &self.module,
                inkwell::targets::FileType::Object,
                output_path,
            )
            .map_err(|e| e.to_string())
    }
}
