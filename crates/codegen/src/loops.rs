use super::CodeGen;
use parser::ast::ASTNode;
use lexer::Token;
use inkwell::values::{BasicValueEnum};
use inkwell::IntPredicate;

impl<'ctx> CodeGen<'ctx> {

    pub fn generate_for_loop(&mut self, variable: &Token, start: &ASTNode,
                        end: &ASTNode, is_inclusive: bool, body: &[ASTNode]) 
                -> Result<Option<BasicValueEnum<'ctx>>, String>
    {
        let parent_fn = self.current_function.ok_or("loop can not be freestanding")?;

        let start_val = self.generate_node(start)?.ok_or("loop start must return a value")?.into_int_value();
        let end_val = self.generate_node(end)?.ok_or("loop end must return a value")?.into_int_value();

        let cmp_dir = self.builder.build_int_compare(IntPredicate::SLE, start_val, end_val, "dir_check");
        let one = start_val.get_type().const_int(1, false);
        let neg_one = start_val.get_type().const_all_ones();
        let step = self.builder.build_select(cmp_dir, one, neg_one, "step").into_int_value();

        self.symbol_table.enter_scope();

        let loop_var_alloca = self.create_entry_block_alloca(variable.lexeme, start_val.get_type());
        self.builder.build_store(loop_var_alloca, start_val);

        self.symbol_table.insert(variable.lexeme.to_string(), loop_var_alloca);

        let cond_bb = self.context.append_basic_block(parent_fn, "loop_cond");
        let body_bb = self.context.append_basic_block(parent_fn, "loop_body");
        let update_bb = self.context.append_basic_block(parent_fn, "loop_update");
        let after_bb = self.context.append_basic_block(parent_fn, "after_loop");

        self.loop_stack.push((after_bb, update_bb));

        self.builder.build_unconditional_branch(cond_bb);

        self.builder.position_at_end(cond_bb);
        let cur_val = self.builder.build_load(loop_var_alloca, "cur_val").into_int_value();
        
        let is_increasing = self.builder.build_int_compare(IntPredicate::SGT, step, start_val.get_type().const_zero(), "is_inc");

        let cond_inc = if is_inclusive {
            self.builder.build_int_compare(IntPredicate::SLE, cur_val, end_val, "le")
        } else {
            self.builder.build_int_compare(IntPredicate::SLT, cur_val, end_val, "lt")
        };

        let cond_dec = if is_inclusive {
            self.builder.build_int_compare(IntPredicate::SGE, cur_val, end_val, "ge")
        } else {
            self.builder.build_int_compare(IntPredicate::SGT, cur_val, end_val, "gt")
        };

        let loop_cond = self.builder.build_select(is_increasing, cond_inc, cond_dec, "loop_cond").into_int_value();

        self.builder.build_conditional_branch(loop_cond, body_bb, after_bb);

        self.builder.position_at_end(body_bb);
        for node in body {
            self.generate_node(node)?;
        }

        if self.builder.get_insert_block().and_then(|b| b.get_terminator()).is_none() {
            self.builder.build_unconditional_branch(update_bb);
        }

        self.builder.position_at_end(update_bb);

        let curr_for_update = self.builder.build_load(loop_var_alloca, "val_for_inc").into_int_value();
        let next_val = self.builder.build_int_add(curr_for_update, step, "next_val");

        self.builder.build_store(loop_var_alloca, next_val);
        self.builder.build_unconditional_branch(cond_bb);

        self.builder.position_at_end(after_bb);

        self.symbol_table.exit_scope();

        self.loop_stack.pop();

        Ok(None)
    }

    pub fn generate_while_loop(&mut self, condition: &ASTNode, body: &[ASTNode])
            -> Result<Option<BasicValueEnum<'ctx>>, String>
    {
        let parent_fn = self.current_function.ok_or("loop can not be freestanding")?;

        let cond_bb = self.context.append_basic_block(parent_fn, "while_cond");
        let body_bb = self.context.append_basic_block(parent_fn, "while_body");
        let after_bb = self.context.append_basic_block(parent_fn, "after_while");

        self.loop_stack.push((after_bb, cond_bb));

        self.builder.build_unconditional_branch(cond_bb);
        self.builder.position_at_end(cond_bb);

        let cond_val = self.generate_node(condition)?.ok_or("while condition must return a value")?;
        self.builder.build_conditional_branch(cond_val.into_int_value(), body_bb, after_bb);

        self.builder.position_at_end(body_bb);

        self.symbol_table.enter_scope();

        for node in body {
            self.generate_node(node)?;
        }

        self.symbol_table.exit_scope();

        if self.builder.get_insert_block().and_then(|b| b.get_terminator()).is_none() {
            self.builder.build_unconditional_branch(cond_bb);
        }

        self.builder.position_at_end(after_bb);

        self.loop_stack.pop();

        Ok(None)
    }
}
