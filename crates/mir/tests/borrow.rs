use errors::error::Span;

use ir::{
    Constant,
    context::{
        DefID,
        DefKind,
        HIRContext,
        SymbolInfo,
    },
    hir::{
        HIRBlock,
        HIRExpr,
        HIRExprKind,
        HIRFunction,
        HIRStmt,
    },
    types::{
        Type,
        TypeRef,
    },
};

use mir::{
    LocalID,
    Operand,
    Place,
    ProjectionElem,
    Rvalue,
    StatementKind,
    builder::MIRBuilder,
};


// ============================================================================
// FIXTURE
// ============================================================================

struct MirFixture {
    context: HIRContext,
    function_def: DefID,
    params: Vec<(DefID, Type)>,
}

impl MirFixture {
    fn new() -> Self {
        let mut context = HIRContext::new();

        let function_def = context.insert_def(SymbolInfo {
            name: "test".to_string(),
            span: Span::default(),
            absolute_path: vec!["test".to_string()],
            kind: DefKind::Function {
                params: vec![],
                annotations: vec![],
                return_type: Type::VOID,
                generic_params: vec![],
                owner_generic_count: 0,
                intrinsic: None,
            },
            is_pub: false,
        });

        Self {
            context,
            function_def,
            params: Vec::new(),
        }
    }

    fn param(
        &mut self,
        name: &str,
        ty: Type,
    ) -> DefID {
        let def_id = self.context.insert_def(SymbolInfo {
            name: name.to_string(),
            span: Span::default(),
            absolute_path: vec![],
            kind: DefKind::Variable {
                ty: ty.clone(),
                is_mutable: true,
            },
            is_pub: false,
        });

        self.params.push((
            def_id,
            ty,
        ));

        def_id
    }

    fn struct_type(
        &mut self,
        name: &str,
        fields: Vec<(String, Type, bool)>,
    ) -> Type {
        let def_id = self.context.insert_def(SymbolInfo {
            name: name.to_string(),
            span: Span::default(),
            absolute_path: vec![name.to_string()],
            kind: DefKind::Struct {
                fields,
                generic_params: vec![],
            },
            is_pub: false,
        });

        Type::STRUCT(
            TypeRef::new(
                def_id,
                name.to_string(),
            )
        )
    }

    fn lower(
        self,
        stmts: Vec<HIRStmt>,
    ) -> mir::MIRFunction {
        let function = HIRFunction {
            name: "test".to_string(),
            def_id: self.function_def,
            params: self.params,
            return_type: Type::VOID,
            body: HIRBlock {
                stmts,
                span: Span::default(),
            },
            is_extern: false,
            is_inline: false,
            generic_params: vec![],
        };

        MIRBuilder::new(&self.context)
            .build_function(function)
    }
}


// ============================================================================
// HIR HELPERS
// ============================================================================

fn span() -> Span {
    Span::default()
}

fn var(
    def_id: DefID,
    ty: Type,
) -> HIRExpr {
    HIRExpr {
        kind: HIRExprKind::VarRef(def_id),
        ty,
        span: span(),
    }
}

fn int(
    value: i64,
    ty: Type,
) -> HIRExpr {
    HIRExpr {
        kind: HIRExprKind::IntLiteral(value),
        ty,
        span: span(),
    }
}

fn borrow(
    target: HIRExpr,
    is_mut: bool,
) -> HIRExpr {
    let target_ty = target.ty.clone();

    HIRExpr {
        kind: HIRExprKind::Borrow {
            is_mut,
            target: Box::new(target),
        },
        ty: if is_mut {
            Type::REF(
                Box::new(target_ty),
            )
        } else {
            Type::CONST_REF(
                Box::new(target_ty),
            )
        },
        span: span(),
    }
}

fn deref(
    target: HIRExpr,
    ty: Type,
) -> HIRExpr {
    HIRExpr {
        kind: HIRExprKind::Dereference {
            target: Box::new(target),
        },
        ty,
        span: span(),
    }
}

fn field(
    object: HIRExpr,
    field_index: usize,
    ty: Type,
) -> HIRExpr {
    HIRExpr {
        kind: HIRExprKind::FieldAccess {
            object: Box::new(object),
            field_index,
        },
        ty,
        span: span(),
    }
}

fn index(
    array: HIRExpr,
    index: HIRExpr,
    ty: Type,
) -> HIRExpr {
    HIRExpr {
        kind: HIRExprKind::ArrayAccess {
            array: Box::new(array),
            index: Box::new(index),
        },
        ty,
        span: span(),
    }
}

