use ir::hir::{CastKind, HIRBinOp, HIRExpr, HIRExprKind};
use ir::Constant;
use ir::types::Type;

pub fn fold_cast(kind: CastKind, value: Constant, target: &Type) -> Option<Constant> {
    match kind {
        CastKind::NoOp => Some(value),

        CastKind::Pointer | CastKind::PointerToInteger | CastKind::IntegerToPointer => None,

        CastKind::Numeric => match (value, target) {
            (Constant::Float(value, _), Type::F32) => {
                Some(Constant::Float((value as f32) as f64, Type::F32))
            }

            (Constant::Float(value, _), Type::F64) => {
                Some(Constant::Float(value, Type::F64))
            }

            (Constant::Float(value, _), Type::I8) if value.is_finite() && value >= i8::MIN as f64 && value < (i8::MAX as f64) + 1.0 => 
            {
                Some(Constant::Int(value as i8 as i64, Type::I8))
            }

            (Constant::Float(value, _), Type::I16) if value.is_finite() && value >= i16::MIN as f64 && value < (i16::MAX as f64) + 1.0 => 
            {
                Some(Constant::Int(value as i16 as i64, Type::I16))
            }

            (Constant::Float(value, _), Type::I32) if value.is_finite() && value >= i32::MIN as f64 && value < (i32::MAX as f64) + 1.0 => 
            {
                Some(Constant::Int(value as i32 as i64, Type::I32))
            }
            (Constant::Float(value, _), Type::I64 | Type::ISIZE) if value.is_finite() && value >= i64::MIN as f64 && value < 9223372036854775808.0 => 
            {
                Some(Constant::Int(value as i64, target.clone()))
            }

            (Constant::Float(value, _), Type::U8) if value.is_finite() && (0.0..256.0).contains(&value) => {
                Some(Constant::Int(value as u8 as i64, Type::U8))
            }

            (Constant::Float(value, _), Type::U16) if value.is_finite() && (0.0..65536.0).contains(&value) => {
                Some(Constant::Int(value as u16 as i64, Type::U16))
            }

            (Constant::Float(value, _), Type::U32) if value.is_finite() && (0.0..4294967296.0).contains(&value) => {
                Some(Constant::Int(value as u32 as i64, Type::U32))
            }

            (Constant::Float(value, _), Type::U64 | Type::USIZE) if value.is_finite() && (0.0..18446744073709551616.0).contains(&value) => 
            {
                Some(Constant::Int((value as u64) as i64, target.clone()))
            }

            (Constant::Int(value, _), Type::F32) => {
                Some(Constant::Float((value as f32) as f64, Type::F32))
            }

            (Constant::Int(value, _), Type::F64) => {
                Some(Constant::Float(value as f64, Type::F64))
            }

            _ => None,
        },
    }
}

pub fn const_fold_hir(expr: &HIRExpr) -> Option<Constant> {
    match &expr.kind {
        HIRExprKind::IntLiteral(v)    => Some(Constant::Int(*v, expr.ty.clone())),
        HIRExprKind::FloatLiteral(v)  => Some(Constant::Float(*v, expr.ty.clone())),
        HIRExprKind::BoolLiteral(v)   => Some(Constant::Bool(*v)),
        HIRExprKind::CharLiteral(v)   => Some(Constant::Char(*v)),
        HIRExprKind::StringLiteral(v) => Some(Constant::String(v.clone())),

        HIRExprKind::ConstRef { value, .. } => {
            Some(value.clone())
        }

        HIRExprKind::VarRef(_) => None,

        HIRExprKind::Binary { op, lhs, rhs } => {
            let l = const_fold_hir(lhs)?;
            let r = const_fold_hir(rhs)?;

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

        HIRExprKind::Cast { expr: inner, kind } => {
            fold_cast(*kind, const_fold_hir(inner)?, &expr.ty)
        }

        _ => None,
    }
}
