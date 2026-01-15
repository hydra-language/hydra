use lexer::Token;

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
        parameters: Vec<(Token<'a>, Box<ASTNode<'a>>)>,
        return_type: Box<ASTNode<'a>>,
        body: Vec<ASTNode<'a>>,
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

    WhileLoop {
        condition: Box<ASTNode<'a>>,
        body: Vec<ASTNode<'a>>
    },
}
