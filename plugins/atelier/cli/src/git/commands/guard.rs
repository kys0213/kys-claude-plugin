//! `guard` command glue — port of `git-utils/src/commands/guard.ts`. The TS
//! module only re-declares types; the decision logic lives in
//! `core::guard::GuardService`. This thin wrapper runs a guard check so the
//! CLI layer stays declarative.

use crate::git::core::guard::GuardService;
use crate::git::types::{GuardInput, GuardOutput};

pub struct GuardCommandDeps<'a> {
    pub guard: &'a dyn GuardService,
}

/// Runs the guard check.
pub fn run(deps: &GuardCommandDeps, input: &GuardInput) -> GuardOutput {
    deps.guard.check(input)
}
