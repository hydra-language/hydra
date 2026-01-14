pub mod functions;
pub mod variables;
pub mod expressions;
pub mod arrays;
pub mod builtins;
pub mod types;
pub mod conditionals;
pub mod loops;
pub mod scope;

use std::collections::HashMap;

use inkwell::targets::{InitializationConfig, Target, TargetData, TargetMachine};
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use parser::ast::ASTNode;
use crate::scope::ScopeTable;

pub struct CodeGen<'ctx> {
    pub context: &'ctx Context,
    pub builder: Builder<'ctx>,
    pub module: Module<'ctx>,
    pub symbol_table: ScopeTable<'ctx>,
    pub current_function: Option<FunctionValue<'ctx>>,
    pub string_constants: HashMap<String, PointerValue<'ctx>>,
    pub target_data: TargetData
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
            symbol_table: ScopeTable::new(),
            current_function: None,
            string_constants: HashMap::new(),
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
            PostfixUnaryExpression { operator, left } => {
                self.generate_postfix_expression(operator, left)
            },
            UnaryExpression { operator, right } => {
                self.generate_unary_expression(operator, right)
            },
            ArrayInitializer { elements, .. } => {
                self.generate_array_initializer(elements)
            }
            IfStatement { condition, then_branch, else_branch } => {
                self.generate_if_statement(condition, then_branch, else_branch)
            }
            ForLoop { variable, start, end, is_inclusive, body } => {
                self.generate_for_loop(variable, start, end, *is_inclusive, body)
            }
            WhileLoop { condition, body } => {
                self.generate_while_loop(condition, body)
            }
            _ => Err(format!("unsupported AST node: {:?}", node)),
        }
    }

    pub fn get_global_string_ptr(&mut self, value: &str) -> PointerValue<'ctx> {
        // if string exists return it immediately
        if let Some(ptr) = self.string_constants.get(value) {
            return *ptr;
        }

        // otherwise create it
        let ptr = self.builder.build_global_string_ptr(value, "str").as_pointer_value();

        // save and cache it
        self.string_constants.insert(value.to_string(), ptr);

        ptr
    }

    pub fn ir_to_string(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