fn expr_stmt(expr: HIRExpr) -> HIRStmt {
    HIRStmt::Expr(expr)
}


// ============================================================================
// BORROWS
// ============================================================================

#[test]
fn lowers_shared_borrow_to_ref_rvalue() {
    let mut fixture = MirFixture::new();

    let x = fixture.param(
        "x",
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                var(x, Type::I32),
                false,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        1,
    );

    let StatementKind::Assign(
        destination,
        Rvalue::Ref(is_mut, borrowed),
    ) = &statements[0].kind
    else {
        panic!(
            "expected shared borrow assignment, found {:#?}",
            statements[0].kind,
        );
    };

    //
    // _0 = return place
    // _1 = x
    // _2 = temporary holding &x
    //
    assert_eq!(
        destination.local,
        LocalID(2),
    );

    assert!(
        destination.projection.is_empty(),
    );

    assert!(
        !is_mut,
        "shared borrow must lower with is_mut = false",
    );

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert!(
        borrowed.projection.is_empty(),
    );
}


#[test]
fn lowers_mutable_borrow_to_mut_ref_rvalue() {
    let mut fixture = MirFixture::new();

    let x = fixture.param(
        "x",
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                var(x, Type::I32),
                true,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        1,
    );

    let StatementKind::Assign(
        _,
        Rvalue::Ref(is_mut, borrowed),
    ) = &statements[0].kind
    else {
        panic!(
            "expected mutable borrow assignment, found {:#?}",
            statements[0].kind,
        );
    };

    assert!(
        *is_mut,
        "mutable borrow must lower with is_mut = true",
    );

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert!(
        borrowed.projection.is_empty(),
    );
}


// ============================================================================
// DEREFERENCE
// ============================================================================

#[test]
fn borrowing_dereferenced_reference_preserves_deref_projection() {
    let mut fixture = MirFixture::new();

    let p = fixture.param(
        "p",
        Type::REF(
            Box::new(Type::I32),
        ),
    );

    let dereferenced = deref(
        var(
            p,
            Type::REF(
                Box::new(Type::I32),
            ),
        ),
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                dereferenced,
                false,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        1,
    );

    let StatementKind::Assign(
        _,
        Rvalue::Ref(_, borrowed),
    ) = &statements[0].kind
    else {
        panic!(
            "expected borrow of dereferenced place, found {:#?}",
            statements[0].kind,
        );
    };

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert_eq!(
        borrowed.projection,
        vec![
            ProjectionElem::Deref,
        ],
    );
}


