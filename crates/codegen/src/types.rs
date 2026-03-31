use inkwell::AddressSpace;
use inkwell::targets::TargetData;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::context::Context;

use ir::types::Type;

pub fn compile_type<'c>(context: &'c Context, target_data: &TargetData, ty: &Type) -> BasicTypeEnum<'c> {
    match ty {
        Type::I8  | Type::U8 | Type::CHAR => context.i8_type().as_basic_type_enum(),
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

        Type::VOID => panic!("cannot create a variable or value of type 'void'"),

        Type::ARRAY(inner, size) => {
            let inner_type = compile_type(context, target_data, inner);
            inner_type.array_type(*size as u32).as_basic_type_enum()
        },

        // TODO: optimzation, do not allocate the struct until its actually used
        // let s: Struct = Struct::new(); wont actually allocate until some field is accessed
        // or some method is called
        Type::STRUCT(name) => {
            context.get_struct_type(name)
                .unwrap_or_else(|| panic!("LLVM struct type {} not found", name))
                .into()
        }

        Type::REF(inner) | Type::CONST_REF(inner) => {
            let llvm_inner = compile_type(context, target_data, inner);
            let basic_inner: BasicTypeEnum = llvm_inner;

            basic_inner.ptr_type(AddressSpace::default()).into()
        }

        Type::POINTER(inner) => {
            let llvm_inner = compile_type(context, target_data, inner);
            llvm_inner.ptr_type(inkwell::AddressSpace::default()).into()
        }

        _ => panic!("codegen not yet implemented for type {:?}", ty),
    }
}
