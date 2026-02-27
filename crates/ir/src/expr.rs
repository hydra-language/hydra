use std::fmt;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    INT_LITERAL(i64),
    FLOAT_LITERAL(f64),
    BOOL_LITERAL(bool),
    STRING_LITERAL(String),
    CHAR_LITERAL(char),

    Assignment {
        target: Box<Expr>,
        value: Box<Expr>,
    },

    VariableReference {
        name: String
    },

    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>
    },

    Unary {
        op: UnaryOp,
        operand: Box<Expr>
    },

    Call {
        callee: String,
        args: Vec<Expr>,
        generic_args: Vec<Type>,
    },

    Cast {
        expr: Box<Expr>
    },

    Index {
        target: Box<Expr>,
        index: Box<Expr>
    },

    ArrayInit {
        elements: Vec<Expr>
    },

    ArrayAccess {
        array: Box<Expr>,
        index: Box<Expr>,
    },

    StructInit {
        name: String,
        values: Vec<Expr>,
    },

    MemberAccess {
        object: Box<Expr>,
        member: String,
        index: u32, // The field position for LLVM offsets
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    ADD, SUB, MUL, DIV, MOD,
    EQ, NE, LT, GT, LE, GE,
    AND, OR
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    NEG, NOT, ADDR_OF, DEREF,
}

impl ExprKind {

    pub fn pretty_print(&self, indent: usize) -> String {
        match self {
            ExprKind::INT_LITERAL(val) => val.to_string(),
            ExprKind::FLOAT_LITERAL(val) => val.to_string(),
            ExprKind::BOOL_LITERAL(val) => val.to_string(),
            ExprKind::STRING_LITERAL(s) => format!("{:?}", s),
            ExprKind::CHAR_LITERAL(c) => format!("{:?}", c),

            ExprKind::Assignment { target, value } => {
                format!("{} = {}", target.pretty_print(indent), value.pretty_print(indent))
            },

            ExprKind::VariableReference { name } => name.clone(),

            ExprKind::Binary { op, lhs, rhs } => {
                format!("{} {} {}", lhs.pretty_print(indent), op, rhs.pretty_print(indent))
            },

            ExprKind::Unary { op, operand } => {
                format!("{}{}", op, operand.pretty_print(indent))
            },

            ExprKind::ArrayInit { elements } => {
                let elems: Vec<String> = elements.iter()
                    .map(|e| e.pretty_print(indent))
                    .collect();
                format!("{{ {} }}", elems.join(", "))
            },

            ExprKind::ArrayAccess { array, index } => {
                format!("{}[{}]", array.pretty_print(indent), index.pretty_print(indent))
            },

            ExprKind::Index { target, index } => {
                format!("{}[{}]", target.pretty_print(indent), index.pretty_print(indent))
            },

            ExprKind::Cast { expr } => {
                expr.pretty_print(indent)
            },
            
            ExprKind::Call { callee, args, generic_args: _ } => {
                if args.is_empty() {
                    return format!("{}()", callee);
                }

                let mut s = format!("{}(\n", callee);
                let arg_indent = "  ".repeat(indent + 1);
                let closing_indent = "  ".repeat(indent);

                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        s.push_str(",\n");
                    }
                    s.push_str(&format!("{}{}", arg_indent, arg.pretty_print(indent + 1)));
                }
                s.push_str(&format!("\n{})", closing_indent));

                s
            }

            ExprKind::StructInit { name, values } => {
                let inner_indent = "  ".repeat(indent + 1);
                let closing_indent = "  ".repeat(indent);
                let fields: Vec<String> = values.iter()
                    .map(|v| format!("{}{}", inner_indent, v.pretty_print(indent + 1)))
                    .collect();
                
                format!("{} {{\n{}\n{}}}", name, fields.join(",\n"), closing_indent)
            },

            ExprKind::MemberAccess { object, member, .. } => {
                format!("{}.{}", object.pretty_print(indent), member)
            },
        }
    }
}

impl fmt::Display for ExprKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_print(0))
    }
}

impl Expr {
    pub fn pretty_print(&self, indent: usize) -> String {
        self.kind.pretty_print(indent)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({} : {})", self.kind, self.ty)
    }
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::ADD => "+",
            BinaryOp::SUB => "-",
            BinaryOp::MUL => "*",
            BinaryOp::DIV => "/",
            BinaryOp::MOD => "%",
            
            BinaryOp::EQ => "==",
            BinaryOp::NE => "!=",
            BinaryOp::LT => "<",
            BinaryOp::GT => ">",
            BinaryOp::LE => "<=",
            BinaryOp::GE => ">=",
            
            BinaryOp::AND => "&&",
            BinaryOp::OR  => "||",
        };

        write!(f, "{}", s)
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::NEG => write!(f, "-"),
            UnaryOp::NOT => write!(f, "!"),
            UnaryOp::ADDR_OF => write!(f, "&"),
            UnaryOp::DEREF => write!(f, "*"),
        }
    }
}
