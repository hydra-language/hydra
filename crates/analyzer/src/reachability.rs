use std::collections::{HashMap, HashSet, VecDeque};

use ir::context::{DefID, DefKind, HIRContext};
use ir::hir::{HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRProgram, HIRStmt};
use ir::types::Type;

use crate::traits::DROP_TRAIT_PATH;

const UNINIT_TYPE_PATH: &str = "core::mem::Uninit";

pub fn retain_reachable_functions(program: &mut HIRProgram, context: &HIRContext) {
    let function_indices: HashMap<DefID, usize> = program.functions.iter()
        .enumerate()
        .map(|(index, function)| (function.def_id, index))
        .collect();

    let Some(main_def_id) = program.functions.iter().find(|function| function.name == "main").map(|function| function.def_id) else 
    {
        return;
    };

    let drop_trait_def_id = context.find_trait_by_path(DROP_TRAIT_PATH);

    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([main_def_id]);
    let mut visited_drop_types = HashSet::new();

    while let Some(def_id) = queue.pop_front() {
        if !reachable.insert(def_id) {
            continue;
        }

        let Some(&function_index) = function_indices.get(&def_id) else {
            continue;
        };

        let function = &program.functions[function_index];

        collect_function_dependencies(
            function,
            context,
            drop_trait_def_id,
            &mut queue,
            &mut visited_drop_types,
        );
    }

    program.functions.retain(|function| reachable.contains(&function.def_id));
}

fn collect_function_dependencies(
    function: &HIRFunction,
    context: &HIRContext,
    drop_trait_def_id: Option<DefID>,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) 
{
    for (_, ty) in &function.params {
        collect_drop_dependencies(ty, context, drop_trait_def_id, queue, visited_drop_types);
    }

    collect_block_dependencies(
        &function.body,
        context,
        drop_trait_def_id,
        queue,
        visited_drop_types,
    );
}

fn collect_block_dependencies(
    block: &HIRBlock,
    context: &HIRContext,
    drop_trait_def_id: Option<DefID>,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) 
{
    for stmt in &block.stmts {
        match stmt {
            HIRStmt::VarDecl { ty, init, .. } => {
                collect_drop_dependencies(ty, context, drop_trait_def_id, queue, visited_drop_types);

                if let Some(init) = init {
                    collect_expr_dependencies(
                        init,
                        context,
                        drop_trait_def_id,
                        queue,
                        visited_drop_types,
                    );
                }
            }

            HIRStmt::Expr(expr) => {
                collect_expr_dependencies(
                    expr,
                    context,
                    drop_trait_def_id,
                    queue,
                    visited_drop_types,
                );
            }
        }
    }
}

