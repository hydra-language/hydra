use std::collections::{HashMap, HashSet, VecDeque};

use ir::context::DefID;
use ir::hir::{
    HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRProgram, HIRStmt, HIRTypeInfo,
};
use ir::types::Type;

const UNINIT_TYPE_PATH: &str = "core::mem::Uninit";

pub fn retain_reachable_functions(program: &mut HIRProgram, type_info: &HIRTypeInfo) {
    let function_indices: HashMap<DefID, usize> = program.functions.iter().enumerate()
        .map(|(index, function)| (function.def_id, index))
        .collect();

    let Some(main_def_id) = program.functions.iter()
        .find(|function| function.name == "main")
        .map(|function| function.def_id)
    else {
        return;
    };

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

        collect_function_dependencies(
            &program.functions[function_index],
            type_info,
            &mut queue,
            &mut visited_drop_types,
        );
    }

    program.functions.retain(|function| reachable.contains(&function.def_id));
}

fn collect_function_dependencies(
    function: &HIRFunction,
    type_info: &HIRTypeInfo,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) {
    for param in &function.params {
        collect_drop_dependencies(&param.ty, type_info, queue, visited_drop_types);
    }

    collect_block_dependencies(
        &function.body,
        type_info,
        queue,
        visited_drop_types,
    );
}

fn collect_block_dependencies(
    block: &HIRBlock,
    type_info: &HIRTypeInfo,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) {
    for stmt in &block.stmts {
        match stmt {
            HIRStmt::VarDecl { local, init, .. } => {
                collect_drop_dependencies(
                    &local.ty,
                    type_info,
                    queue,
                    visited_drop_types,
                );

                if let Some(init) = init {
                    collect_expr_dependencies(
                        init,
                        type_info,
                        queue,
                        visited_drop_types,
                    );
                }
            }

            HIRStmt::Expr(expr) => {
                collect_expr_dependencies(
                    expr,
                    type_info,
                    queue,
                    visited_drop_types,
                );
            }
        }
    }
}

fn collect_expr_dependencies(
    expr: &HIRExpr,
    type_info: &HIRTypeInfo,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) {
    match &expr.kind {
        HIRExprKind::Call { callee, args, .. } => {
            queue.push_back(callee.def_id);

            for arg in args {
                collect_expr_dependencies(
                    arg,
                    type_info,
                    queue,
                    visited_drop_types,
                );
            }
        }

        HIRExprKind::IntrinsicCall { args, .. } => {
            for arg in args {
                collect_expr_dependencies(
                    arg,
                    type_info,
                    queue,
                    visited_drop_types,
                );
            }
        }

        HIRExprKind::StructInit { values, .. } => {
            for value in values {
                collect_expr_dependencies(
                    value,
                    type_info,
                    queue,
                    visited_drop_types,
                );
            }
        }

        HIRExprKind::ArrayInit { elements } | HIRExprKind::SliceInit { elements } => {
            for element in elements {
                collect_expr_dependencies(
                    element,
                    type_info,
                    queue,
                    visited_drop_types,
                );
            }
        }

        HIRExprKind::ArrayAccess { array, index } => {
            collect_expr_dependencies(
                array,
                type_info,
                queue,
                visited_drop_types,
            );

            collect_expr_dependencies(
                index,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::FieldAccess { object, .. } => {
            collect_expr_dependencies(
                object,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_dependencies(
                lhs,
                type_info,
                queue,
                visited_drop_types,
            );

            collect_expr_dependencies(
                rhs,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::Unary { operand, .. } | HIRExprKind::Cast { expr: operand, .. } | 
        HIRExprKind::Borrow { target: operand, .. } | HIRExprKind::Dereference { target: operand, .. } | HIRExprKind::OptionalPresent(operand) => 
        {
            collect_expr_dependencies(
                operand,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::OrElse { optional, fallback } => {
            collect_expr_dependencies(
                optional,
                type_info,
                queue,
                visited_drop_types,
            );

            collect_expr_dependencies(
                fallback,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::Assign { target, value } => {
            collect_expr_dependencies(
                target,
                type_info,
                queue,
                visited_drop_types,
            );

            collect_expr_dependencies(
                value,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::If { cond, then_block, else_block } => {
            collect_expr_dependencies(
                cond,
                type_info,
                queue,
                visited_drop_types,
            );

            collect_block_dependencies(
                then_block,
                type_info,
                queue,
                visited_drop_types,
            );

            if let Some(else_block) = else_block {
                collect_block_dependencies(
                    else_block,
                    type_info,
                    queue,
                    visited_drop_types,
                );
            }
        }

        HIRExprKind::Loop(block) => {
            collect_block_dependencies(
                block,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::Block(block) => {
            collect_block_dependencies(
                block,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::Return(Some(value)) => {
            collect_expr_dependencies(
                value,
                type_info,
                queue,
                visited_drop_types,
            );
        }

        HIRExprKind::VarRef(_) | HIRExprKind::ConstRef { .. } | HIRExprKind::IntLiteral(_) | 
        HIRExprKind::FloatLiteral(_) | HIRExprKind::StringLiteral(_) | HIRExprKind::CharLiteral(_) | 
        HIRExprKind::BoolLiteral(_) | HIRExprKind::NONE | HIRExprKind::Break | HIRExprKind::Continue | 
        HIRExprKind::Return(None) => {}
    }
}

fn collect_drop_dependencies(
    ty: &Type,
    type_info: &HIRTypeInfo,
    queue: &mut VecDeque<DefID>,
    visited_drop_types: &mut HashSet<Type>,
) {
    if !visited_drop_types.insert(ty.clone()) {
        return;
    }

    if let Some(drop_fn) = type_info.drop_functions.get(ty) {
        queue.push_back(drop_fn.def_id);
    }

    if is_uninit_type(ty) {
        return;
    }

    let Type::STRUCT(type_ref) = ty else {
        return;
    };

    let Some(struct_info) = type_info.structs.get(&type_ref.def_id) else {
        return;
    };

    for field_ty in &struct_info.fields {
        collect_drop_dependencies(
            field_ty,
            type_info,
            queue,
            visited_drop_types,
        );
    }
}

fn is_uninit_type(ty: &Type) -> bool {
    let type_ref = match ty {
        Type::GENERIC_INSTANCE(type_ref, _) | Type::STRUCT(type_ref) => type_ref,
        _ => return false,
    };

    type_ref.symbol == UNINIT_TYPE_PATH || type_ref.display_symbol.as_deref().is_some_and(|name| name.starts_with("core::mem::Uninit<"))
}
