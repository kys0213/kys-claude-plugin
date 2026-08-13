//! Helpers both subsystems need. `git` and `session` are siblings — neither
//! owns the other — so anything they share lives here rather than one of them
//! reaching into the other's internals.

pub mod process;
pub mod shell;
