use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// A `.claude/rules` document reduced to what decides where it applies.
#[derive(Debug, Clone)]
pub struct Rule {
    path: String,
    scope: Scope,
}

#[derive(Debug, Clone)]
enum Scope {
    Always,
    Paths(GlobSet),
}

impl Rule {
    /// No frontmatter, or frontmatter without `paths:`, means the rule loads
    /// for every file. Errors carry `path` so the caller can name the file.
    pub fn parse(path: &str, content: &str) -> Result<Rule, String> {
        let fail = |reason: String| format!("{path}: {reason}");
        let scope = match frontmatter(content).map_err(fail)? {
            None => Scope::Always,
            Some(lines) => match paths_value(&lines).map_err(fail)? {
                None => Scope::Always,
                Some(globs) => Scope::Paths(compile(&globs).map_err(fail)?),
            },
        };
        Ok(Rule {
            path: path.to_string(),
            scope,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn is_always(&self) -> bool {
        matches!(self.scope, Scope::Always)
    }

    /// Always-scoped rules answer `false`: they apply everywhere, so no single
    /// file is what selects them.
    pub fn matches(&self, file: &str) -> bool {
        match &self.scope {
            Scope::Always => false,
            Scope::Paths(set) => set.is_match(file),
        }
    }
}

fn frontmatter(content: &str) -> Result<Option<Vec<&str>>, String> {
    let mut lines = content.trim_start_matches('\u{feff}').lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Ok(None);
    }
    let mut body = Vec::new();
    for line in lines {
        if line.trim_end() == "---" {
            return Ok(Some(body));
        }
        body.push(line);
    }
    Err("frontmatter has no closing `---`".to_string())
}

fn paths_value(lines: &[&str]) -> Result<Option<Vec<String>>, String> {
    let Some(start) = lines.iter().position(|l| l.starts_with("paths:")) else {
        return Ok(None);
    };
    let inline = lines[start]["paths:".len()..].trim();
    let globs = if inline.is_empty() {
        block_items(&lines[start + 1..])?
    } else if let Some(list) = inline.strip_prefix('[') {
        let list = list
            .strip_suffix(']')
            .ok_or("`paths:` inline list has no closing `]`")?;
        split_inline(list).into_iter().map(|s| scalar(&s)).collect()
    } else {
        vec![scalar(inline)]
    };
    let globs: Vec<String> = globs.into_iter().filter(|g| !g.is_empty()).collect();
    if globs.is_empty() {
        return Err("`paths:` has no entries".to_string());
    }
    Ok(Some(globs))
}

/// Reads `- item` lines up to the next top-level key.
fn block_items(lines: &[&str]) -> Result<Vec<String>, String> {
    let mut items = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match trimmed.strip_prefix('-') {
            Some(item) => items.push(scalar(item.trim())),
            None if line.starts_with(char::is_whitespace) => {
                return Err(format!("unsupported `paths:` entry: {trimmed}"));
            }
            None => break,
        }
    }
    Ok(items)
}

/// Splits on commas outside quotes and `{}` — `{ts,tsx}` is one glob.
fn split_inline(list: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut depth = 0usize;
    for c in list.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '{') => depth += 1,
            (None, '}') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    parts.push(current);
    parts
}

fn scalar(raw: &str) -> String {
    let raw = raw.trim();
    for q in ['"', '\''] {
        if let Some(rest) = raw.strip_prefix(q) {
            if let Some(end) = rest.find(q) {
                return rest[..end].to_string();
            }
        }
    }
    let unquoted = raw.split(" #").next().unwrap_or_default();
    unquoted.trim().to_string()
}

fn compile(globs: &[String]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for glob in globs {
        let compiled = GlobBuilder::new(glob)
            .literal_separator(true)
            .build()
            .map_err(|e| format!("invalid glob `{glob}`: {e}"))?;
        builder.add(compiled);
    }
    builder.build().map_err(|e| e.to_string())
}
