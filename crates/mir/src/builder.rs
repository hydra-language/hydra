use std::collections::HashMap;

use crate::{
    AggregateKind, BasicBlock, BasicBlockID, LocalDecl, LocalID, MIRFunction, Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator
};
use crate::Operand;

use errors::error::Span;
use ir::hir::{HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRStmt};
use ir::types::Type;
use ir::context::{DefKind, DefID, HIRContext};
use ir::Constant;

use analyzer::fold;

pub struct MIRBuilder<'a> {
    locals: Vec<LocalDecl>,
    basic_blocks: Vec<BasicBlock>,
    current_block: BasicBlockID,
    var_map: HashMap<DefID, LocalID>, // we need to map HIR variables (DefID) to MIR locals (LocalID)
    const_map: HashMap<DefID, ir::Constant>,
    loop_context: Vec<(BasicBlockID, BasicBlockID)>,
    context: &'a HIRContext,
}

impl<'a> MIRBuilder<'a> {

    pub fn new(context: &'a HIRContext) -> Self {
        Self {
            locals: Vec::new(),
            basic_blocks: Vec::new(),
            current_block: BasicBlockID(0),
            var_map: HashMap::new(),
            const_map: HashMap::new(),
            loop_context: Vec::new(),
            context,
        }
    }

    /// creates a new local variable (temporary or named) and returns its ID
    fn new_local(&mut self, ty: Type, is_mutable: bool, debug_def_id: Option<DefID>) -> LocalID {
        let id = LocalID(self.locals.len());
        self.locals.push(LocalDecl { ty, is_mutable, debug_def_id });
        id
    }

    /// creates a new empty basic block and returns its ID
    fn new_block(&mut self) -> BasicBlockID {
        let id = BasicBlockID(self.basic_blocks.len());
        self.basic_blocks.push(BasicBlock {
            statements: Vec::new(),
            terminator: Terminator::Unreachable, // placeholder until we close the block
        });
        id
    }

    /// adds a statement to the current basic block
    fn push_statement(&mut self, stmt: Statement) {
        self.basic_blocks[self.current_block.0].statements.push(stmt);
    }

    /// sets the terminator for the current block
    fn terminate_block(&mut self, terminator: Terminator) {
        self.basic_blocks[self.current_block.0].terminator = terminator;
    }

    // ========================================================================
    // ENTRY POINT
    // ========================================================================
    
    pub fn build_function(mut self, hir_fn: HIRFunction) -> MIRFunction {
        // 1. _0 is ALWAYS the return value in MIR
        self.new_local(hir_fn.return_type.clone(), true, None);

        // 2. _1 to _N are the function parameters
        for (def_id, ty) in &hir_fn.params {
            let local_id = self.new_local(ty.clone(), false, Some(*def_id));
            self.var_map.insert(*def_id, local_id);
        }

        // 3. Create the entry block (bb0)
        let entry_block = self.new_block();
        self.current_block = entry_block;

        // 4. Flatten the body
        self.lower_block(&hir_fn.body);

        if matches!(self.basic_blocks[self.current_block.0].terminator, Terminator::Unreachable) {
            self.emit_pending_drops(hir_fn.body.span);
            self.terminate_block(Terminator::Return);
        }

        MIRFunction {
            name: hir_fn.name,
            def_id: hir_fn.def_id,
            return_type: hir_fn.return_type,
            arg_count: hir_fn.params.len(),
            locals: self.locals,
            basic_blocks: self.basic_blocks,
            is_inline: hir_fn.is_inline,
        }
    }

    // ========================================================================
    // LOWERING LOGIC
    // ========================================================================

    fn lower_block(&mut self, block: &HIRBlock) {
        for stmt in &block.stmts {
            self.lower_stmt(stmt);
        }
    }

