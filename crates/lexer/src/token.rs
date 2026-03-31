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
    IDENTIFIER(String),

    // -----------------------------------------------------------------------
    // Primitive Types
    // -----------------------------------------------------------------------
    ISIZE,
    I8,
    I16,
    I32,
    I64,
    USIZE,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    CHAR,
    BOOL,

    // -----------------------------------------------------------------------
    // Keywords
    // -----------------------------------------------------------------------
    LET,
    CONST,
    FN,                     // fn
    STRUCT,
    EXTENSION,
    RETURN,
    IN,
    AS,
    ON,
    IF,
    ELSE,
    FOR,
    FOREACH,
    WHILE,
    MATCH,
    BREAK,
    CONTINUE,
    INCLUDE,                // for imports
    TYPEDEF,                // for aliasing predefined types and others
    TRAIT,
    ANYSIZE,                // comptime generic used in function parameters of arrays
    ANYTYPE,                // comptime generic used in function parameters, return types and struct fields
    EXTERN,                 // extern
    PUB,                    // pub
    NONE,

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
    Hash,

    // -----------------------------------------------------------------------
    // Special
    // -----------------------------------------------------------------------
    EOF,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token<'a> {
    pub token_type: TokenType,
    pub lexeme: &'a str,
    pub line: usize,
    pub column: usize,
}
