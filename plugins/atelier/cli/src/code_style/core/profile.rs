use crate::code_style::core::role::Role;
use serde::Deserialize;

/// A language's role → path globs mapping, read from its `profile.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    roles: RoleGlobs,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleGlobs {
    source: Vec<String>,
    config: Vec<String>,
    boundary: Vec<String>,
    test: Vec<String>,
}

impl Profile {
    pub fn parse(json: &str) -> Result<Profile, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn globs(&self, role: Role) -> &[String] {
        match role {
            Role::Source => &self.roles.source,
            Role::Config => &self.roles.config,
            Role::Boundary => &self.roles.boundary,
            Role::Test => &self.roles.test,
        }
    }
}
