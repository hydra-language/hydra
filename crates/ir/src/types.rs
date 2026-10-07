use std::{collections::HashMap, fmt, hash::Hash};

use crate::context::DefID;

#[derive(Debug, Clone)]
pub struct TypeRef {
    pub def_id: DefID,
    pub symbol: String,
    pub display_symbol: Option<String>,
}

impl TypeRef {
    
    pub fn new(def_id: DefID, symbol: String) -> Self {
        Self {
            def_id,
            symbol,
            display_symbol: None,
        }
    }

    pub fn with_display_symbol(mut self, display_symbol: String) -> Self {
        self.display_symbol = Some(display_symbol);
        self
    }
}

impl PartialEq for TypeRef {
    fn eq(&self, other: &Self) -> bool {
        self.def_id == other.def_id && self.symbol == other.symbol
    }
}

impl Eq for TypeRef {}

impl Hash for TypeRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.def_id.hash(state);
        self.symbol.hash(state);
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_symbol.as_deref().unwrap_or(&self.symbol))
    }
}
    
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

    STRUCT(TypeRef),

    GENERIC(String),
    GENERIC_INSTANCE(TypeRef, Vec<Type>),
    OPTIONAL(Box<Type>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MangledPathKind {
    Type,
    Value,
    Method,
}

fn push_path_segment(result: &mut String, namespace: char, name: &str) {
    result.push(namespace);
    result.push_str(&name.len().to_string());
    result.push_str(name);
}

fn mangle_path(path: &[String], type_args: &[Type], kind: MangledPathKind) -> String {
    let mut result = String::from("N");

    let trait_start = path.iter().position(|segment| segment.starts_with('<'));
    let trait_end = path.iter().position(|segment| segment.ends_with('>'));
    let terminal = path.len().checked_sub(1);

    let owner = match kind {
        MangledPathKind::Method => trait_start
            .and_then(|index| index.checked_sub(1))
            .or_else(|| path.len().checked_sub(2)),
        _ => None,
    };

    for (index, segment) in path.iter().enumerate() {
        let starts_trait = segment.starts_with('<');
        let ends_trait = segment.ends_with('>');

        if starts_trait {
            result.push('X');
        }

        let name = segment.trim_start_matches('<').trim_end_matches('>');

        let inside_trait = match (trait_start, trait_end) {
            (Some(start), Some(end)) => index >= start && index <= end,
            _ => false,
        };

        let namespace = if inside_trait {
            if Some(index) == trait_end { 
                't' 
            } else { 
                'm' 
            }
        } else if Some(index) == terminal {
            match kind {
                MangledPathKind::Type => 't',
                MangledPathKind::Value | MangledPathKind::Method => 'v',
            }
        } else if Some(index) == owner {
            't'
        } else {
            'm'
        };

        push_path_segment(&mut result, namespace, name);

        if ends_trait {
            result.push('E');
        }
    }

    if !type_args.is_empty() {
        result.push('I');

        for arg in type_args {
            result.push_str(&arg.mangle());
        }

        result.push('E');
    }

    result.push('E');
    result
}

pub fn mangle_type_symbol(path: &[String], type_args: &[Type]) -> String {
    format!("_H{}", mangle_path(path, type_args, MangledPathKind::Type))
}

pub fn mangle_function_symbol(path: &[String], type_args: &[Type], params: &[Type], return_type: &Type, is_method: bool) -> String 
{
    let kind = if is_method {
        MangledPathKind::Method
    } else {
        MangledPathKind::Value
    };

    let mut result = format!("_H{}", mangle_path(path, type_args, kind));

    result.push('F');

    for param in params {
        result.push_str(&param.mangle());
    }

    result.push('E');
    result.push_str(&return_type.mangle());

    result
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
                let new_args = args.iter().map(|arg| arg.substitute(substitutions)).collect();
                Type::GENERIC_INSTANCE(base.clone(), new_args)
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

            Type::OPTIONAL(inner) => Type::OPTIONAL(Box::new(inner.substitute(substitutions))),

            // Base types (I64, BOOL, STRUCT, etc.) stay exactly the same
            _ => self.clone(),
        }
    }

    /// generates a safe string for LLVM symbol names (e.g. `Type::I64` -> `"i64"`)
    pub fn mangle(&self) -> String {
        match self {
            // Standard Itanium Primitive Codes
            Type::I8 => "a".to_string(), // a => i8
            Type::U8 => "h".to_string(), // h => u8
            Type::I16 => "s".to_string(), // s => i16
            Type::U16 => "t".to_string(), // t => u16
            Type::I32 => "i".to_string(), // i => i32
            Type::U32 => "j".to_string(), // j => u32
            Type::I64 => "x".to_string(), // x => i64
            Type::U64 => "y".to_string(), // y => u64
            Type::F32 => "f".to_string(), // f => f32
            Type::F64 => "d".to_string(), // d => f64
            Type::BOOL => "b".to_string(), // b => bool
            Type::CHAR => "c".to_string(), // c => char
            Type::VOID => "v".to_string(), // v => void

            Type::ISIZE => "5isize".to_string(),
            Type::USIZE => "5usize".to_string(),

            Type::POINTER(inner) => format!("P{}", inner.mangle()),
            Type::CONST_POINTER(inner) => format!("PK{}", inner.mangle()),
            Type::REF(inner) => format!("R{}", inner.mangle()),
            Type::CONST_REF(inner) => format!("RK{}", inner.mangle()),

            Type::ARRAY(inner, len) => format!("A{}_{}", len, inner.mangle()),
            Type::INFERRED_ARRAY(inner) => format!("A__{}", inner.mangle()), 

            Type::SLICE(inner) => format!("N5sliceI{}EE", inner.mangle()),
            Type::OPTIONAL(inner) => format!("N8optionalI{}EE", inner.mangle()),

            Type::STRUCT(type_ref) => {
                let path: Vec<String> = type_ref.symbol.split("::").map(String::from).collect();
                mangle_path(&path, &[], MangledPathKind::Type)
            }

            Type::GENERIC_INSTANCE(type_ref, args) => {
                let path: Vec<String> = type_ref.symbol.split("::").map(String::from).collect();
                mangle_path(&path, args, MangledPathKind::Type)
            }

            Type::GENERIC(name) => format!("{}{}", name.len(), name),
        }
    }

    pub fn contains_generic(&self) -> bool {
        match self {
            Type::GENERIC(_) => true,

            Type::GENERIC_INSTANCE(_, args) => {
                args.iter().any(Type::contains_generic)
            }

            Type::POINTER(inner) | Type::CONST_POINTER(inner) | 
            Type::REF(inner) | Type::CONST_REF(inner) | 
            Type::SLICE(inner) | Type::INFERRED_ARRAY(inner) => {
                inner.contains_generic()
            }

            Type::ARRAY(inner, _) => {
                inner.contains_generic()
            }
            
            Type::OPTIONAL(inner) => inner.contains_generic(),

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

            Type::OPTIONAL(inner) => write!(f, "{}?", inner),
        }
    }
}
