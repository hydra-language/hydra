use lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub enum ASTNode<'a> {
    VariableDeclaration {
        is_const: bool,
        name: Token<'a>,
        type_annotation: Option<Token<'a>>,
        initializer: Box<ASTNode<'a>>,
    },

    FunctionDeclaration {
        name: Token<'a>,
        parameters: Vec<(Token<'a>, Token<'a>)>,
        return_type: Token<'a>,
        body: Vec<ASTNode<'a>>,
    },

    ReturnStatement {
        value: Box<ASTNode<'a>>,
    },

    VariableExpression {
        name: Token<'a>,
    },

    FunctionCallExpression {
        name: Token<'a>,
        arguments: Vec<ASTNode<'a>>,
    },

    Primtive {
        token: Token<'a>,
    },

    Expression {
        token: Token<'a>,
    },
}
