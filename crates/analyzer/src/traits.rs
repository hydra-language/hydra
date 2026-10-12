use std::collections::HashMap;

use errors::error::{HydraError, Span};
use ir::context::{DefID, TraitBound};
use ir::types::Type;
use parser::WhereClause;

use crate::Analyzer;
use crate::solver::TraitSolver;

pub const DROP_TRAIT_PATH: &str = "core::ops::drop::Drop";
pub const COPY_TRAIT_PATH: &str = "core::marker::copy::Copy";
pub const DISPLAY_TRAIT_PATH: &str = "core::fmt::display::Display";
pub const DEREF_TRAIT_PATH: &str = "core::ops::deref::Deref";
pub const DEREF_MUT_TRAIT_PATH: &str = "core::ops::deref::DerefMut";

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn trait_solver(&self) -> TraitSolver<'_> {
        TraitSolver::new(
            &self.context.trait_impls,
            &self.context.trait_impl_index,
            self.context.find_trait_by_path(COPY_TRAIT_PATH),
        )
    }

    pub(crate) fn lower_where_clause_bounds(&mut self, where_clause: Option<&WhereClause>) -> Result<Vec<TraitBound>, HydraError> 
    {
        let Some(where_clause) = where_clause else {
            return Ok(Vec::new());
        };

        let mut bounds = Vec::new();

        for predicate in &where_clause.predicates {
            let target_type = self.lower_type(&predicate.target_type)?;

            for bound_trait in &predicate.bound_traits {
                let trait_def_id = self.resolve_trait_def_id(bound_trait)?;

                bounds.push(TraitBound {
                    target_type: target_type.clone(),
                    trait_def_id,
                });
            }
        }

        Ok(bounds)
    }

    pub(crate) fn validate_trait_bounds(&self, bounds: &[TraitBound], substitutions: &HashMap<String, Type>, span: Span)
        -> Result<(), HydraError> 
    {
        for bound in bounds {
            let target_type = bound.target_type.substitute(substitutions);

            if self.trait_solver().type_implements_trait(&target_type, bound.trait_def_id, &self.current_trait_bounds) {
                continue;
            }

            let trait_name = self.trait_name(bound.trait_def_id);

            let mut error = self.error(
                "S028",
                format!("type `{target_type}` does not implement `{trait_name}`"),
                span,
            );

            if target_type.contains_generic() {
                error = error.with_help(format!("add a `where {target_type}: {trait_name}` bound"));
            } else {
                error = error.with_help(format!("implement `{trait_name}` for `{target_type}`"));
            }

            return Err(error);
        }

        Ok(())
    }

    pub(crate) fn trait_name(&self, trait_def_id: DefID) -> String {
        let Some(info) = self.context.get_def(trait_def_id) else {
            return format!("{trait_def_id}");
        };

        if info.absolute_path.is_empty() {
            info.name.clone()
        } else {
            info.absolute_path.join("::")
        }
    }
}

