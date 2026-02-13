use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    VOID,

    I8, I16, I32, I64, ISIZE,
    U8, U16, U32, U64, USIZE,

    F32, F64,

    BOOL,
    CHAR,

    ARRAY(Box<Type>, usize),
    INFERRED_ARRAY(Box<Type>),

    POINTER(Box<Type>),
    REF(Box<Type>),
    CONST_REF(Box<Type>),

    STRUCT(String),
}

impl Type {

    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64 | 
                       Type::U8 | Type::U16 | Type::U32 | Type::U64 |
                       Type::F32 | Type::F64 | Type::ISIZE | Type::USIZE)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::VOID => write!(f, "void"),
            Type::I8 => write!(f, "i8"),
            Type::I16 => write!(f, "i16"),
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"), 
            Type::ISIZE => write!(f, "isize"),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"), 
            Type::USIZE => write!(f, "usize"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::BOOL => write!(f, "bool"),
            Type::CHAR => write!(f, "char"),
            Type::ARRAY(ty, size) => write!(f, "[{}, {}]", ty, size),
            Type::INFERRED_ARRAY(ty) => write!(f, "[{}, anysize]", ty),

            _ => write!(f, "{:?}", self)
        }
    }
}
