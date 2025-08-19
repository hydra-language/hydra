/**
 * ======
 * ast.rs
 * ======
 * 
 * This file is responsible for defining the ast nodes of Hydra
 */

pub type Program = Vec<Statement>;

#[derive(Debug)]
pub enum Statement {
    VariableDeclaration(VarDecl), // e.g let x: i32 = 5;
    FunctionDeclaration(FnDecl),
    StructDeclaration(StructDecl),
    ExtensionDeclaration(ExtensionDecl),
    IncludeDeclaration(IncludeDecl),
    TypedefDeclaration(TypedefDecl),

    If {
        condition: Expr,
        then_branch: Box<Statement>,
        else_branch: Option<Box<Statement>>,
    },

    For(ForRangeLoop),

    ForEach(ForEachLoop),

    While {
        condition: Expr,
        body: Vec<Statement>,
    },

    Return(Option<Expr>),
    Break,
    Skip,
    Expression(Expr), // stand alones like function calls
}

#[derive(Debug)]
pub enum Expr {
    // most basic values
    // 123, 3.14, "hello", 'c', true
    Literal(LiteralValue),

    // Var name
    Variable(String),

    // Math
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },

    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },

    FunctionCall {
        name: String,
        args: Vec<Expr>
    },

    StructInit {
        name: String,
        fields: Vec<(String, Expr)>
    },

    Assignment {
        name: String,
        value: Box<Expr>,
    },

    Update {
        name: String,
        op: BinaryOp, // for += or -=
        value: Box<Expr>,
    },

    Get {
        object: Box<Expr>,
        name: String,
    },

    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },

    Slice {
        is_heap_allocated: bool, // distinguish '|arr|' from '&arr'
        array: Box<Expr>,
        range_start: Box<Expr>,
        range_end: Box<Expr>,
        is_inclusive: bool, // distinguish .. from ..=
    },

    Cast {
        expr: Box<Expr>,
        target_type: Type,
    },

    Path {
        parts: Vec<String>,
    }
}

#[derive(Debug)]
pub enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    Char(char),
    Bool(bool),
    None,
}

#[derive(Debug)]
pub enum Type {
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Char,
    Bool,
    String,
    Void,
    Custom(String), // Struct names
    Array {
        is_element_const: bool, // true if declared with 'const', false otherwise
        element_type: Box<Type>,
        size: ArraySize,
    },
    Optional(Box<Type>),
}

#[derive(Debug)]
pub struct VarDecl {
    pub is_mutable: bool, // true for 'let', false for 'const'
    pub name: String,
    pub var_type: Type,
    pub initializer: Expr,
}

#[derive(Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Vec<Statement>,
}

#[derive(Debug)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, Type)>
}

#[derive(Debug)]
pub struct ExtensionDecl {
    pub name: String, // Name of struct being extended
    pub methods: Vec<FnDecl>, // List of functions defined without the extension block
}

#[derive(Debug)]
pub struct IncludeDecl {
    pub path: String,
}

#[derive(Debug)]
pub struct TypedefDecl {
    pub alias: String,
    pub original_type: Type,
}

#[derive(Debug)]
pub struct ForRangeLoop {
    pub iterator_name: String,
    pub range_start: Box<Expr>,
    pub range_end: Box<Expr>,
    pub is_inclusive: bool, // true for ..= false for ..
    pub body: Vec<Statement>,
}

#[derive(Debug)]
pub struct ForEachLoop {
    pub element_name: String,
    pub iterable: Box<Expr>,
    pub body: Vec<Statement>,
}

#[derive(Debug)]
pub enum ArraySize {
    Concrete(u64),
    Generic(String), // holds N
}

#[derive(Debug)]
pub enum BinaryOp {
    Add, 
    Subtract, 
    Multiply, 
    Divide, 
    Modulo,
    Equals, 
    NotEquals,
    LessEquals, 
    GreaterEquals,
    And,
    Or,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitShiftLeft,
    BitShiftRight,
}

#[derive(Debug)]
pub enum UnaryOp {
    Increment, // ++x or x++
    Decrement, // --x or x--
    Not, // !
    Negate, // flip sign
}
