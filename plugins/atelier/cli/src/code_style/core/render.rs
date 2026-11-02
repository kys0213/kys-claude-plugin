use crate::code_style::core::profile::Profile;
use crate::code_style::core::role::Role;
use std::collections::BTreeMap;

const FENCE: &str = "---";
const ROLES_KEY: &str = "roles:";
const PATHS_KEY: &str = "paths:";

/// Turns a common rule's `roles:` line into the `paths:` list the given
/// languages' profiles map those roles to. Every other byte is kept.
pub fn render_common(source: &str, profiles: &BTreeMap<String, Profile>) -> Result<String, String> {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let (roles_at, roles) = find_roles(&lines)?;
    let globs = globs_for(&roles, profiles);
    if globs.is_empty() {
        let roles: Vec<&str> = roles.iter().map(Role::as_str).collect();
        let langs: Vec<&str> = profiles.keys().map(String::as_str).collect();
        return Err(format!(
            "roles [{}] match no path for languages: {}",
            roles.join(", "),
            langs.join(", ")
        ));
    }

    let mut out = String::with_capacity(source.len());
    for (i, line) in lines.iter().enumerate() {
        if i == roles_at {
            out.push_str(PATHS_KEY);
            out.push('\n');
            for glob in &globs {
                out.push_str(&format!("  - \"{glob}\"\n"));
            }
        } else {
            out.push_str(line);
        }
    }
    Ok(out)
}

fn find_roles(lines: &[&str]) -> Result<(usize, Vec<Role>), String> {
    let missing = || "a common rule needs a frontmatter with roles".to_string();
    if lines.first().map(|l| l.trim_end()) != Some(FENCE) {
        return Err(missing());
    }
    let mut found = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        let line = line.trim_end();
        if line == FENCE {
            return found.ok_or_else(missing);
        }
        if line.starts_with(PATHS_KEY) {
            return Err("a common rule declares roles, not paths".to_string());
        }
        if let Some(value) = line.strip_prefix(ROLES_KEY) {
            found = Some((i, parse_roles(value)?));
        }
    }
    Err(missing())
}

fn parse_roles(value: &str) -> Result<Vec<Role>, String> {
    let inner = value
        .trim()
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .ok_or_else(|| {
            format!("roles must be an inline list, e.g. roles: [source] — got{value}")
        })?;
    let roles = inner
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(Role::parse)
        .collect::<Result<Vec<_>, _>>()?;
    if roles.is_empty() {
        return Err("roles must list at least one role, e.g. roles: [source]".to_string());
    }
    Ok(roles)
}

fn globs_for(roles: &[Role], profiles: &BTreeMap<String, Profile>) -> Vec<String> {
    let mut globs: Vec<String> = Vec::new();
    for profile in profiles.values() {
        for role in roles {
            for glob in profile.globs(*role) {
                if !globs.contains(glob) {
                    globs.push(glob.clone());
                }
            }
        }
    }
    globs
}
