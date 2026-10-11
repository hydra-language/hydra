use codegen::CodeGen;
use errors::error::Span;
use inkwell::context::Context;
use ir::{Constant, context::DefID, types::Type};
use mir::{
    BasicBlock, LocalDecl, LocalID, MIRFunction, MIRProgram, Operand,
    Place, Rvalue, Statement, StatementKind, Terminator,
};

fn local(ty: Type, is_mutable: bool) -> LocalDecl {
    LocalDecl {
        ty, is_mutable, debug_def_id: None, debug_name: None, debug_span: None,
    }
}

#[test]
fn unicode_string_literal_uses_utf8_byte_storage() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "unicode_string_test", "generic".into(), String::new());
    let string_ty = Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::U8))));

    let function = MIRFunction {
        name: "main".into(),
        def_id: DefID(0),
        display_name: None,
        return_type: Type::VOID,
        arg_count: 0,
        locals: vec![local(Type::VOID, true), local(string_ty, false)],
        basic_blocks: vec![BasicBlock {
            statements: vec![Statement {
                kind: StatementKind::Assign(
                    Place { local: LocalID(1), projection: vec![] },
                    Rvalue::Use(Operand::Const(Constant::String("A🦀".into()))),
                ),
                span: Span::default(),
            }],
            terminator: Terminator::Return,
        }],
        is_inline: false,
    };

    codegen.generate(&MIRProgram {
        functions: vec![function],
        structs: vec![],
    }).expect("codegen should succeed");

    let llvm = codegen.ir_to_string();

    assert!(
        llvm.contains("[5 x i8]"),
        "`A🦀` must use five UTF-8 bytes of backing storage:\n{llvm}",
    );
    assert!(
        !llvm.contains("[2 x i32]"),
        "string literals must not use Unicode-scalar char storage:\n{llvm}",
    );
}
