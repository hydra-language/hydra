use std::collections::HashMap;

use errors::error::Span;
use ir::{
    Constant,
    context::DefID,
    hir::{
        HIRBlock, HIRExpr, HIRExprKind, HIRFunction, HIRLocal,
        HIRStmt, HIRStructInfo, HIRTypeInfo,
    },
    types::{Type, TypeRef},
};
use mir::{
    LocalID, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    builder::MIRBuilder,
};

struct Fixture {
    info: HIRTypeInfo,
    params: Vec<HIRLocal>,
    next_id: usize,
}

impl Fixture {
    fn new() -> Self {
        Self { info: HIRTypeInfo::default(), params: vec![], next_id: 1 }
    }

    fn param(&mut self, name: &str, ty: Type) -> DefID {
        let id = DefID(self.next_id);
        self.next_id += 1;
        self.params.push(HIRLocal {
            def_id: id,
            name: name.into(),
            ty,
            is_mutable: true,
            span: Span::default(),
        });
        id
    }

    fn struct_type(&mut self, name: &str, fields: Vec<Type>) -> Type {
        let id = DefID(self.next_id);
        self.next_id += 1;
        let type_ref = TypeRef::new(id, name.into());
        self.info.structs.insert(id, HIRStructInfo {
            type_ref: type_ref.clone(),
            fields,
        });
        Type::STRUCT(type_ref)
    }

    fn lower(self, stmts: Vec<HIRStmt>) -> mir::MIRFunction {
        let function = HIRFunction {
            name: "test".into(),
            symbol: "test".into(),
            display_name: None,
            source_id: 0,
            def_id: DefID(0),
            params: self.params,
            return_type: Type::VOID,
            body: HIRBlock { stmts, span: Span::default() },
            is_extern: false,
            is_inline: false,
            generic_params: vec![],
        };
        let mut cache = HashMap::new();
        MIRBuilder::new(&self.info, &mut cache).build_function(function)
    }
}

fn expr(kind: HIRExprKind, ty: Type) -> HIRExpr {
    HIRExpr { kind, ty, span: Span::default() }
}

fn var(id: DefID, ty: Type) -> HIRExpr {
    expr(HIRExprKind::VarRef(id), ty)
}

fn int(value: i64) -> HIRExpr {
    expr(HIRExprKind::IntLiteral(value), Type::I32)
}

fn borrow(target: HIRExpr, mutable: bool) -> HIRExpr {
    let ty = if mutable {
        Type::REF(Box::new(target.ty.clone()))
    } else {
        Type::CONST_REF(Box::new(target.ty.clone()))
    };
    expr(HIRExprKind::Borrow { is_mut: mutable, target: Box::new(target) }, ty)
}

fn deref(target: HIRExpr, ty: Type) -> HIRExpr {
    expr(HIRExprKind::Dereference { target: Box::new(target) }, ty)
}

fn field(object: HIRExpr, index: usize, ty: Type) -> HIRExpr {
    expr(HIRExprKind::FieldAccess { object: Box::new(object), field_index: index }, ty)
}

fn index(array: HIRExpr, at: HIRExpr, ty: Type) -> HIRExpr {
    expr(HIRExprKind::ArrayAccess {
        array: Box::new(array),
        index: Box::new(at),
    }, ty)
}

fn borrowed_places(mir: &mir::MIRFunction) -> Vec<(bool, Place)> {
    mir.basic_blocks.iter().flat_map(|block| &block.statements)
        .filter_map(|statement| match &statement.kind {
            StatementKind::Assign(_, Rvalue::Ref(is_mut, place)) => {
                Some((*is_mut, place.clone()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn lowers_shared_borrow_to_ref_rvalue() {
    let mut fixture = Fixture::new();
    let id = fixture.param("x", Type::I32);
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(var(id, Type::I32), false))]);
    let refs = borrowed_places(&mir);

    assert_eq!(refs.len(), 1);
    assert!(!refs[0].0);
    assert_eq!(refs[0].1.local, LocalID(1));
    assert!(refs[0].1.projection.is_empty());
}

#[test]
fn lowers_mutable_borrow_to_mut_ref_rvalue() {
    let mut fixture = Fixture::new();
    let id = fixture.param("x", Type::I32);
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(var(id, Type::I32), true))]);
    let refs = borrowed_places(&mir);

    assert_eq!(refs.len(), 1);
    assert!(refs[0].0);
    assert_eq!(refs[0].1.local, LocalID(1));
}

#[test]
fn borrowing_dereferenced_reference_preserves_deref_projection() {
    let mut fixture = Fixture::new();
    let ref_ty = Type::REF(Box::new(Type::I32));
    let id = fixture.param("p", ref_ty.clone());
    let target = deref(var(id, ref_ty), Type::I32);
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(target, false))]);

    assert_eq!(
        borrowed_places(&mir)[0].1.projection,
        vec![ProjectionElem::Deref],
    );
}