    fn lower_stmt(&mut self, stmt: &HIRStmt) {
        match stmt {
            HIRStmt::VarDecl { def_id, init, span: decl_span } => {
                let is_const = matches!(
                    self.context.get_def(*def_id).map(|i| &i.kind),
                    Some(DefKind::Constant { .. })
                );

                if is_const {
                    let is_scalar = match init.as_ref().map(|e| &e.ty) {
                        Some(Type::I8)  | Some(Type::I16) | Some(Type::I32) | Some(Type::I64) |
                        Some(Type::U8)  | Some(Type::U16) | Some(Type::U32) | Some(Type::U64) |
                        Some(Type::F32) | Some(Type::F64) | Some(Type::BOOL) | Some(Type::CHAR) => true,
                        _ => false,
                    };

                    if is_scalar {
                        if let Some(expr) = init {
                            if let Some(value) = fold::const_fold_hir(expr, self.context) {
                                self.const_map.insert(*def_id, value);
                                return;
                            }
                        }
                    }
                }

                let ty = match self.context.get_def(*def_id).map(|info| &info.kind) {
                    Some(DefKind::Variable { ty, .. }) | Some(DefKind::Constant { ty, .. }) => {
                        ty.clone()
                    }

                    _ => init.as_ref().map(|e| e.ty.clone()).unwrap_or(Type::VOID),
                };

                let local_id = self.new_local(ty, false, Some(*def_id));
                self.var_map.insert(*def_id, local_id);

                if let Some(expr) = init {
                    let operand = self.lower_expr_to_operand(expr);

                    self.push_statement(Statement {
                        kind: crate::StatementKind::Assign(
                            Place { local: local_id, projection: vec![] },
                            Rvalue::Use(operand),
                        ),
                        span: *decl_span
                    });
                }
            }

            HIRStmt::Expr(expr) => {
                // If it's just a standalone expression (like a function call), evaluate it
                self.lower_expr_to_operand(expr);
            }
        }
    }

    /// The core of MIR flattening: Every complex expression is broken down into
    /// a sequence of statements that assign into temporary variables, returning
    /// a simple Operand (a local or a constant).
    fn lower_expr_to_operand(&mut self, expr: &HIRExpr) -> Operand {
        match &expr.kind {
            HIRExprKind::IntLiteral(val) => Operand::Const(Constant::Int(*val, expr.ty.clone())),
            HIRExprKind::FloatLiteral(val) => Operand::Const(Constant::Float(*val, expr.ty.clone())),
            HIRExprKind::BoolLiteral(val) => Operand::Const(Constant::Bool(*val)),
            HIRExprKind::CharLiteral(val) => Operand::Const(Constant::Char(*val)),
            HIRExprKind::StringLiteral(val) => Operand::Const(Constant::String(val.clone())),
            
            HIRExprKind::VarRef(def_id) => {
                if let Some(&local) = self.var_map.get(def_id) {
                    let place = Place { local, projection: vec![] };
                    if self.is_copy_type(&expr.ty) { Operand::Copy(place) } else { Operand::Move(place) }
                } else if let Some(value) = self.const_map.get(def_id) {
                    Operand::Const(value.clone())
                } else {
                    let info = self.context.get_def(*def_id).expect("variable or constant not found");
                    match &info.kind {
                        DefKind::Constant { value, .. } => Operand::Const(value.clone()),
                        _ => panic!("DefID {:?} is neither a local variable nor a constant", def_id),
                    }
                }
            }

            HIRExprKind::Assign { target, value } => {
                // 1. lower the right-hand side first to get the value to store
                let rval_operand = self.lower_expr_to_operand(value);

                // 2. lower the left-hand side to resolve the memory location
                let target_op = self.lower_expr_to_operand(target);

                // 3. extract the Place from the evaluated target
                let target_place = match target_op {
                    Operand::Copy(p) | Operand::Move(p) => p,
                    Operand::Const(_) => panic!("ICE: semantic analysis allowed assignment to a non-place"),
                };

                // 4. emit the assignment using the dynamically resolved place
                self.push_statement(Statement {
                    kind: StatementKind::Assign(target_place, Rvalue::Use(rval_operand)),
                    span: expr.span
                });

                // assignments evaluate to VOID, so we return a dummy unit type
                let unit_local = self.new_local(Type::VOID, false, None);
                Operand::Copy(Place { local: unit_local, projection: vec![] })
            }

            HIRExprKind::Binary { op, lhs, rhs } => {
                let left_op = self.lower_expr_to_operand(lhs);
                let right_op = self.lower_expr_to_operand(rhs);

                let temp_local = self.new_local(expr.ty.clone(), false, None);
                let target_place = Place { local: temp_local, projection: vec![] };

                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        target_place.clone(),
                        Rvalue::BinaryOp(*op, left_op, right_op)
                    ),
                    span: expr.span
                });

