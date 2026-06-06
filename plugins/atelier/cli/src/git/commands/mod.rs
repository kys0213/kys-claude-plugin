//! `atelier git` command layer (ported from git-utils `src/commands/`).
//! Each command validates input, delegates to the core services, and returns
//! `Result<Output, String>` (the `{ ok, data } | { ok, error }` union in TS).

pub mod branch;
pub mod commit;
