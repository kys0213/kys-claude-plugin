/// The kind of file a common rule applies to. Each language profile maps
/// every role to that language's path globs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    Config,
    Boundary,
    Test,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Source, Role::Config, Role::Boundary, Role::Test];

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Source => "source",
            Role::Config => "config",
            Role::Boundary => "boundary",
            Role::Test => "test",
        }
    }

    pub fn parse(name: &str) -> Result<Role, String> {
        Role::ALL
            .into_iter()
            .find(|role| role.as_str() == name)
            .ok_or_else(|| {
                let names: Vec<&str> = Role::ALL.iter().map(Role::as_str).collect();
                format!(
                    "unknown role: {name} — expected one of: {}",
                    names.join(", ")
                )
            })
    }
}