                Operand::Copy(target_place)
            }

            HIRExprKind::Unary { op, operand } => {
                let inner_op = self.lower_expr_to_operand(operand);
                let temp_local = self.new_local(expr.ty.clone(), false, None);
                let target_place = Place { local: temp_local, projection: vec![] };

                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        target_place.clone(),
                        Rvalue::UnaryOp(*op, inner_op)
                    ),
                    span: expr.span
                });

                Operand::Copy(target_place)
            }

            HIRExprKind::Cast { expr: inner, kind } => {
                let inner_op = self.lower_expr_to_operand(inner);
                let temp_local = self.new_local(expr.ty.clone(), false, None);
                let target_place = Place { local: temp_local, projection: vec![] };

                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        target_place.clone(),
                        Rvalue::Cast(*kind, inner_op, expr.ty.clone())
                    ),
                    span: expr.span
                });

                Operand::Copy(target_place)
            }

            HIRExprKind::Return(ret_expr_opt) => {
                if let Some(ret_expr) = ret_expr_opt {
                    let ret_operand = self.lower_expr_to_operand(ret_expr);

                    self.push_statement(Statement {
                        kind: StatementKind::Assign(
                            Place {
                                local: LocalID(0),
                                projection: vec![],
                            },
                            Rvalue::Use(ret_operand),
                        ),
                        span: ret_expr.span,
                    });
                }

                // drop everything still owned before leaving this function.
                self.emit_pending_drops(expr.span);

                self.terminate_block(Terminator::Return);

                // anything lowered after this point is unreachable.
                self.current_block = self.new_block();

                let unit_local =
                self.new_local(Type::VOID, false, None);

                Operand::Copy(Place {
                    local: unit_local,
                    projection: vec![],
                })
            }

            HIRExprKind::Call { callee, args, .. } => {
                let info = self.context.get_def(*callee).expect("ICE: function definition not found");
                let callee_name = if info.absolute_path.is_empty() {
                    info.name.clone()
                } else {
                    info.absolute_path.join("::")
                };

                let mut lowered_args = Vec::new();
                for arg in args {
                    lowered_args.push(self.lower_expr_to_operand(arg));
                }

                let temp_local = self.new_local(expr.ty.clone(), false, None);
                let destination = Place { local: temp_local, projection: vec![] };
                let success_block = self.new_block();

                self.terminate_block(Terminator::Call {
                    callee: callee_name,
                    args: lowered_args,
                    destination: destination.clone(),
                    target: success_block,
                });

                self.current_block = success_block;

                Operand::Copy(destination)
            }

            HIRExprKind::IntrinsicCall { callee, kind, args, type_args } => {
                let lowered_args = args
                    .iter()
                    .map(|arg| self.lower_expr_to_operand(arg))
                    .collect();

                let temp_local =
                self.new_local(expr.ty.clone(), false, None);

                let target_place = Place {
                    local: temp_local,
                    projection: vec![],
                };

                let callee_name = self.context
                    .get_def(*callee)
                    .map(|info| {
                        if info.absolute_path.is_empty() {
                            info.name.clone()
                        } else {
                            info.absolute_path.join("::")
                        }
                    })
                    .unwrap_or_else(|| format!("{}", callee));

                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        target_place.clone(),
                        Rvalue::Intrinsic {
                            callee: callee_name,
                            kind: *kind,
                            type_args: type_args.clone(),
                            args: lowered_args,
                        },
                    ),
                    span: expr.span,
                });

                Operand::Copy(target_place)
            }

            HIRExprKind::BuiltinCall { name, args } => {
                let mut lowered_args = Vec::new();
                for arg in args {
                    lowered_args.push(self.lower_expr_to_operand(arg));
                }

                let success_block = self.new_block();

                self.terminate_block(Terminator::BuiltinCall {
                    name: name.clone(),
                    args: lowered_args,
                    target: success_block,
                });

                self.current_block = success_block;

                let unit_local = self.new_local(Type::VOID, false, None);
                Operand::Copy(Place { local: unit_local, projection: vec![] })
            }

            HIRExprKind::If { cond, then_block, else_block } => {
                let cond_op = self.lower_expr_to_operand(cond);

                let then_bb = self.new_block();
                let else_bb = self.new_block();
                let merge_bb = self.new_block();

                self.terminate_block(Terminator::SwitchInt {
                    discriminant: cond_op,
                    true_target: then_bb,
                    false_target: if else_block.is_some() { else_bb } else { merge_bb },
                });

                // Build THEN block
                self.current_block = then_bb;
                self.lower_block(then_block);
                if matches!(self.basic_blocks[self.current_block.0].terminator, Terminator::Unreachable) {
                    self.terminate_block(Terminator::Goto { target: merge_bb });
                }

                // Build ELSE block (if it exists)
                if let Some(els) = else_block {
                    self.current_block = else_bb;
                    self.lower_block(els);
                    if matches!(self.basic_blocks[self.current_block.0].terminator, Terminator::Unreachable) {
                        self.terminate_block(Terminator::Goto { target: merge_bb });
                    }
                }

                self.current_block = merge_bb;

                let unit_local = self.new_local(Type::VOID, false, None);
                Operand::Copy(Place { local: unit_local, projection: vec![] })
            }

            HIRExprKind::Loop(block) => {
                let loop_header = self.new_block();
                let loop_exit = self.new_block();

                // 1. Jump from the current block into the loop header
                self.terminate_block(Terminator::Goto { target: loop_header });

                // 2. Push this loop's targets onto the context stack
                self.loop_context.push((loop_header, loop_exit));
                
                // 3. Switch to the header and compile the body
                self.current_block = loop_header;
                self.lower_block(block);

                // 4. If the body naturally hits the bottom without breaking, loop back to the top
                if matches!(self.basic_blocks[self.current_block.0].terminator, Terminator::Unreachable) {
                    self.terminate_block(Terminator::Goto { target: loop_header });
                }

                // 5. Pop the context and switch the active block to the exit block
                self.loop_context.pop();
                self.current_block = loop_exit;

                // Loops evaluate to VOID in Hydra
                let unit_local = self.new_local(Type::VOID, false, None);
                Operand::Copy(Place { local: unit_local, projection: vec![] })
            }

            HIRExprKind::Break => {
                // Find the exit block of the innermost loop
                let (_, loop_exit) = *self.loop_context.last().expect("break outside of loop");
                
                self.terminate_block(Terminator::Goto { target: loop_exit });
                
                // Any code after a break is unreachable, so we dump it into an empty dead block
                self.current_block = self.new_block();

                let unit_local = self.new_local(Type::VOID, false, None);
                Operand::Copy(Place { local: unit_local, projection: vec![] })
            }

            HIRExprKind::Continue => {
                // Find the header block of the innermost loop
                let (loop_header, _) = *self.loop_context.last().expect("continue outside of loop");
                
                self.terminate_block(Terminator::Goto { target: loop_header });
                
                // Dead code block
                self.current_block = self.new_block();

                let unit_local = self.new_local(Type::VOID, false, None);
                Operand::Copy(Place { local: unit_local, projection: vec![] })
            }

            HIRExprKind::Block(block) => {
                self.lower_block(block);
                let unit_local = self.new_local(Type::VOID, false, None);
                Operand::Copy(Place { local: unit_local, projection: vec![] })
            }

            HIRExprKind::ArrayInit { elements } => {
                let mut lowered_elements = Vec::new();
                for elem in elements {
                    lowered_elements.push(self.lower_expr_to_operand(elem));
                }

                // Create a temporary variable to hold the newly constructed array
                let temp_local = self.new_local(expr.ty.clone(), false, None);
                let target_place = Place { local: temp_local, projection: vec![] };

                // Extract the inner type of the array
                let inner_ty = match &expr.ty {
                    Type::ARRAY(inner, _) => inner.as_ref().clone(),
                    _ => Type::VOID, 
                };

                // Emit the Aggregate instruction
                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        target_place.clone(),
                        Rvalue::Aggregate(AggregateKind::Array(inner_ty), lowered_elements)
                    ),
                    span: expr.span
                });

                Operand::Copy(target_place)
            }

            HIRExprKind::SliceInit { elements } => {
                //
                // a slice literal:
                //
                //     [1, 2, 3]
                //
                // owns no storage itself. materialize a hidden backing array
                // and then construct a fat slice reference to it.
                //
                let element_ty = match &expr.ty {
                    Type::CONST_REF(inner) | Type::REF(inner) => {
                        match inner.as_ref() {
                            Type::SLICE(element) => {
                                element.as_ref().clone()
                            }

                            other => {
                                panic!(
                                    "ICE: SliceInit expected slice reference, found reference to {}",
                                    other
                                );
                            }
                        }
                    }

                    other => {
                        panic!(
                            "ICE: SliceInit expected &[T] or &mut [T], found {}",
                            other
                        );
                    }
                };

                let len = elements.len();

                //
                // first lower all literal elements.
                //
                let mut lowered_elements = Vec::new();
                for element in elements {
                    lowered_elements.push(
                        self.lower_expr_to_operand(element)
                    );
                }

                //
                // hidden backing storage:
                //
                //     _tmp: [T, N]
                //     _tmp = aggregate(...)
                //
                let backing_ty = Type::ARRAY(
                    Box::new(element_ty.clone()),
                    len,
                );

                let backing_local = self.new_local(
                    backing_ty,
                    false,
                    None,
                );

                let backing_place = Place {
                    local: backing_local,
                    projection: vec![],
                };

                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        backing_place.clone(),
                        Rvalue::Aggregate(
                            AggregateKind::Array(
                                element_ty.clone()
                            ),
                            lowered_elements,
                        ),
                    ),
                    span: expr.span,
                });

                //
                // then construct the actual &[T] fat reference.
                //
                let slice_local =
                self.new_local(
                    expr.ty.clone(),
                    false,
                    None,
                );

                let slice_place = Place {
                    local: slice_local,
                    projection: vec![],
                };

                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        slice_place.clone(),
                        Rvalue::SliceRef {
                            is_mut: matches!(
                                expr.ty,
                                Type::REF(_)
                            ),
                            place: backing_place,
                            len,
                            element_ty,
                        },
                    ),
                    span: expr.span,
                });

                Operand::Copy(slice_place)
            }

            HIRExprKind::ArrayAccess { array, index } => {
                // 1. Evaluate the array down to a Place in memory
                let array_op = self.lower_expr_to_operand(array);
                let mut array_place = match array_op {
                    Operand::Copy(p) | Operand::Move(p) => p,
                    Operand::Const(_) => panic!("Cannot index directly into a constant array literal"),
                };

                // 2. Evaluate the index
                let index_op = self.lower_expr_to_operand(index);
                
                // 3. Our ProjectionElem::Index requires a LocalID, not an Operand. 
                // If the index evaluated to a constant (e.g., arr[0]), we must bind that '0' to a temp local first!
                let index_local = match index_op {
                    Operand::Copy(Place { local, projection }) if projection.is_empty() => local,
                    Operand::Move(Place { local, projection }) if projection.is_empty() => local,
                    _ => {
                        let idx_temp = self.new_local(index.ty.clone(), false, None);
                        self.push_statement(Statement {
                            kind: StatementKind::Assign(
                                Place { local: idx_temp, projection: vec![] },
                                Rvalue::Use(index_op)
                            ),
                            span: index.span
                        });
                        idx_temp
                    }               
                };

                // 4. Push the index projection onto the array's place
                array_place.projection.push(ProjectionElem::Index(index_local));

                // 5. Evaluate to a Copy or Move based on the type we are reading out of the array
                if self.is_copy_type(&expr.ty) {
                    Operand::Copy(array_place)
                } else {
                    Operand::Move(array_place)
                }
            }

            HIRExprKind::Borrow { is_mut, target } => {
                let target_op = self.lower_expr_to_operand(target);

                let target_place = match target_op {
                    Operand::Copy(p) | Operand::Move(p) => p,
                    Operand::Const(c) => {
                        let temp = self.new_local(target.ty.clone(), false, None);
                        self.push_statement(Statement {
                            kind: StatementKind::Assign(
                                Place { local: temp, projection: vec![] },
                                Rvalue::Use(Operand::Const(c)),
                            ),
                            span: target.span
                        });
                        Place { local: temp, projection: vec![] }
                    }
                };

                let temp_local = self.new_local(expr.ty.clone(), false, None);
                let out_place = Place { local: temp_local, projection: vec![] };

                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        out_place.clone(),
                        Rvalue::Ref(*is_mut, target_place),
                    ),
                    span: expr.span
                });

                Operand::Copy(out_place)
            }

            HIRExprKind::Dereference { target } => {
                let inner_op = self.lower_expr_to_operand(target);

                // wrap the operand in a deref projection so downstream code
                // can treat it as a place (e.g. for assignment or further projection)
                let inner_place = match inner_op {
                    Operand::Copy(p) | Operand::Move(p) => p,
                    Operand::Const(_) => panic!("cannot dereference a constant"),
                };

                let mut deref_place = inner_place;
                deref_place.projection.push(ProjectionElem::Deref);

                if self.is_copy_type(&expr.ty) {
                    Operand::Copy(deref_place)
                } else {
                    Operand::Move(deref_place)
                }
            }

            HIRExprKind::StructInit { def_id, values } => {
                let mut field_operands = Vec::new();

                for field_expr in values {
                    let operand = self.lower_expr_to_operand(field_expr);
                    field_operands.push(operand);
                }

                // create a temporary variable to hold the newly constructed struct
                let temp_local = self.new_local(expr.ty.clone(), false, None);
                let target_place = Place { local: temp_local, projection: vec![] };

                // emit the Aggregate instruction
                self.push_statement(Statement {
                    kind: StatementKind::Assign(
                        target_place.clone(),
                        Rvalue::Aggregate(AggregateKind::Struct(*def_id), field_operands)
                    ),
                    span: expr.span
                });

                Operand::Copy(target_place)
            }

            HIRExprKind::FieldAccess { object, field_index } => {
                // 1. lower the base expression (e.g., `self`) to an operand
                let base_op = self.lower_expr_to_operand(object);

                // 2. extract the Place from the operand
                let mut place = match base_op {
                    Operand::Copy(p) | Operand::Move(p) => p,
                    Operand::Const(_) => panic!("Cannot access field of a constant directly"),
                };

                // 3. append the field access to the projection list.
                place.projection.push(ProjectionElem::Field(*field_index));

                // 4. return as a copy or move depending on the field's type
                if self.is_copy_type(&expr.ty) {
                    Operand::Copy(place)
                } else {
                    Operand::Move(place)
                }
            }
        }
    }

    fn is_copy_type(&self, ty: &Type) -> bool {
        match ty {
            // Primitives are trivially copied
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::ISIZE |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::USIZE |
            Type::F32 | Type::F64 | Type::CHAR | Type::BOOL => true,
            
            // Pointers and References are just memory addresses. 
            // Copying an address is safe (the underlying data is NOT copied).
            Type::POINTER(_) | Type::CONST_POINTER(_) | Type::REF(_) | Type::CONST_REF(_) => true,
            
            // Everything else (Structs, dynamically sized arrays, strings) defaults to Move.
            // (Later, you can add logic so a Struct is Copy if all its fields are Copy!)
            Type::STRUCT(_) | Type::ARRAY(_, _) => false,
            
            Type::VOID => true,
            _ => false,
        }
    }

    fn type_needs_drop(&self, ty: &Type) -> bool {
        match ty {
            Type::STRUCT(name) => {
                self.context.get_drop_impl(name).is_some()
            }

            _ => false,
        }
    }

    fn emit_pending_drops(&mut self, span: Span) {
        for index in (1..self.locals.len()).rev() {
            let local = LocalID(index);

            let should_drop = {
                let decl = &self.locals[index];
                decl.debug_def_id.is_some() && self.type_needs_drop(&decl.ty)
            };

            if !should_drop {
                continue;
            }

            if self.local_is_moved(local) {
                continue;
            }

            self.push_statement(Statement {
                kind: StatementKind::Drop(Place {
                    local,
                    projection: vec![],
                }),
                span,
            });
        }
    }

    fn local_is_moved(&self, local: LocalID) -> bool {
        fn operand_moves(op: &Operand, local: LocalID) -> bool {
            matches!(
                op,
                Operand::Move(place) if place.local == local
            )
        }

        for block in &self.basic_blocks {
            for stmt in &block.statements {
                match &stmt.kind {
                    StatementKind::Assign(_, rval) => {
                        let moved = match rval {
                            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(_, op, _) => {
                                operand_moves(op, local)
                            }

                            Rvalue::BinaryOp(_, lhs, rhs) => {
                                operand_moves(lhs, local) || operand_moves(rhs, local)
                            }

                            Rvalue::Aggregate(_, ops) => {
                                ops.iter().any(|op| operand_moves(op, local))
                            }

                            Rvalue::Intrinsic { args, .. } => {
                                args.iter().any(|op| operand_moves(op, local))
                            }

                            Rvalue::Ref(_, _) | Rvalue::SliceRef { .. } => false,
                        };

                        if moved {
                            return true;
                        }
                    }

                    StatementKind::Drop(place) => {
                        if place.local == local {
                            return true;
                        }
                    }
                }
            }

            match &block.terminator {
                Terminator::Call { args, .. }
                | Terminator::BuiltinCall { args, .. } => {
                    if args.iter().any(|op| operand_moves(op, local)) {
                        return true;
                    }
                }

                _ => {}
            }
        }

        false
    }
}
