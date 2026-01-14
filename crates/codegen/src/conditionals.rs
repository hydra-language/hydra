use super::CodeGen;

use parser::ast::ASTNode;
use inkwell::values::BasicValueEnum;

impl<'ctx> CodeGen<'ctx> {

    pub fn generate_if_statement(&mut self, condition: &ASTNode, then_branch: &[ASTNode],
                                else_branch: &Option<Vec<ASTNode>>) -> Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        let conditional = self.generate_node(condition)?.ok_or("expected condition to return a value")?;
        let condition_to_int = conditional.into_int_value();

        let parent_fn = self.current_function.ok_or("if statement cannot be freestanding")?;

        let then_bb = self.context.append_basic_block(parent_fn, "then");
        let else_bb = self.context.append_basic_block(parent_fn, "else");
        let merge_bb = self.context.append_basic_block(parent_fn, "merge");

        if else_branch.is_some() {
            self.builder.build_conditional_branch(condition_to_int, then_bb, else_bb);
        } else {
            unsafe {
                else_bb.delete().map_err(|_| "failed to delete unused else block")?;
            }
            self.builder.build_conditional_branch(condition_to_int, then_bb, merge_bb);
        }

        self.builder.position_at_end(then_bb);

        self.symbol_table.enter_scope();

        for node in then_branch {
            self.generate_node(node)?;
        }

        self.symbol_table.exit_scope();
        
        if self.builder.get_insert_block().and_then(|b| b.get_terminator()).is_none() {
            self.builder.build_unconditional_branch(merge_bb);
        }

        if let Some(else_nodes) = else_branch {
            self.builder.position_at_end(else_bb);

            self.symbol_table.enter_scope();

            for node in else_nodes {
                self.generate_node(node)?;
            }

            self.symbol_table.exit_scope();

            if self.builder.get_insert_block().and_then(|b| b.get_terminator()).is_none() {
                self.builder.build_unconditional_branch(merge_bb);
            }
        }

        self.builder.position_at_end(merge_bb);

        Ok(None)
    }
}
