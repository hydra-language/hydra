
// ===========================================================================
// TOKEN DEFINITIONS
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Literals
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    CharLiteral(char),
    BoolLiteral(bool),

    // Identifer
    Identifier(String),

    // Keywords
    Let,
    Const,
    Function, // fn
    Return,
    In,
    As,
    If,
    ElseIf,
    Else,
    For,
    ForEach,
    While,
    Break,
    Skip,
    Include,
    Typedef,
    None,

    // Operators
    Assign,           // =
    Equal,            // ==
    NotEqual,         // !=
    LessEqual,        // <=
    GreaterEqual,     // >=
    Plus,             // +
    Minus,            // -
    Multiply,         // *
    Divide,           // /
    Modulo,           // %
    Increment,        // ++
    Decrement,        // --
    PlusAssign,       // +=
    MinusAssign,      // -=
    MultiplyAssign,   // *=
    DivideAssign,     // /=
    ModuloAssign,     // %=
    And,              // &&
    Or,               // ||
    Not,              // !
    Arrow,            // ->

    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,
    HeapPointerBar,     // |
    Semicolon,
    Comma,
    Dot,                // .
    RangeExclusive,     // ..
    RangeInclusive,     // ..=
    Ellipsis,          // ...
    Colon,              // :
    DoubleColon,        // ::
    Optional,           // ?
    Reference,          // &

    // Special
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token<'a> {
    pub token_type: TokenType,
    pub lexeme: &'a str,
    pub line: usize,
    pub column: usize,
}
