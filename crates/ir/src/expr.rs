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
        args: Vec<Expr>
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    ADD, SUB, MUL, DIV, MOD,
    EQ, NE, LT, GT, LE, GE,
    AND, OR
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::ADD => "+",
            BinaryOp::SUB => "-",
            BinaryOp::MUL => "*",
            BinaryOp::DIV => "/",

            _ => ""
        };

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    NEG, NOT
}

impl ExprKind {

    pub fn pretty_print(&self, indent: usize) -> String {
        match self {
            ExprKind::INT_LITERAL(val) => val.to_string(),
            ExprKind::STRING_LITERAL(s) => format!("{:?}", s),
            ExprKind::VariableReference { name } => name.clone(),
            ExprKind::Binary { op, lhs, rhs } => {
                format!("{} {} {}", lhs.pretty_print(indent), op, rhs.pretty_print(indent))
            },

            ExprKind::ArrayInit { elements } => {
                let elems: Vec<String> = elements.iter()
                    .map(|e| e.pretty_print(indent))
                    .collect();
                format!("{{ {} }}", elems.join(", "))
            },
            
            ExprKind::Call { callee, args } => {
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

            _ => "".to_string()
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
        format!("({} : {})", self.kind.pretty_print(indent), self.ty)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({} : {})", self.kind, self.ty)
    }
}
