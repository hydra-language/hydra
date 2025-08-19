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
    Function,           // fn
    Struct,
    Return,
    In,
    As,
    If,
    Else,
    For,
    ForEach,
    While,
    Break,
    Skip,
    Include,
    Typedef,
    Size,
    None,

    // Operators
    Assign,             // =
    Equal,              // ==
    NotEqual,           // !=
    LessEqual,          // <=
    GreaterEqual,       // >=
    Plus,               // +
    Minus,              // -
    Multiply,           // *
    Divide,             // /
    Modulo,             // %
    Increment,          // ++
    Decrement,          // --
    PlusAssign,         // +=
    MinusAssign,        // -=
    MultiplyAssign,     // *=
    DivideAssign,       // /=
    ModuloAssign,       // %=
    And,                // &&
    Or,                 // ||
    Not,                // !
    Arrow,              // ->
    EqualArrow,         // =>
    RangeExclusive,     // ..
    RangeInclusive,     // ..=
    Pipe,               // |

    // Bitwise
    // BitwiseAnd,      // &
    // BitwiseOr,       // |
    BitwiseXor,         // ^
    BitShiftLeft,       // <<
    BitShiftRight,      // >>
    BitAndAssign,       // &=
    BitOrAssign,        // |=
    BitXorAssign,       // ^=
    ShiftAssignLeft,    // <<=
    ShiftAssignRight,   // >>=  


    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftAngle,          // <
    RightAngle,         // > 
    Semicolon,
    Comma,
    Dot,                // .
    Ellipsis,           // ...
    Colon,              // :
    DoubleColon,        // ::
    Optional,           // ?
    Ampersand,          // &

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
