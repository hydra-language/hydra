use std::{collections::HashMap, fmt};

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
    SLICE(Box<Type>),

    CONST_POINTER(Box<Type>),
    POINTER(Box<Type>),
    REF(Box<Type>),
    CONST_REF(Box<Type>),

    STRUCT(String),

    GENERIC(String),
    GENERIC_INSTANCE(Box<Type>, Vec<Type>),
}

impl Type {

    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64 | 
                       Type::U8 | Type::U16 | Type::U32 | Type::U64 |
                       Type::F32 | Type::F64 | Type::ISIZE | Type::USIZE)
    }

    /// Recursively replaces any GENERIC types with their concrete implementations
    pub fn substitute(&self, substitutions: &HashMap<String, Type>) -> Type {
        match self {
            Type::GENERIC(name) => {
                if let Some(concrete_type) = substitutions.get(name) {
                    concrete_type.clone()
                } else {
                    self.clone() // Return as-is if no substitution provided
                }
            }

            Type::GENERIC_INSTANCE(base, args) => {
                let new_base = Box::new(base.substitute(substitutions));
                let new_args = args.iter().map(|arg| arg.substitute(substitutions)).collect();
                Type::GENERIC_INSTANCE(new_base, new_args)
            }

            Type::ARRAY(inner, size) => {
                Type::ARRAY(Box::new(inner.substitute(substitutions)), *size)
            }

            Type::INFERRED_ARRAY(inner) => {
                Type::INFERRED_ARRAY(Box::new(inner.substitute(substitutions)))
            }

            Type::POINTER(inner) => {
                Type::POINTER(Box::new(inner.substitute(substitutions)))
            }

            Type::CONST_POINTER(inner) => {
                Type::CONST_POINTER(Box::new(inner.substitute(substitutions)))
            }

            Type::REF(inner) => {
                Type::REF(Box::new(inner.substitute(substitutions)))
            }

            Type::CONST_REF(inner) => {
                Type::CONST_REF(Box::new(inner.substitute(substitutions)))
            }

            Type::SLICE(inner) => {
                Type::SLICE(
                    Box::new(
                        inner.substitute(substitutions)
                    )
                )
            }
            // Base types (I64, BOOL, STRUCT, etc.) stay exactly the same
            _ => self.clone(),
        }
    }

    /// Generates a safe string for LLVM symbol names (e.g. `Type::I64` -> `"i64"`)
    pub fn mangle(&self) -> String {
        match self {
            Type::I8 => "i8".to_string(),
            Type::I16 => "i16".to_string(),
            Type::I32 => "i32".to_string(),
            Type::I64 => "i64".to_string(),
            Type::ISIZE => "isize".to_string(),

            Type::U8 => "u8".to_string(),
            Type::U16 => "u16".to_string(),
            Type::U32 => "u32".to_string(),
            Type::U64 => "u64".to_string(),
            Type::USIZE => "usize".to_string(),

            Type::BOOL => "bool".to_string(),
            Type::CHAR => "char".to_string(),

            Type::F32 => "f32".to_string(),
            Type::F64 => "f64".to_string(),

            Type::STRUCT(name) => name.replace("::", "_"),

            Type::POINTER(inner) => format!("ptr_{}", inner.mangle()),
            Type::CONST_POINTER(inner) => format!("cptr_{}", inner.mangle()),

            Type::REF(inner) => format!("ref_{}", inner.mangle()),
            Type::CONST_REF(inner) => format!("cref_{}", inner.mangle()),

            Type::ARRAY(inner, size) => format!("arr_{}_{}", inner.mangle(), size),
            Type::SLICE(inner) => {
                format!("slice_{}", inner.mangle())
            }
            Type::INFERRED_ARRAY(inner) => {
                format!("array_{}", inner.mangle())
            }

            _ => "unknown".to_string(),
        }
    }

    pub fn contains_generic(&self) -> bool {
        match self {
            Type::GENERIC(_) => true,

            Type::GENERIC_INSTANCE(base, args) => {
                base.contains_generic()
                || args.iter().any(Type::contains_generic)
            }

            Type::POINTER(inner) | Type::CONST_POINTER(inner) | 
            Type::REF(inner) | Type::CONST_REF(inner) | 
            Type::SLICE(inner) | Type::INFERRED_ARRAY(inner) => {
                inner.contains_generic()
            }

            Type::ARRAY(inner, _) => {
                inner.contains_generic()
            }

            _ => false,
        }
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

            Type::STRUCT(name) => write!(f, "{}", name),
            Type::REF(inner) => write!(f, "&mut {}", inner),
            Type::CONST_REF(inner) => write!(f, "&{}", inner),
            Type::POINTER(inner) => write!(f, "*mut {}", inner),
            Type::CONST_POINTER(inner) => write!(f, "*const {}", inner),
            
            Type::ARRAY(inner, size) => write!(f, "[{}, {}]", inner, size),
            Type::INFERRED_ARRAY(inner) => write!(f, "[{}]", inner),
            Type::SLICE(inner) => write!(f, "[{}]", inner),

            Type::GENERIC(name) => write!(f, "{}", name),
            Type::GENERIC_INSTANCE(base, args) => {
                write!(f, "{}<", base)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
        }
    }
}
