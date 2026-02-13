use crate::CodeGen;
use ir::{expr::Expr, stmt::Block};

impl<'c> CodeGen<'c> {

    pub fn compile_while(&mut self, cond: &Expr, body: &Block) -> Result<(), String> {
        let parent = self.current_fn.unwrap();

        let while_header = self.context.append_basic_block(parent, "while_header");
        let while_body = self.context.append_basic_block(parent, "while_body");
        let while_exit = self.context.append_basic_block(parent, "while_exit");

        self.builder.build_unconditional_branch(while_header);

        // 1. Compile Header (Condition)
        self.builder.position_at_end(while_header);
        let cond_val = self.compile_expr(cond)?.into_int_value();
        self.builder.build_conditional_branch(cond_val, while_body, while_exit);

        // 2. Push Loop Context for Break/Continue
        // continue -> jumps to header
        // break    -> jumps to exit
        self.loop_stack.push((while_header, while_exit));

        // 3. Compile Body
        self.builder.position_at_end(while_body);
        for stmt in &body.stmts {
            self.compile_stmt(stmt)?;
        }

        // Only add the loop-back jump if the body didn't already return or break
        if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
            self.builder.build_unconditional_branch(while_header);
        }

        // 4. Pop Loop Context
        self.loop_stack.pop();

        // 5. Continue after loop
        self.builder.position_at_end(while_exit);

        Ok(())
    }
}