#[test]
fn assignment_through_reference_writes_dereferenced_place() {
    let mut fixture = Fixture::new();
    let ref_ty = Type::REF(Box::new(Type::I32));
    let id = fixture.param("p", ref_ty.clone());
    let target = deref(var(id, ref_ty), Type::I32);
    let assignment = expr(HIRExprKind::Assign {
        target: Box::new(target),
        value: Box::new(int(42)),
    }, Type::VOID);
    let mir = fixture.lower(vec![HIRStmt::Expr(assignment)]);

    assert!(
        mir.basic_blocks.iter().flat_map(|block| &block.statements).any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign(place, Rvalue::Use(
                    Operand::Const(Constant::Int(42, Type::I32))
                )) if place.local == LocalID(1)
                    && place.projection == vec![ProjectionElem::Deref]
            )
        }),
        "assignment must write to the dereferenced location",
    );
}

#[test]
fn borrowing_struct_field_targets_field_place() {
    let mut fixture = Fixture::new();
    let ty = fixture.struct_type("Point", vec![Type::I32, Type::I32]);
    let id = fixture.param("point", ty.clone());
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(
        field(var(id, ty), 0, Type::I32), false,
    ))]);

    assert_eq!(
        borrowed_places(&mir)[0].1.projection,
        vec![ProjectionElem::Field { index: 0, ty: Type::I32 }],
    );
}

#[test]
fn field_through_reference_keeps_deref_before_field_projection() {
    let mut fixture = Fixture::new();
    let ty = fixture.struct_type("Point", vec![Type::I32]);
    let ref_ty = Type::REF(Box::new(ty.clone()));
    let id = fixture.param("point", ref_ty.clone());
    let target = field(deref(var(id, ref_ty), ty), 0, Type::I32);
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(target, false))]);

    assert_eq!(
        borrowed_places(&mir)[0].1.projection,
        vec![
            ProjectionElem::Deref,
            ProjectionElem::Field { index: 0, ty: Type::I32 },
        ],
    );
}

#[test]
fn borrowing_array_element_uses_index_projection() {
    let mut fixture = Fixture::new();
    let ty = Type::ARRAY(Box::new(Type::I32), 3);
    let id = fixture.param("array", ty.clone());
    let target = index(var(id, ty), int(1), Type::I32);
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(target, false))]);
    let place = &borrowed_places(&mir)[0].1;

    assert_eq!(place.local, LocalID(1));
    assert!(matches!(place.projection.as_slice(), [ProjectionElem::Index(_)]));
}

#[test]
fn borrowing_array_element_with_local_index_reuses_index_local() {
    let mut fixture = Fixture::new();
    let ty = Type::ARRAY(Box::new(Type::I32), 3);
    let array_id = fixture.param("array", ty.clone());
    let index_id = fixture.param("i", Type::USIZE);
    let target = index(var(array_id, ty), var(index_id, Type::USIZE), Type::I32);
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(target, false))]);

    assert_eq!(
        borrowed_places(&mir)[0].1.projection,
        vec![ProjectionElem::Index(LocalID(2))],
    );
}

#[test]
fn borrow_preserves_full_deref_field_index_projection_chain() {
    let mut fixture = Fixture::new();
    let array_ty = Type::ARRAY(Box::new(Type::I32), 3);
    let container_ty = fixture.struct_type("Container", vec![array_ty.clone()]);
    let ref_ty = Type::REF(Box::new(container_ty.clone()));
    let id = fixture.param("container", ref_ty.clone());

    let target = index(
        field(deref(var(id, ref_ty), container_ty), 0, array_ty),
        int(1),
        Type::I32,
    );
    let mir = fixture.lower(vec![HIRStmt::Expr(borrow(target, false))]);
    let projection = &borrowed_places(&mir)[0].1.projection;

    assert!(matches!(
        projection.as_slice(),
        [
            ProjectionElem::Deref,
            ProjectionElem::Field { index: 0, .. },
            ProjectionElem::Index(_)
        ]
    ));
}

#[test]
fn slice_literal_lowers_to_backing_array_and_slice_ref() {
    let fixture = Fixture::new();
    let ty = Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::I32))));
    let slice = expr(HIRExprKind::SliceInit {
        elements: vec![int(10), int(20), int(30)],
    }, ty);
    let mir = fixture.lower(vec![HIRStmt::Expr(slice)]);
    let statements: Vec<_> = mir.basic_blocks.iter()
        .flat_map(|block| &block.statements).collect();

    assert!(statements.iter().any(|statement| matches!(
        &statement.kind,
        StatementKind::Assign(_, Rvalue::Aggregate(_, values))
            if values.len() == 3
    )));
    assert!(statements.iter().any(|statement| matches!(
        &statement.kind,
        StatementKind::Assign(_, Rvalue::SliceRef {
            is_mut: false, len: 3, element_ty: Type::I32, ..
        })
    )));
}

#[test]
fn mutable_slice_literal_sets_mutable_slice_ref_flag() {
    let fixture = Fixture::new();
    let ty = Type::REF(Box::new(Type::SLICE(Box::new(Type::I32))));
    let slice = expr(HIRExprKind::SliceInit {
        elements: vec![int(1), int(2)],
    }, ty);
    let mir = fixture.lower(vec![HIRStmt::Expr(slice)]);

    assert!(
        mir.basic_blocks.iter().flat_map(|block| &block.statements).any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign(_, Rvalue::SliceRef {
                    is_mut: true, len: 2, element_ty: Type::I32, ..
                })
            )
        }),
        "mutable slice literal should create a mutable slice reference",
    );
}
