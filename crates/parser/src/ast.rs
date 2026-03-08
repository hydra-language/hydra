use lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ASTNode<'a> {
    VariableDeclaration {
        is_const: bool,
        name: Token<'a>,
        type_annotation: Option<Box<ASTNode<'a>>>,
        initializer: Box<ASTNode<'a>>,
    },

    FunctionDeclaration {
        name: Token<'a>,
        annotations: Vec<Annotation>,
        generic_params: Vec<Token<'a>>,
        parameters: Vec<(Token<'a>, Box<ASTNode<'a>>)>,
        return_type: Box<ASTNode<'a>>,
        body: Vec<ASTNode<'a>>,
        is_extern: bool,
    },

    ReturnStatement {
        value: Box<ASTNode<'a>>,
    },

    TypeIdentifier {
        type_token: Token<'a>,
    },

    VariableExpression {
        name: Token<'a>,
    },

    FunctionCallExpression {
        name: Token<'a>,
        arguments: Vec<ASTNode<'a>>,
        generic_args: Vec<ASTNode<'a>>,
    },

    BinaryExpression {
        left: Box<ASTNode<'a>>,
        operator: Token<'a>,
        right: Box<ASTNode<'a>>
    },

    AssignmentExpression {
        target: Box<ASTNode<'a>>,
        operator: Token<'a>,
        value: Box<ASTNode<'a>>
    },

    UnaryExpression {
        operator: Token<'a>,
        right: Box<ASTNode<'a>>,
    },

    PostfixUnaryExpression {
        operator: Token<'a>,
        left: Box<ASTNode<'a>>,
    },

    MemberExpression {
        object: Box<ASTNode<'a>>,
        property: Token<'a>,
    },

    ArrayType {
        element_type: Box<ASTNode<'a>>,
        size: Box<ASTNode<'a>>,
        token: Token<'a>, 
    },

    ArrayInitializer {
        elements: Vec<ASTNode<'a>>,
        token: Token<'a>
    },

    ArrayAccess {
        array: Box<ASTNode<'a>>,
        index: Box<ASTNode<'a>>,
        token: Token<'a>
    },

    Primtive {
        token: Token<'a>,
    },

    Expression {
        token: Token<'a>,
    },

    IfStatement {
        condition: Box<ASTNode<'a>>,
        then_branch: Vec<ASTNode<'a>>,
        else_branch: Option<Vec<ASTNode<'a>>>,
    },

    Break {
        condition: Option<Box<ASTNode<'a>>>,
    },

    Continue {
        condition: Option<Box<ASTNode<'a>>>,
    },

    ForLoop {
        variable: Token<'a>,
        start: Box<ASTNode<'a>>,
        end: Box<ASTNode<'a>>,
        is_inclusive: bool,
        body: Vec<ASTNode<'a>>,
    },

    ForEach {
        item: Token<'a>,
        iterable: Box<ASTNode<'a>>,
        body: Vec<ASTNode<'a>>,
    },

    WhileLoop {
        condition: Box<ASTNode<'a>>,
        body: Vec<ASTNode<'a>>
    },

    StructDeclaration {
        name: Token<'a>,
        generic_params: Vec<Token<'a>>,
        constants: Vec<ASTNode<'a>>,
        fields: Vec<(Token<'a>, Box<ASTNode<'a>>)>,
        methods: Vec<ASTNode<'a>>,
    },

    StructInitializer {
        name: Token<'a>,
        fields: Vec<(Token<'a>, Box<ASTNode<'a>>)>,
    },

    MethodCallExpression {
        object: Box<ASTNode<'a>>,
        method: Token<'a>,
        arguments: Vec<ASTNode<'a>>,
        generic_args: Vec<ASTNode<'a>>,
    },

    CastExpression {
        value: Box<ASTNode<'a>>,
        target: Box<ASTNode<'a>>,
    },

    Reference { 
        inner: Box<ASTNode<'a>> 
    },

    ConstReference { 
        inner: Box<ASTNode<'a>> 
    },

    Pointer {
        inner: Box<ASTNode<'a>>,
    },

    GenericType {
        base: Box<ASTNode<'a>>,
        args: Vec<ASTNode<'a>>,
    },
}