#[test]
fn assignment_through_reference_writes_dereferenced_place() {
    let mut fixture = MirFixture::new();

    let p_ty = Type::REF(
        Box::new(Type::I32),
    );

    let p = fixture.param(
        "p",
        p_ty.clone(),
    );

    let target = deref(
        var(
            p,
            p_ty,
        ),
        Type::I32,
    );

    let assignment = HIRExpr {
        kind: HIRExprKind::Assign {
            target: Box::new(target),
            value: Box::new(
                int(
                    42,
                    Type::I32,
                )
            ),
        },
        ty: Type::I32,
        span: span(),
    };

    let mir = fixture.lower(vec![
        expr_stmt(assignment),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        1,
    );

    let StatementKind::Assign(
        destination,
        Rvalue::Use(
            Operand::Const(
                Constant::Int(value, ty),
            ),
        ),
    ) = &statements[0].kind
    else {
        panic!(
            "expected assignment through dereference, found {:#?}",
            statements[0].kind,
        );
    };

    assert_eq!(
        *value,
        42,
    );

    assert_eq!(
        ty,
        &Type::I32,
    );

    assert_eq!(
        destination.local,
        LocalID(1),
    );

    assert_eq!(
        destination.projection,
        vec![
            ProjectionElem::Deref,
        ],
    );
}


// ============================================================================
// FIELD PROJECTIONS
// ============================================================================

#[test]
fn borrowing_struct_field_targets_field_place() {
    let mut fixture = MirFixture::new();

    let point_ty = fixture.struct_type(
        "Point",
        vec![
            (
                "x".to_string(),
                Type::I32,
                false,
            ),
            (
                "y".to_string(),
                Type::I32,
                false,
            ),
        ],
    );

    let point = fixture.param(
        "point",
        point_ty.clone(),
    );

    let x_field = field(
        var(
            point,
            point_ty,
        ),
        0,
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                x_field,
                false,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        1,
    );

    let StatementKind::Assign(
        _,
        Rvalue::Ref(_, borrowed),
    ) = &statements[0].kind
    else {
        panic!(
            "expected field borrow, found {:#?}",
            statements[0].kind,
        );
    };

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert_eq!(
        borrowed.projection,
        vec![
            ProjectionElem::Field(0),
        ],
    );
}


#[test]
fn field_through_reference_keeps_deref_before_field_projection() {
    let mut fixture = MirFixture::new();

    let point_ty = fixture.struct_type(
        "Point",
        vec![
            (
                "x".to_string(),
                Type::I32,
                false,
            ),
        ],
    );

    let ref_ty = Type::REF(
        Box::new(point_ty.clone()),
    );

    let point = fixture.param(
        "point",
        ref_ty.clone(),
    );

    let dereferenced = deref(
        var(
            point,
            ref_ty,
        ),
        point_ty,
    );

    let x_field = field(
        dereferenced,
        0,
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                x_field,
                false,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        1,
    );

    let StatementKind::Assign(
        _,
        Rvalue::Ref(_, borrowed),
    ) = &statements[0].kind
    else {
        panic!(
            "expected borrow of field through reference, found {:#?}",
            statements[0].kind,
        );
    };

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert_eq!(
        borrowed.projection,
        vec![
            ProjectionElem::Deref,
            ProjectionElem::Field(0),
        ],
    );
}


// ============================================================================
// ARRAY INDEX PROJECTIONS
// ============================================================================

#[test]
fn borrowing_array_element_uses_index_projection() {
    let mut fixture = MirFixture::new();

    let array_ty = Type::ARRAY(
        Box::new(Type::I32),
        3,
    );

    let array = fixture.param(
        "array",
        array_ty.clone(),
    );

    let element = index(
        var(
            array,
            array_ty,
        ),
        int(
            1,
            Type::USIZE,
        ),
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                element,
                false,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    //
    // A constant array index has to be materialized into a local because
    // ProjectionElem::Index stores a LocalID.
    //
    // _2 = const 1
    // _3 = &_1[_2]
    //
    assert_eq!(
        statements.len(),
        2,
    );

    let StatementKind::Assign(
        index_place,
        Rvalue::Use(
            Operand::Const(
                Constant::Int(value, ty),
            ),
        ),
    ) = &statements[0].kind
    else {
        panic!(
            "expected index temporary, found {:#?}",
            statements[0].kind,
        );
    };

    assert_eq!(
        index_place.local,
        LocalID(2),
    );

    assert_eq!(
        *value,
        1,
    );

    assert_eq!(
        ty,
        &Type::USIZE,
    );

    let StatementKind::Assign(
        _,
        Rvalue::Ref(_, borrowed),
    ) = &statements[1].kind
    else {
        panic!(
            "expected borrow of indexed element, found {:#?}",
            statements[1].kind,
        );
    };

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert_eq!(
        borrowed.projection,
        vec![
            ProjectionElem::Index(
                LocalID(2),
            ),
        ],
    );
}


#[test]
fn borrowing_array_element_with_local_index_reuses_index_local() {
    let mut fixture = MirFixture::new();

    let array_ty = Type::ARRAY(
        Box::new(Type::I32),
        3,
    );

    let array = fixture.param(
        "array",
        array_ty.clone(),
    );

    let index_def = fixture.param(
        "index",
        Type::USIZE,
    );

    let element = index(
        var(
            array,
            array_ty,
        ),
        var(
            index_def,
            Type::USIZE,
        ),
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                element,
                false,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    //
    // _1 = array
    // _2 = index
    //
    // Since the index is already a plain local, no temporary is needed.
    //
    assert_eq!(
        statements.len(),
        1,
    );

    let StatementKind::Assign(
        destination,
        Rvalue::Ref(_, borrowed),
    ) = &statements[0].kind
    else {
        panic!(
            "expected indexed borrow, found {:#?}",
            statements[0].kind,
        );
    };

    assert_eq!(
        destination.local,
        LocalID(3),
    );

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert_eq!(
        borrowed.projection,
        vec![
            ProjectionElem::Index(
                LocalID(2),
            ),
        ],
    );
}


// ============================================================================
// COMBINED PROJECTIONS
// ============================================================================

#[test]
fn borrow_preserves_full_deref_field_index_projection_chain() {
    let mut fixture = MirFixture::new();

    let array_ty = Type::ARRAY(
        Box::new(Type::I32),
        4,
    );

    let container_ty = fixture.struct_type(
        "Container",
        vec![
            (
                "values".to_string(),
                array_ty.clone(),
                false,
            ),
        ],
    );

    let ref_ty = Type::REF(
        Box::new(container_ty.clone()),
    );

    let container = fixture.param(
        "container",
        ref_ty.clone(),
    );

    //
    // Conceptually:
    //
    //     &(*container).values[2]
    //
    let dereferenced = deref(
        var(
            container,
            ref_ty,
        ),
        container_ty,
    );

    let values = field(
        dereferenced,
        0,
        array_ty,
    );

    let element = index(
        values,
        int(
            2,
            Type::USIZE,
        ),
        Type::I32,
    );

    let mir = fixture.lower(vec![
        expr_stmt(
            borrow(
                element,
                false,
            ),
        ),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        2,
    );

    //
    // _2 is the materialized index.
    //
    let StatementKind::Assign(
        _,
        Rvalue::Ref(_, borrowed),
    ) = &statements[1].kind
    else {
        panic!(
            "expected projected borrow, found {:#?}",
            statements[1].kind,
        );
    };

    assert_eq!(
        borrowed.local,
        LocalID(1),
    );

    assert_eq!(
        borrowed.projection,
        vec![
            ProjectionElem::Deref,
            ProjectionElem::Field(0),
            ProjectionElem::Index(
                LocalID(2),
            ),
        ],
    );
}


// ============================================================================
// SLICE REFERENCES
// ============================================================================

#[test]
fn slice_literal_lowers_to_backing_array_and_slice_ref() {
    let fixture = MirFixture::new();

    let slice_ty = Type::CONST_REF(
        Box::new(
            Type::SLICE(
                Box::new(Type::I32),
            )
        ),
    );

    let slice = HIRExpr {
        kind: HIRExprKind::SliceInit {
            elements: vec![
                int(1, Type::I32),
                int(2, Type::I32),
                int(3, Type::I32),
            ],
        },
        ty: slice_ty,
        span: span(),
    };

    let mir = fixture.lower(vec![
        expr_stmt(slice),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    assert_eq!(
        statements.len(),
        2,
    );

    //
    // _1: [i32, 3] = aggregate(...)
    //
    let StatementKind::Assign(
        backing,
        Rvalue::Aggregate(_, elements),
    ) = &statements[0].kind
    else {
        panic!(
            "expected hidden backing array, found {:#?}",
            statements[0].kind,
        );
    };

    assert_eq!(
        backing.local,
        LocalID(1),
    );

    assert_eq!(
        elements.len(),
        3,
    );

    //
    // _2 = &slice(_1, len=3)
    //
    let StatementKind::Assign(
        slice_place,
        Rvalue::SliceRef {
            is_mut,
            place,
            len,
            element_ty,
        },
    ) = &statements[1].kind
    else {
        panic!(
            "expected slice reference, found {:#?}",
            statements[1].kind,
        );
    };

    assert_eq!(
        slice_place.local,
        LocalID(2),
    );

    assert!(
        !is_mut,
    );

    assert_eq!(
        place,
        &Place {
            local: LocalID(1),
            projection: vec![],
        },
    );

    assert_eq!(
        *len,
        3,
    );

    assert_eq!(
        element_ty,
        &Type::I32,
    );
}


#[test]
fn mutable_slice_literal_sets_mutable_slice_ref_flag() {
    let fixture = MirFixture::new();

    let slice_ty = Type::REF(
        Box::new(
            Type::SLICE(
                Box::new(Type::I32),
            )
        ),
    );

    let slice = HIRExpr {
        kind: HIRExprKind::SliceInit {
            elements: vec![
                int(1, Type::I32),
                int(2, Type::I32),
            ],
        },
        ty: slice_ty,
        span: span(),
    };

    let mir = fixture.lower(vec![
        expr_stmt(slice),
    ]);

    let statements = &mir.basic_blocks[0].statements;

    let StatementKind::Assign(
        _,
        Rvalue::SliceRef {
            is_mut,
            len,
            ..
        },
    ) = &statements[1].kind
    else {
        panic!(
            "expected mutable slice reference, found {:#?}",
            statements[1].kind,
        );
    };

    assert!(
        *is_mut,
        "mutable slice must lower with is_mut = true",
    );

    assert_eq!(
        *len,
        2,
    );
}
