#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // -----------------------------------------------------------------------
    // Literals
    // -----------------------------------------------------------------------
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    CharLiteral(char),
    BoolLiteral(bool),

    // -----------------------------------------------------------------------
    // Identifier
    // -----------------------------------------------------------------------
    Identifier(String),

    // -----------------------------------------------------------------------
    // Primitive Types
    // -----------------------------------------------------------------------
    ISize,
    I8,
    I16,
    I32,
    I64,
    USize,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Char,
    Bool,

    // -----------------------------------------------------------------------
    // Keywords
    // -----------------------------------------------------------------------
    Let,
    Const,
    Function,               // fn
    Struct,
    Extension,
    Return,
    In,
    As,
    On,
    If,
    Else,
    For,
    ForEach,
    While,
    Match,
    Break,
    Continue,
    Include,                // for imports
    Typedef,                // for aliasing predefined types and others
    Trait,
    AnySize,                // comptime generic used in function parameters of arrays
    AnyType,                // comptime generic used in function parameters, return types and struct fields
    None,

    // -----------------------------------------------------------------------
    // Operators
    // -----------------------------------------------------------------------

    // Assignment & compound assignment
    Equal,              // =
    DoubleEqual,        // ==
    ExclamEqual,        // !=
    LessEqual,          // <=
    GreaterEqual,       // >=
    PlusEqual,          // +=
    MinusEqual,         // -=
    StarEqual,          // *=
    ForwardSlashEqual,  // /=
    ModuloEqual,        // %=
    AmpersandEqual,     // &=
    PipeEqual,          // |=
    CarrotEqual,        // ^=
    DoubleLeftEqual,    // <<=
    DoubleRightEqual,   // >>=

    // Arithmetic
    Plus,               // +
    Minus,              // -
    Star,               // *
    ForwardSlash,       // /
    Modulo,             // %
    PlusPlus,           // ++
    MinusMinus,         // --

    // Bitwise and References ( & )
    Ampersand,          // &
    Pipe,               // |
    Carrot,             // ^
    DoubleLeftAngle,    // <<
    DoubleRightAngle,   // >>

    // Logical
    DoubleAmpersand,    // &&
    DoublePipe,         // ||
    ExclamationMark,    // !

    // Comparison
    LeftAngle,          // <
    RightAngle,         // >

    // Other operators
    Arrow,              // ->
    EqualArrow,         // =>
    Dot,                // .
    DoubleDot,          // ..
    DoubleDotEqual,     // ..=
    TripleDot,          // ...
    QuestionMark,       // ?

    // -----------------------------------------------------------------------
    // Punctuation
    // -----------------------------------------------------------------------
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Semicolon,
    Comma,
    Colon,              // :
    DoubleColon,        // ::

    // -----------------------------------------------------------------------
    // Special
    // -----------------------------------------------------------------------
    EOF,
}

#[derive(Debug, Clone)]
pub struct Token<'a> {
    pub token_type: TokenType,
    pub lexeme: &'a str,
    pub line: usize,
    pub column: usize,
}
