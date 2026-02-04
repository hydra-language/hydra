use inkwell::targets::TargetData;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::context::Context;

use ir::types::Type;

pub fn compile_type<'c>(context: &'c Context, target_data: &TargetData, ty: &Type) -> BasicTypeEnum<'c> {
    match ty {
        Type::I8  | Type::U8  => context.i8_type().as_basic_type_enum(),
        Type::I16 | Type::U16 => context.i16_type().as_basic_type_enum(),
        Type::I32 | Type::U32 => context.i32_type().as_basic_type_enum(),
        Type::I64 | Type::U64 => context.i64_type().as_basic_type_enum(),

        Type::F32 => context.f32_type().as_basic_type_enum(),
        Type::F64 => context.f64_type().as_basic_type_enum(),

        // --- Pointer Sized Integers (isize / usize) ---
        // automatically maps to i64 on 64-bit, i32 on 32-bit
        Type::ISIZE | Type::USIZE => {
            context.ptr_sized_int_type(target_data, None).as_basic_type_enum()
        },

        Type::BOOL => context.bool_type().as_basic_type_enum(), // i1
        Type::CHAR => context.i32_type().as_basic_type_enum(),

        Type::VOID => panic!("cannot create a variable or value of type 'void'"),

        Type::ARRAY(inner, size) => {
            let inner_type = compile_type(context, target_data, inner);
            inner_type.array_type(*size as u32).as_basic_type_enum()
        },

        // TODO: structs
        _ => panic!("codegen not yet implemented for type {:?}", ty),

    }
}
