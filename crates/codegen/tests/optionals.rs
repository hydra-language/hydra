use codegen::{
    CodeGen,
    types::compile_type,
};
use inkwell::{
    context::Context,
    types::{
        BasicTypeEnum,
    },
};
use ir::{
    Constant,
    context::HIRContext,
    types::Type,
};
use mir::{
    AggregateKind,
    MIRFunction,
    Operand,
    Rvalue,
};

fn dummy_mir_function() -> MIRFunction {
    MIRFunction {
        name: "test".to_string(),
        def_id: ir::context::DefID(0),
        display_name: None,
        return_type: Type::VOID,
        arg_count: 0,
        locals: vec![],
        basic_blocks: vec![],
        is_inline: false,
    }
}

#[test]
fn optional_type_lowers_to_discriminant_and_payload_struct() {
    let context = Context::create();
    let hir_context = HIRContext::new();

    let codegen = CodeGen::new(
        &context,
        &hir_context,
        "optional_type_test",
    );

    let llvm_ty = compile_type(
        &context,
        &codegen.target_data,
        &Type::OPTIONAL(Box::new(Type::I32)),
    );

    let BasicTypeEnum::StructType(optional) = llvm_ty else {
        panic!("optional should lower to an LLVM struct");
    };

    let fields = optional.get_field_types();

    assert_eq!(fields.len(), 2);

    assert_eq!(
        fields[0].into_int_type().get_bit_width(),
        1,
    );

    assert_eq!(
        fields[1].into_int_type().get_bit_width(),
        32,
    );
}

#[test]
fn none_optional_aggregate_starts_with_zeroed_payload() {
    let context = Context::create();
    let hir_context = HIRContext::new();

    let mut codegen = CodeGen::new(
        &context,
        &hir_context,
        "none_optional_test",
    );

    let optional_ty = compile_type(
        &context,
        &codegen.target_data,
        &Type::OPTIONAL(Box::new(Type::I32)),
    )
    .into_struct_type();

    let fn_type = optional_ty.fn_type(
        &[],
        false,
    );

    let function = codegen
        .module
        .add_function(
            "test_none",
            fn_type,
            None,
        );

    let block = context.append_basic_block(
        function,
        "entry",
    );

    codegen.builder.position_at_end(block);

    let value = codegen
        .compile_rvalue(
            &Rvalue::Aggregate(
                AggregateKind::Optional(Type::I32),
                vec![
                    Operand::Const(
                        Constant::Bool(false),
                    ),
                ],
            ),
            &dummy_mir_function(),
        )
        .expect("None optional aggregate should compile");

    codegen.builder.build_return(
        Some(&value),
    );

    let llvm = codegen
        .module
        .print_to_string()
        .to_string();

    assert!(
        llvm.contains("zeroinitializer"),
        "None should begin with a zero-initialized optional payload:\n{llvm}",
    );
}

#[test]
fn present_optional_aggregate_contains_payload() {
    let context = Context::create();
    let hir_context = HIRContext::new();

    let mut codegen = CodeGen::new(
        &context,
        &hir_context,
        "present_optional_test",
    );

    let optional_ty = compile_type(
        &context,
        &codegen.target_data,
        &Type::OPTIONAL(Box::new(Type::I32)),
    )
    .into_struct_type();

    let fn_type = optional_ty.fn_type(
        &[],
        false,
    );

    let function = codegen
        .module
        .add_function(
            "test_present",
            fn_type,
            None,
        );

    let block = context.append_basic_block(
        function,
        "entry",
    );

    codegen.builder.position_at_end(block);

    let value = codegen
        .compile_rvalue(
            &Rvalue::Aggregate(
                AggregateKind::Optional(Type::I32),
                vec![
                    Operand::Const(
                        Constant::Bool(true),
                    ),
                    Operand::Const(
                        Constant::Int(
                            42,
                            Type::I32,
                        ),
                    ),
                ],
            ),
            &dummy_mir_function(),
        )
        .expect("present optional aggregate should compile");

    codegen.builder.build_return(
        Some(&value),
    );

    let llvm = codegen
        .module
        .print_to_string()
        .to_string();

    assert!(
        llvm.contains("i1 true"),
        "present optional should set its discriminant:\n{llvm}",
    );

    assert!(
        llvm.contains("i32 42"),
        "present optional should contain its payload:\n{llvm}",
    );
}