fn collect_expr_dependencies(
    expr: &HIRExpr,
    context: &HIRContext,
    drop_trait_def_id: Option<DefID>,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) {
    match &expr.kind {
        HIRExprKind::Call { callee, args, .. } => {
            queue.push_back(*callee);

            for arg in args {
                collect_expr_dependencies(arg, context, drop_trait_def_id, queue, visited_drop_types);
            }
        }

        HIRExprKind::IntrinsicCall { args, .. } => {
            for arg in args {
                collect_expr_dependencies(arg, context, drop_trait_def_id, queue, visited_drop_types);
            }
        }

        HIRExprKind::StructInit { values, .. } => {
            for value in values {
                collect_expr_dependencies(value, context, drop_trait_def_id, queue, visited_drop_types);
            }
        }

        HIRExprKind::ArrayInit { elements } | HIRExprKind::SliceInit { elements } => {
            for element in elements {
                collect_expr_dependencies(element, context, drop_trait_def_id, queue, visited_drop_types);
            }
        }

        HIRExprKind::ArrayAccess { array, index } => {
            collect_expr_dependencies(array, context, drop_trait_def_id, queue, visited_drop_types);
            collect_expr_dependencies(index, context, drop_trait_def_id, queue, visited_drop_types);
        }

        HIRExprKind::FieldAccess { object, .. } => {
            collect_expr_dependencies(object, context, drop_trait_def_id, queue, visited_drop_types);
        }

        HIRExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_dependencies(lhs, context, drop_trait_def_id, queue, visited_drop_types);
            collect_expr_dependencies(rhs, context, drop_trait_def_id, queue, visited_drop_types);
        }

        HIRExprKind::Unary { operand, .. } | HIRExprKind::Cast { expr: operand, .. } | 
        HIRExprKind::Borrow { target: operand, .. } | HIRExprKind::Dereference { target: operand, .. } |
        HIRExprKind::OptionalPresent(operand) => 
        {
            collect_expr_dependencies(operand, context, drop_trait_def_id, queue, visited_drop_types);
        }

        HIRExprKind::OrElse { optional, fallback } => {
            collect_expr_dependencies(optional, context, drop_trait_def_id, queue, visited_drop_types);
            collect_expr_dependencies(fallback, context, drop_trait_def_id, queue, visited_drop_types);
        }

        HIRExprKind::Assign { target, value } => {
            collect_expr_dependencies(target, context, drop_trait_def_id, queue, visited_drop_types);
            collect_expr_dependencies(value, context, drop_trait_def_id, queue, visited_drop_types);
        }

        HIRExprKind::If { cond, then_block, else_block } => {
            collect_expr_dependencies(cond, context, drop_trait_def_id, queue, visited_drop_types);

            collect_block_dependencies(
                then_block,
                context,
                drop_trait_def_id,
                queue,
                visited_drop_types,
            );

            if let Some(else_block) = else_block {
                collect_block_dependencies(
                    else_block,
                    context,
                    drop_trait_def_id,
                    queue,
                    visited_drop_types,
                );
            }
        }

        HIRExprKind::Loop(block) => {
            collect_block_dependencies(
                block,
                context,
                drop_trait_def_id,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::Block(block) => {
            collect_block_dependencies(
                block,
                context,
                drop_trait_def_id,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::Return(Some(value)) => {
            collect_expr_dependencies(value, context, drop_trait_def_id, queue, visited_drop_types);
        }

        HIRExprKind::VarRef(_) | HIRExprKind::IntLiteral(_) | HIRExprKind::FloatLiteral(_) | 
        HIRExprKind::StringLiteral(_) | HIRExprKind::CharLiteral(_) | HIRExprKind::BoolLiteral(_) | 
        HIRExprKind::NONE | HIRExprKind::Break | HIRExprKind::Continue | HIRExprKind::Return(None) => {}
    }
}

fn collect_drop_dependencies(
    ty: &Type,
    context: &HIRContext,
    drop_trait_def_id: Option<DefID>,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) {
    if !visited_drop_types.insert(ty.clone()) {
        return;
    }

    if let Some(drop_trait_def_id) = drop_trait_def_id {
        if let Some(drop_method) = context.get_exact_trait_method(drop_trait_def_id, ty, "drop") {
            queue.push_back(drop_method);
        }
    }

    if is_uninit_type(ty, context) {
        return;
    }

    let Type::STRUCT(type_ref) = ty else {
        return;
    };

    let Some(info) = context.get_def(type_ref.def_id) else {
        return;
    };

    let DefKind::Struct { fields, .. } = &info.kind else {
        return;
    };

    for (_, field_ty, _) in fields {
        collect_drop_dependencies(
            field_ty,
            context,
            drop_trait_def_id,
            queue,
            visited_drop_types,
        );
    }
}

fn is_uninit_type(ty: &Type, context: &HIRContext) -> bool {
    match ty {
        Type::GENERIC_INSTANCE(type_ref, _) => type_ref.symbol == UNINIT_TYPE_PATH,

        Type::STRUCT(type_ref) => {
            type_ref.symbol == UNINIT_TYPE_PATH
            || context
                .get_display_name(type_ref.def_id)
                .is_some_and(|name| name.starts_with("core::mem::Uninit<"))
        }

        _ => false,
    }
}
