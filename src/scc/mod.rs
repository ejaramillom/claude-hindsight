//! Semantic Context Capsule (SCC) - A token-efficient context serialization tool.
//!
//! SCC implements a compiled semantic graph representation for LLM context,
//! prioritizing token minimization and deterministic reproduction.

pub mod symtable;
pub mod graph;
pub mod capsule;

#[cfg(test)]
mod tests;

pub use capsule::Capsule;
pub use symtable::SymTable;
pub use graph::State;
