use crate::{context::DefID, types::Type };

// a concrete instantiation request for a function definition.
//
// `def_id` identifies the original declared function.
// `type_args` identifies the concrete generic substitution.
//
// Examples:
//
//     foo::<i32>
//         Instance {
//             def_id: DefID(foo),
//             type_args: [i32],
//         }
//
//     main
//         Instance {
//             def_id: DefID(main),
//             type_args: [],
//         }
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Instance {
    pub def_id: DefID,
    pub type_args: Vec<Type>,
}

impl Instance {

    pub fn new(def_id: DefID, type_args: Vec<Type>) -> Self {
        Self {
            def_id,
            type_args,
        }
    }

    pub fn monomorphic(def_id: DefID) -> Self {
        Self {
            def_id,
            type_args: Vec::new(),
        }
    }
}
