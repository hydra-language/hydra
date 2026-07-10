use lexer::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeID(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam<'a> {
    pub id: NodeID,
    pub name: Token<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WherePredicate<'a> {
    pub id: NodeID,
    pub target_type: Type<'a>,
    pub bound_traits: Vec<Type<'a>>
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause<'a> {
    pub predicates: Vec<WherePredicate<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block<'a> {
    pub id: NodeID,
    pub statements: Vec<Stmt<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type<'a> {

    Path {
        id: NodeID,
        segments: Vec<Token<'a>>,
    },

    Generic {
        id: NodeID,
        base: Box<Type<'a>>,
        args: Vec<Type<'a>>,
    },

    Borrow {
        id: NodeID,
        is_mut: bool,
        inner: Box<Type<'a>>,
    },

    RawPointer {
        id: NodeID,
        is_mut: bool,
        inner: Box<Type<'a>>,
    },

    Array {
        id: NodeID,
        element_type: Box<Type<'a>>,
        size: Box<Expr<'a>>,
        token: Token<'a>,
    },

    Slice {
        id: NodeID,
        element_type: Box<Type<'a>>,
        token: Token<'a>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr<'a> {

    /// A literal value (number, string, bool)
    Literal { id: NodeID, token: Token<'a> },
    
    /// e.g., `my_var`
    Variable { id: NodeID, name: Token<'a> },
    
    /// e.g., `std::random::seed`
    Path { id: NodeID, segments: Vec<Token<'a>> },
    
    /// e.g., `foo()`
    FunctionCall { id: NodeID, callee: Box<Expr<'a>>, arguments: Vec<Expr<'a>>, generic_args: Vec<Type<'a>> },
    
    /// e.g., `object::method()`
    MethodCall { id: NodeID, object: Box<Expr<'a>>, method: Token<'a>, arguments: Vec<Expr<'a>>, generic_args: Vec<Type<'a>> },
    
    /// e.g., `a + b`
    Binary { id: NodeID, left: Box<Expr<'a>>, operator: Token<'a>, right: Box<Expr<'a>> },
    
    /// e.g., `-a` or `!a`
    Unary { id: NodeID, operator: Token<'a>, right: Box<Expr<'a>> },
    
    /// e.g., `a++`
    PostfixUnary { id: NodeID, operator: Token<'a>, left: Box<Expr<'a>> },
    
    /// e.g., `a = b`
    Assignment { id: NodeID, target: Box<Expr<'a>>, operator: Token<'a>, value: Box<Expr<'a>> },
    
    /// e.g., `&mut a`
    Borrow { id: NodeID, is_mut: bool, right: Box<Expr<'a>> },
    
    /// e.g., `*a`
    Dereference { id: NodeID, right: Box<Expr<'a>> },
    
    /// e.g., `object.property`
    Member { id: NodeID, object: Box<Expr<'a>>, property: Token<'a> },
    
    /// e.g., `{1, 2, 3}`
    ArrayInitializer { id: NodeID, elements: Vec<Expr<'a>>, token: Token<'a> },
    
    /// e.g., `arr[0]`
    ArrayAccess { id: NodeID, array: Box<Expr<'a>>, index: Box<Expr<'a>>, token: Token<'a> },
    
    /// e.g., `Point { x: 1, y: 2 }`
    StructInitializer { id: NodeID, name: Box<Expr<'a>>, fields: Vec<(Token<'a>, Box<Expr<'a>>)> },
    
    /// e.g., `a as i32`
    Cast { id: NodeID, value: Box<Expr<'a>>, target: Box<Type<'a>> },

    // Control Flow Expressions
    If { id: NodeID, condition: Box<Expr<'a>>, then_branch: Block<'a>, else_branch: Option<Block<'a>> },
    While { id: NodeID, condition: Box<Expr<'a>>, body: Block<'a> },
    For { id: NodeID, variable: Token<'a>, start: Box<Expr<'a>>, end: Box<Expr<'a>>, is_inclusive: bool, body: Block<'a> },
    ForEach { id: NodeID, item: Token<'a>, iterable: Box<Expr<'a>>, body: Block<'a> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt<'a> {
    /// e.g., `let x: i32 = 5;` or `const y = 10;`
    VariableDecl {
        id: NodeID,
        is_const: bool,
        name: Token<'a>,
        type_annotation: Option<Type<'a>>,
        initializer: Box<Expr<'a>>,
    },
    
    /// An expression followed by a semicolon, e.g., `foo();`
    Expr(Box<Expr<'a>>),
    
    /// e.g., `return 5;`
    Return { id: NodeID, value: Option<Box<Expr<'a>>> },
    
    /// e.g., `break;`
    Break { id: NodeID, condition: Option<Box<Expr<'a>>> },
    
    /// e.g., `continue;`
    Continue { id: NodeID, condition: Option<Box<Expr<'a>>> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item<'a> {
    Function(FunctionDecl<'a>),
    Struct(StructDecl<'a>),
    Trait(TraitDecl<'a>),
    Extension(ExtensionDecl<'a>),
    Include(IncludeDecl<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl<'a> {
    pub id: NodeID,
    pub name: Token<'a>,
    pub annotations: Vec<Annotation>,
    pub generic_params: Vec<GenericParam<'a>>,
    pub parameters: Vec<(Token<'a>, Type<'a>)>,
    pub return_type: Option<Type<'a>>,
    pub where_clause: Option<WhereClause<'a>>,
    pub body: Option<Block<'a>>, // None for externs or trait definitions
    pub is_extern: bool,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl<'a> {
    pub id: NodeID,
    pub name: Token<'a>,
    pub generic_params: Vec<GenericParam<'a>>,
    pub where_clause: Option<WhereClause<'a>>,
    // We treat struct constants and fields separately based on your original design
    pub constants: Vec<Stmt<'a>>, // Only VariableDecl (consts) should go here
    pub fields: Vec<(Token<'a>, Type<'a>)>,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl<'a> {
    pub id: NodeID,
    pub name: Token<'a>,
    pub methods: Vec<FunctionDecl<'a>>,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionDecl<'a> {
    pub id: NodeID,
    pub target_trait: Option<Type<'a>>, // e.g., Some(RandomGenerator)
    pub target_type: Type<'a>,          // e.g., i32
    pub generic_params: Vec<GenericParam<'a>>,
    pub where_clause: Option<WhereClause<'a>>,
    pub constants: Vec<Stmt<'a>>, 
    pub methods: Vec<FunctionDecl<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncludeDecl<'a> {
    pub id: NodeID,
    pub path: Type<'a>, // Using a Type::Path 
    pub symbols: Option<Vec<Token<'a>>>,
    pub alias: Option<Token<'a>>,
}
