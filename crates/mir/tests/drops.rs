use std::collections::HashMap;

use errors::error::Span;
use ir::{
    context::{DefID, DefKind, HIRContext, SymbolInfo, TraitImpl},
    hir::{HIRBlock, HIRFunction, HIRStmt},
    types::{Type, TypeRef},
};
use mir::{builder::MIRBuilder, StatementKind};

fn insert_drop_trait(context: &mut HIRContext) -> DefID {
    let drop_method = context.insert_def(SymbolInfo {
        name: "drop".to_string(),
        span: Span::default(),
        absolute_path: vec![
            "core".to_string(),
            "ops".to_string(),
            "drop".to_string(),
            "Drop".to_string(),
            "drop".to_string(),
        ],
        kind: DefKind::Function {
            params: vec![Type::REF(Box::new(Type::GENERIC("Self".to_string())))],
            annotations: vec![],
            return_type: Type::VOID,
            generic_params: vec![],
            owner_generic_count: 0,
            intrinsic: None,
        },
        is_pub: false,
    });

    context.insert_def(SymbolInfo {
        name: "Drop".to_string(),
        span: Span::default(),
        absolute_path: vec![
            "core".to_string(),
            "ops".to_string(),
            "drop".to_string(),
            "Drop".to_string(),
        ],
        kind: DefKind::Trait {
            methods: vec![drop_method],
        },
        is_pub: true,
    })
}

fn insert_struct(context: &mut HIRContext, name: &str) -> Type {
    let def_id = context.insert_def(SymbolInfo {
        name: name.to_string(),
        span: Span::default(),
        absolute_path: vec![name.to_string()],
        kind: DefKind::Struct {
            fields: vec![],
            generic_params: vec![],
        },
        is_pub: false,
    });

    Type::STRUCT(TypeRef::new(def_id, name.to_string()))
}

fn insert_drop_method(context: &mut HIRContext, target_ty: &Type) -> DefID {
    context.insert_def(SymbolInfo {
        name: "drop".to_string(),
        span: Span::default(),
        absolute_path: vec![
            "Thing::<core::ops::drop::Drop>".to_string(),
            "drop".to_string(),
        ],
        kind: DefKind::Function {
            params: vec![Type::REF(Box::new(target_ty.clone()))],
            annotations: vec![],
            return_type: Type::VOID,
            generic_params: vec![],
            owner_generic_count: 0,
            intrinsic: None,
        },
        is_pub: false,
    })
}

fn build_function_with_local(context: &mut HIRContext, local_ty: Type) -> HIRFunction {
    let local_def_id = context.insert_def(SymbolInfo {
        name: "value".to_string(),
        span: Span::default(),
        absolute_path: vec![],
        kind: DefKind::Variable {
            ty: local_ty.clone(),
            is_mutable: true,
        },
        is_pub: false,
    });

    let function_def_id = context.insert_def(SymbolInfo {
        name: "main".to_string(),
        span: Span::default(),
        absolute_path: vec!["main".to_string()],
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

    HIRFunction {
        name: "main".to_string(),
        def_id: function_def_id,
        params: vec![],
        return_type: Type::VOID,
        body: HIRBlock {
            stmts: vec![HIRStmt::VarDecl {
                def_id: local_def_id,
                ty: local_ty,
                init: None,
                has_type_annotation: true,
                span: Span::default(),
            }],
            span: Span::default(),
        },
        is_extern: false,
        is_inline: false,
        generic_params: vec![],
    }
}

#[test]
fn emits_drop_for_type_implementing_core_drop_trait() {
    let mut context = HIRContext::new();

    let drop_trait_def_id = insert_drop_trait(&mut context);
    let thing_ty = insert_struct(&mut context, "Thing");
    let drop_method_def_id = insert_drop_method(&mut context, &thing_ty);

    context.register_trait_impl(TraitImpl {
        trait_def_id: drop_trait_def_id,
        target_type: thing_ty.clone(),
        methods: HashMap::from([("drop".to_string(), drop_method_def_id)]),
    });

    let function = build_function_with_local(&mut context, thing_ty);

    let mir = MIRBuilder::new(&context).build_function(function);

    assert!(
        mir.basic_blocks
            .iter()
            .flat_map(|block| &block.statements)
            .any(|statement| matches!(statement.kind, StatementKind::Drop(_))),
        "a type implementing core::ops::Drop should receive MIR drop glue",
    );
}

#[test]
fn inherent_method_named_drop_does_not_enable_drop_glue() {
    let mut context = HIRContext::new();

    insert_drop_trait(&mut context);

    let thing_ty = insert_struct(&mut context, "Thing");

    //
    // This function deliberately exists without a TraitImpl entry.
    // Merely naming an inherent method `drop` must not give the type
    // destructor semantics.
    //
    insert_drop_method(&mut context, &thing_ty);

    let function = build_function_with_local(&mut context, thing_ty);

    let mir = MIRBuilder::new(&context).build_function(function);

    assert!(
        mir.basic_blocks
            .iter()
            .flat_map(|block| &block.statements)
            .all(|statement| !matches!(statement.kind, StatementKind::Drop(_))),
        "an inherent method named `drop` must not enable drop glue",
    );
}
