//! The single owner of cross-crate primitives: owner-only atomic file
//! replacement ([`atomic`]), bounded advisory file locks ([`lock`]), lossy
//! environment reads ([`env`](mod@env)), HTTP client construction
//! ([`http`]), the request deadline [`budget`], response [`retry`] rules and
//! the bounded body reader, the bounded stream [`read`], Gregorian day math ([`civil`]), the in-process
//! [`memo`] and keyed single [`flight`], stderr [`notice`]s, and POSIX [`shell`]
//! quoting for printed commands.
//! GitHub, runtime and CLI all use them, so they live in this leaf below
//! all three. The engine is a leaf too and keeps its own two moka caches
//! (graph facts, LSP responses).

pub mod atomic;
pub mod budget;
pub mod civil;
pub mod env;
pub mod flight;
pub mod http;
pub mod lock;
pub mod memo;
pub mod notice;
pub mod read;
pub mod retry;
pub mod shell;
