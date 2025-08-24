
// This file defines the AST nodes for the Hydra programming language

pub type Program = Vec<Statement>;

#[derive(Debug, Clone)]
pub enum Statement {
    VariableDeclaration(VarDecl),
    FunctionDeclaration(FnDecl),
    StructDeclaration(StructDecl),
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

    Match {
        expr: Expr,
        arms: Vec<MatchArm>,
    },

    Return(Option<Expr>),
    Break,
    Skip,
    Expression(Expr),
    Block(Vec<Statement>),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Literal(LiteralValue),
    Identifier(String),
    Wildcard, // _ pattern
}

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(LiteralValue),
    Variable(String),
    
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
        args: Vec<Expr>,
    },

    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },

    StaticCall {
        type_name: String,
        method: String,
        args: Vec<Expr>,
    },

    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
    },

    ArrayInit {
        elements: Vec<Expr>,
    },

    Assignment {
        name: String,
        value: Box<Expr>,
    },

    CompoundAssignment {
        name: String,
        op: BinaryOp,
        value: Box<Expr>,
    },

    FieldAccess {
        object: Box<Expr>,
        field: String,
    },

    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },

    Slice {
        is_heap_allocated: bool, // true for |arr|, false for &arr
        array: Box<Expr>,
        start: Box<Expr>,
        end: Box<Expr>,
        is_inclusive: bool, // true for ..=, false for ..
    },

    Cast {
        expr: Box<Expr>,
        target_type: Type,
    },

    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        is_inclusive: bool,
    },

    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    Char(char),
    Bool(bool),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    // Primitive integer types
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    
    // Floating point types
    F32, F64,
    
    // Other primitives
    Char,
    Bool,
    String,
    Void,
    
    // Custom types (structs)
    Custom(String),
    
    // Array type with optional element mutability
    Array {
        element_const: bool, // true if elements are const
        element_type: Box<Type>,
        size: ArraySize,
    },
    
    Optional(Box<Type>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArraySize {
    Concrete(u64),
    Generic(String), // for compile-time generics like 'size'
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub is_mutable: bool, // true for 'let', false for 'const'
    pub name: String,
    pub var_type: Type,
    pub initializer: Expr,
}

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Type,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub param_type: Type,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<StructField>,
    pub methods: Vec<FnDecl>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub field_type: Type,
}

#[derive(Debug, Clone)]
pub struct IncludeDecl {
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct TypedefDecl {
    pub alias: String,
    pub original_type: Type,
}

#[derive(Debug, Clone)]
pub struct ForRangeLoop {
    pub iterator_name: String,
    pub range: Expr, // This will be a Range expression
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct ForEachLoop {
    pub element_name: String,
    pub iterable: Expr,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    
    // Comparison
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    
    // Logical
    And,
    Or,
    
    // Bitwise
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitShiftLeft,
    BitShiftRight,
    
    // Range operators
    Range,          // ..
    RangeInclusive, // ..=
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,        // !
    Negate,     // -
    PreIncrement,  // ++x
    PostIncrement, // x++
    PreDecrement,  // --x
    PostDecrement, // x--
}
