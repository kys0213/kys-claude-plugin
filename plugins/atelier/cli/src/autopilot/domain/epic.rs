use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epic {
    pub name: String,
    pub spec_path: PathBuf,
    pub branch: String,
    pub status: EpicStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EpicStatus {
    Active,
    Completed,
    Abandoned,
}

impl EpicStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            EpicStatus::Active => "active",
            EpicStatus::Completed => "completed",
            EpicStatus::Abandoned => "abandoned",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(EpicStatus::Active),
            "completed" => Some(EpicStatus::Completed),
            "abandoned" => Some(EpicStatus::Abandoned),
            _ => None,
        }
    }
}
