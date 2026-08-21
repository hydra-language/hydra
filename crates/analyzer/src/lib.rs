pub mod scope;
pub mod expr;
pub mod stmt;
pub mod types;
pub mod utils;
pub mod analyzer;
pub mod fold;
pub mod monomorphizer;
pub mod resolve;

pub use analyzer::Analyzer;
pub use resolve::Resolver;
pub use scope::{NameResolver, Namespace, Scope};
