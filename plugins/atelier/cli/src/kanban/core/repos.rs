//! The board's cards grouped by repository.

use super::board::{Action, Board, Column};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSession {
    pub session_id: String,
    pub column: Column,
    pub cwd: String,
    pub permission_mode: Option<String>,
    pub actions: Vec<Action>,
    pub subtitle: Option<String>,
    pub last_line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub label: String,
    /// The main working tree; `None` groups a session outside any repository
    /// by its directory.
    pub root: Option<String>,
    pub sessions: Vec<RepoSession>,
}

/// Groups in label order; sessions keep the board's column and card order.
pub fn group(board: &Board) -> Vec<Repo> {
    let mut repos: Vec<(String, Repo)> = Vec::new();
    for (column, cards) in &board.columns {
        for card in cards {
            let key = card.repo_root.clone().unwrap_or_else(|| card.cwd.clone());
            let session = RepoSession {
                session_id: card.session_id.clone(),
                column: *column,
                cwd: card.cwd.clone(),
                permission_mode: card.permission_mode.clone(),
                actions: card.actions.clone(),
                subtitle: card.subtitle.clone(),
                last_line: card.last_line.clone(),
            };
            match repos.iter_mut().find(|(k, _)| *k == key) {
                Some((_, repo)) => repo.sessions.push(session),
                None => repos.push((
                    key,
                    Repo {
                        label: card.label.clone(),
                        root: card.repo_root.clone(),
                        sessions: vec![session],
                    },
                )),
            }
        }
    }
    let mut repos: Vec<Repo> = repos.into_iter().map(|(_, repo)| repo).collect();
    repos.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.root.cmp(&b.root)));
    repos
}
