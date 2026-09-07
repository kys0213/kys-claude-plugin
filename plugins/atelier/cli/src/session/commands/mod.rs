pub mod baseline;
pub mod payload;
pub mod push_check;
pub mod simplify;

use crate::session::core::baseline::BaselineStore;
use crate::session::core::repo::RepoReader;

/// Everything the session commands need from the outside world. Injected so
/// the decision logic can be exercised entirely in memory.
pub struct SessionDeps<'a> {
    pub store: &'a dyn BaselineStore,
    pub repo: &'a dyn RepoReader,
}
