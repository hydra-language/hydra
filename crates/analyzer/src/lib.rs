pub mod scope;
pub mod stmt;
pub mod expr;
pub mod types;
pub mod utils;
pub mod analyzer;
pub mod fold;
pub mod monomorphizer;
pub mod resolve;

mod intrinsics;
mod passes;

pub use analyzer::Analyzer;
pub use resolve::Resolver;
pub use scope::{NameResolver, Namespace, Scope};
