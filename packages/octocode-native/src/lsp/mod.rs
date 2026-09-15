//! Process-local language-server pool owned by the native runtime.
//!
//! Tool handlers consume this service. They must not create independent
//! language-server clients or import TypeScript engine wrappers.

mod pool;

pub use pool::LspPool;
