use ir::hir::{HIRExpr, HIRExprKind, HIRBinOp};
use ir::Constant;
use ir::context::{HIRContext, DefKind};

pub fn const_fold_hir(expr: &HIRExpr, context: &HIRContext) -> Option<Constant> {
    match &expr.kind {
        HIRExprKind::IntLiteral(v)    => Some(Constant::Int(*v, expr.ty.clone())),
        HIRExprKind::FloatLiteral(v)  => Some(Constant::Float(*v, expr.ty.clone())),
        HIRExprKind::BoolLiteral(v)   => Some(Constant::Bool(*v)),
        HIRExprKind::CharLiteral(v)   => Some(Constant::Char(*v)),
        HIRExprKind::StringLiteral(v) => Some(Constant::String(v.clone())),

        HIRExprKind::VarRef(def_id) => {
            if let Some(info) = context.get_def(*def_id) {
                if let DefKind::Constant { value, .. } = &info.kind {
                    return Some(value.clone());
                }
            }
            None
        }

        HIRExprKind::Binary { op, lhs, rhs } => {
            let l = const_fold_hir(lhs, context)?;
            let r = const_fold_hir(rhs, context)?;
            match (l, r) {
                (Constant::Float(lv, ty), Constant::Float(rv, _)) => {
                    let result = match op {
                        HIRBinOp::Add => lv + rv,
                        HIRBinOp::Sub => lv - rv,
                        HIRBinOp::Mul => lv * rv,
                        HIRBinOp::Div => lv / rv,
                        _ => return None,
                    };
                    Some(Constant::Float(result, ty))
                }
                (Constant::Int(lv, ty), Constant::Int(rv, _)) => {
                    let result = match op {
                        HIRBinOp::Add => lv + rv,
                        HIRBinOp::Sub => lv - rv,
                        HIRBinOp::Mul => lv * rv,
                        HIRBinOp::Div => lv / rv,
                        _ => return None,
                    };
                    Some(Constant::Int(result, ty))
                }
                _ => None,
            }
        }

        HIRExprKind::Cast { expr: inner, .. } => {
            match const_fold_hir(inner, context)? {
                Constant::Float(v, _) => Some(Constant::Float(v, expr.ty.clone())),
                Constant::Int(v, _)   => Some(Constant::Int(v, expr.ty.clone())),
                other => Some(other),
            }
        }

        _ => None,
    }
}
