mod mutation;
mod projection;

#[cfg(test)]
pub(crate) use mutation::recompute_dependency_state;
pub(in crate::decomposition) use mutation::*;
pub use projection::show_decomposition_plan;
pub(in crate::decomposition) use projection::*;
