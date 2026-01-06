use super::CodeGen;

use inkwell::values::{BasicMetadataValueEnum, BasicValue, BasicValueEnum, PointerValue};
use inkwell::types::BasicType;

use lexer::Token;
use parser::ast::ASTNode;


impl<'ctx> CodeGen<'ctx> {
    
    pub fn generate_function_declaration(&mut self, name: &Token, params: &[(Token, Box<ASTNode>)],
                                    return_type: &Box<ASTNode>, body: &[ASTNode]) -> 
                                    Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        let fn_name = name.lexeme;

        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = params.iter()
            .map(|(_, param_type)| self.get_type_from_node(param_type).unwrap().into())
            .collect();

        let function = if self.get_type_name(return_type)? == "void" {
            let fn_type = self.context.void_type().fn_type(&param_types, false);
            self.module.add_function(fn_name, fn_type, None)
        } else {
            let ret_type = self.get_type_from_node(return_type)?;
            let fn_type = ret_type.fn_type(&param_types, false);
            self.module.add_function(fn_name, fn_type, None)
        };

        let entry = self.context.append_basic_block(function, "entry");

        self.builder.position_at_end(entry);
        self.current_function = Some(function);
        self.named_values.clear();

        for (i, param) in function.get_param_iter().enumerate() {
            let param_name = params[i].0.lexeme;
            let param_type = self.get_type_from_node(&params[i].1)?;
            let alloca = self.create_entry_block_alloca(param_name, param_type);
            self.builder.build_store(alloca, param);
            self.named_values.insert(param_name.to_string(), alloca);
        }

        for node in body {
            self.generate_node(node)?;
        }

        if self.get_type_name(return_type)? == "void" && 
        self.builder.get_insert_block().and_then(|b| b.get_terminator()).is_none() 
        {
            self.builder.build_return(None);
        }

        Ok(Some(function.as_global_value().as_basic_value_enum()))
    }

    pub fn generate_function_call(&mut self, name: &Token, args: &[ASTNode]) -> 
                            Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        if name.lexeme == "println" {
            return self.generate_println_call(args);
        }

        // Look up the function in the module
        let function = self
            .module
            .get_function(name.lexeme)
            .ok_or_else(|| format!("Unknown function call: {}", name.lexeme))?;

        // Generate code for each argument
        let mut compiled_args: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
        for arg in args {
            let arg_val = self.generate_node(arg)?.unwrap();
            compiled_args.push(arg_val.into());
        }

        // Build the call instruction
        let call_value = self
            .builder
            .build_call(function, &compiled_args, "calltmp");

        // Return the result of the function call (if it's not void)
        Ok(call_value.try_as_basic_value().left())
    }
    
    pub fn generate_return(&mut self, value: &ASTNode) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        let return_value = self.generate_node(value)?.unwrap();
        self.builder.build_return(Some(&return_value));
        Ok(None)
    }

    pub fn create_entry_block_alloca<T: inkwell::types::BasicType<'ctx>>(&self, name: &str, ty: T) -> PointerValue<'ctx> {
        let builder = self.context.create_builder();
        let entry = self.current_function.unwrap().get_first_basic_block().unwrap();

        match entry.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry),
        }

        builder.build_alloca(ty, name)
    }
}

