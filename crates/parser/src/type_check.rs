use crate::ASTNode;

pub struct TypeChecker<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> TypeChecker<'a> {
    pub fn new() -> Self {
        TypeChecker {
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn check(&mut self, ast: &Vec<ASTNode<'a>>) -> Result<(), String> {
        for node in ast {
            self.check_node(node)?;
        }

        Ok(())
    }

    fn check_node(&mut self, node: &ASTNode<'a>) -> Result<(), String> {
        match node {
            ASTNode::FunctionDeclaration { name: _, parameters, return_type, body } => {
                self.validate_type(&return_type.lexeme)?;

                for (_param_name, param_type) in parameters {
                    self.validate_type(&param_type.lexeme)?;
                }

                for stmt in body {
                    self.check_node(stmt)?;
                }
            }

            ASTNode::VariableDeclaration { is_const: _, name: _, type_annotation, initializer } => {
                if let Some(type_tok) = type_annotation {
                    self.validate_type(&type_tok.lexeme)?;
                }

                self.check_node(initializer)?
            }

            ASTNode::ReturnStatement { value } => {
                self.check_node(value)?;
            }

            ASTNode::FunctionCallExpression { name: _, arguments } => {
                for arg in arguments {
                    self.check_node(arg)?;
                }
            }

            ASTNode::Expression { token: _ } => {
            }

            ASTNode::VariableExpression { name: _ } => {
            }

            ASTNode::Primtive { token: _ } => {
            }
        }

        Ok(())
    }

    fn validate_type(&self, type_name: &str) -> Result<(), String> {
        if self.is_primitive(type_name) {
            Ok(())
        } else {
            Err(format!("error: unknown type '{}'", type_name))
        }
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        matches!(type_name,
            "i8" | "i16" | "i32" | "i64" | "isize" |
            "u8" | "u16" | "u32" | "u64" | "usize" |
            "f32" | "f64" |
            "bool" | "char" | "void"
        )
    }
}
