pub mod functions;
pub mod variables;
pub mod expressions;
pub mod arrays;
pub mod builtins;
pub mod types;

use std::collections::HashMap;

use inkwell::targets::{InitializationConfig, Target, TargetData, TargetMachine};
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use parser::ast::ASTNode;

pub struct CodeGen<'ctx> {
    context: &'ctx Context,
    builder: Builder<'ctx>,
    module: Module<'ctx>,
    named_values: HashMap<String, PointerValue<'ctx>>,
    current_function: Option<FunctionValue<'ctx>>,
    target_data: TargetData
}

impl<'ctx> CodeGen<'ctx> {

    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        Target::initialize_native(&InitializationConfig::default()).unwrap();

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).unwrap();
        let target_machine = target.create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::Default,
            inkwell::targets::RelocMode::PIC,
            inkwell::targets::CodeModel::Default
        ).unwrap();

        let target_data = target_machine.get_target_data();
        let builder = context.create_builder();
        let module = context.create_module(module_name);

        module.set_triple(&triple);
        module.set_data_layout(&target_data.get_data_layout());

        Self {
            context,
            builder,
            module,
            named_values: HashMap::new(),
            current_function: None,
            target_data
        }
    }

    pub fn generate(&mut self, ast: &[ASTNode]) -> Result<(), String> {
        for node in ast {
            self.generate_node(node)?;
        }

        Ok(())
    }

    pub fn generate_node(&mut self, node: &ASTNode) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        use ASTNode::*;
        match node {
            FunctionDeclaration { name, parameters, return_type, body } => {
                self.generate_function_declaration(name, parameters, return_type, body)
            }
            VariableDeclaration { name, type_annotation, initializer, .. } => {
                self.generate_variable_declaration(name, type_annotation, initializer)
            }
            ReturnStatement { value } => {
                self.generate_return(value)
            }
            Expression { token } => {
                self.generate_expression_literal(token)
            }
            VariableExpression { name } => {
                self.generate_variable_load(name)
            }
            AssignmentExpression { target, operator, value } => {
                self.generate_assignment(target, operator, value)
            }
            FunctionCallExpression { name, arguments } => {
                self.generate_function_call(name, arguments)
            }
            BinaryExpression { left, operator, right } => {
                self.generate_binary_expression(left, operator, right)
            }
            ArrayInitializer { elements, .. } => {
                self.generate_array_initializer(elements)
            }
            _ => Err(format!("unsupported AST node: {:?}", node)),
        }
    }

    pub fn ir_to_string(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
