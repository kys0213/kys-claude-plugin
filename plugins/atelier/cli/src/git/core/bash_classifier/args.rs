use std::path::{Component, Path, PathBuf};

use super::lexer::Word;
use super::Anchor;

#[derive(Debug, Clone)]
pub(super) enum Place {
    Known(PathBuf),
    Dynamic {
        literal_prefix: Option<PathBuf>,
        raw: String,
    },
}

impl Place {
    pub(super) fn unknown(raw: &str) -> Place {
        Place::Dynamic {
            literal_prefix: None,
            raw: raw.to_string(),
        }
    }

    pub(super) fn to_anchor(&self) -> Anchor {
        match self {
            Place::Known(p) => Anchor::Path(p.clone()),
            Place::Dynamic {
                literal_prefix,
                raw,
            } => Anchor::Unresolved {
                literal_prefix: literal_prefix.clone(),
                raw: raw.clone(),
            },
        }
    }
}

pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub(super) fn is_device(path: &Path) -> bool {
    path.starts_with("/dev/fd")
        || matches!(
            path.to_str(),
            Some("/dev/null" | "/dev/stdin" | "/dev/stdout" | "/dev/stderr" | "/dev/tty")
        )
}

pub(super) fn program_name(w: &Word) -> &str {
    w.text.rsplit('/').next().unwrap_or("")
}

pub(super) fn assignment(w: &Word) -> Option<(String, String)> {
    let (name, value) = w.text.split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

pub(super) fn split_eq(s: &str) -> (&str, bool) {
    match s.split_once('=') {
        Some((name, _)) => (name, true),
        None => (s, false),
    }
}

/// Value of a short option inside a bundle: the attached rest, else the next
/// argument (reported as one extra consumed word).
pub(super) fn short_value(w: &Word, after: usize, next: Option<&Word>) -> (Option<Word>, usize) {
    if after < w.text.len() {
        (Some(w.slice_from(after)), 0)
    } else {
        (next.cloned(), 1)
    }
}

#[derive(Default)]
pub(super) struct Parsed {
    pub(super) positionals: Vec<Word>,
    opts: Vec<(String, Option<Word>)>,
}

impl Parsed {
    pub(super) fn value(&self, names: &[&str]) -> Option<&Word> {
        self.opts
            .iter()
            .find(|(n, _)| names.contains(&n.as_str()))
            .and_then(|(_, v)| v.as_ref())
    }

    pub(super) fn values(&self, names: &[&str]) -> Vec<&Word> {
        self.opts
            .iter()
            .filter(|(n, _)| names.contains(&n.as_str()))
            .filter_map(|(_, v)| v.as_ref())
            .collect()
    }

    pub(super) fn has(&self, name: &str) -> bool {
        self.opts.iter().any(|(n, _)| n == name)
    }
}

pub(super) fn parse_args(args: &[Word], short_valued: &str, long_valued: &[&str]) -> Parsed {
    let mut parsed = Parsed::default();
    let mut i = 0;
    let mut options_ended = false;
    while i < args.len() {
        let w = &args[i];
        i += 1;
        if options_ended || !w.is_flag() {
            parsed.positionals.push(w.clone());
            continue;
        }
        if w.is("--") {
            options_ended = true;
            continue;
        }
        if let Some(long) = w.text.strip_prefix("--") {
            match long.split_once('=') {
                Some((name, _)) => parsed
                    .opts
                    .push((name.to_string(), Some(w.slice_from(2 + name.len() + 1)))),
                None if long_valued.contains(&long) => {
                    parsed.opts.push((long.to_string(), args.get(i).cloned()));
                    i += 1;
                }
                None => parsed.opts.push((long.to_string(), None)),
            }
            continue;
        }
        for (ci, ch) in w.text[1..].char_indices() {
            if short_valued.contains(ch) {
                let (value, extra) = short_value(w, 1 + ci + ch.len_utf8(), args.get(i));
                parsed.opts.push((ch.to_string(), value));
                i += extra;
                break;
            }
            parsed.opts.push((ch.to_string(), None));
        }
    }
    parsed
}
