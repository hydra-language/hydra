use lexer::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeID(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub id: NodeID,
    pub name: Token,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WherePredicate {
    pub id: NodeID,
    pub target_type: Type,
    pub bound_traits: Vec<Type>
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    pub predicates: Vec<WherePredicate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: NodeID,
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {

    Path {
        id: NodeID,
        segments: Vec<Token>,
    },

    Generic {
        id: NodeID,
        base: Box<Type>,
        args: Vec<Type>,
    },

    Borrow {
        id: NodeID,
        is_mut: bool,
        inner: Box<Type>,
    },

    RawPointer {
        id: NodeID,
        is_mut: bool,
        inner: Box<Type>,
    },

    Array {
        id: NodeID,
        element_type: Box<Type>,
        size: Box<Expr>,
        token: Token,
    },

    Slice {
        id: NodeID,
        element_type: Box<Type>,
        token: Token,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {

    /// A literal value (number, string, bool)
    Literal { id: NodeID, token: Token },
    
    /// e.g., `my_var`
    Variable { id: NodeID, name: Token },
    
    /// e.g., `std::random::seed`
    Path { id: NodeID, segments: Vec<Token> },
    
    /// e.g., `foo()`
    FunctionCall { id: NodeID, callee: Box<Expr>, arguments: Vec<Expr>, generic_args: Vec<Type> },
    
    /// e.g., `object::method()`
    MethodCall { id: NodeID, object: Box<Expr>, method: Token, arguments: Vec<Expr>, generic_args: Vec<Type> },
    
    /// e.g., `a + b`
    Binary { id: NodeID, left: Box<Expr>, operator: Token, right: Box<Expr> },
    
    /// e.g., `-a` or `!a`
    Unary { id: NodeID, operator: Token, right: Box<Expr> },
    
    /// e.g., `a++`
    PostfixUnary { id: NodeID, operator: Token, left: Box<Expr> },
    
    /// e.g., `a = b`
    Assignment { id: NodeID, target: Box<Expr>, operator: Token, value: Box<Expr> },
    
    /// e.g., `&mut a`
    Borrow { id: NodeID, is_mut: bool, right: Box<Expr> },
    
    /// e.g., `*a`
    Dereference { id: NodeID, right: Box<Expr> },
    
    /// e.g., `object.property`
    Member { id: NodeID, object: Box<Expr>, property: Token },
    
    /// e.g., `{1, 2, 3}`
    ArrayInitializer { id: NodeID, elements: Vec<Expr>, token: Token },

    /// e.g. `[1, 2, 3]`
    SliceInitializer { id: NodeID, elements: Vec<Expr>, token: Token },
    
    /// e.g., `arr[0]`
    ArrayAccess { id: NodeID, array: Box<Expr>, index: Box<Expr>, token: Token },
    
    /// e.g., `Point { x: 1, y: 2 }`
    StructInitializer { id: NodeID, name: Box<Expr>, fields: Vec<(Token, Box<Expr>)> },
    
    /// e.g., `a as i32`
    Cast { id: NodeID, value: Box<Expr>, target: Box<Type> },

    // Control Flow Expressions
    If { id: NodeID, condition: Box<Expr>, then_branch: Block, else_branch: Option<Block> },
    While { id: NodeID, condition: Box<Expr>, body: Block },
    For { id: NodeID, variable: Token, start: Box<Expr>, end: Box<Expr>, is_inclusive: bool, body: Block },
    ForEach { id: NodeID, item: Token, iterable: Box<Expr>, body: Block },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// e.g., `let x: i32 = 5;` or `const y = 10;`
    VariableDecl {
        id: NodeID,
        is_const: bool,
        name: Token,
        type_annotation: Option<Type>,
        initializer: Box<Expr>,
    },
    
    /// An expression followed by a semicolon, e.g., `foo();`
    Expr(Box<Expr>),
    
    /// e.g., `return 5;`
    Return { id: NodeID, value: Option<Box<Expr>> },
    
    /// e.g., `break;`
    Break { id: NodeID, condition: Option<Box<Expr>> },
    
    /// e.g., `continue;`
    Continue { id: NodeID, condition: Option<Box<Expr>> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDecl),
    Struct(StructDecl),
    Trait(TraitDecl),
    Extension(ExtensionDecl),
    Include(IncludeDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub id: NodeID,
    pub name: Token,
    pub annotations: Vec<Annotation>,
    pub generic_params: Vec<GenericParam>,
    pub parameters: Vec<(Token, Type)>,
    pub return_type: Option<Type>,
    pub where_clause: Option<WhereClause>,
    pub body: Option<Block>, // None for externs or trait definitions
    pub is_extern: bool,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub id: NodeID,
    pub name: Token,
    pub generic_params: Vec<GenericParam>,
    pub where_clause: Option<WhereClause>,
    // We treat struct constants and fields separately based on your original design
    pub constants: Vec<Stmt>, // Only VariableDecl (consts) should go here
    pub fields: Vec<(Token, Type)>,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    pub id: NodeID,
    pub name: Token,
    pub methods: Vec<FunctionDecl>,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionDecl {
    pub id: NodeID,
    pub target_trait: Option<Type>, // e.g., Some(RandomGenerator)
    pub target_type: Type,          // e.g., i32
    pub generic_params: Vec<GenericParam>,
    pub where_clause: Option<WhereClause>,
    pub constants: Vec<Stmt>, 
    pub methods: Vec<FunctionDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncludeDecl {
    pub id: NodeID,
    pub path: Type, // Using a Type::Path 
    pub symbols: Option<Vec<Token>>,
    pub alias: Option<Token>,
}
