//! Words that point at acts hard to take back. Matched as case-insensitive
//! substrings of the whitespace-folded text, so they also hit inside longer
//! words.

const IRREVERSIBLE: [&str; 15] = [
    "force",
    "push -f",
    "reset --hard",
    "merge",
    "deploy",
    "release",
    "publish",
    "drop",
    "rm -rf",
    "delete",
    "배포",
    "삭제",
    "강제",
    "머지",
    "릴리스",
];

pub fn irreversible<'a>(texts: impl IntoIterator<Item = &'a str>) -> bool {
    texts.into_iter().any(|text| {
        let folded = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        IRREVERSIBLE.iter().any(|marker| folded.contains(marker))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_match_in_any_case_and_spacing() {
        assert!(irreversible(["Git PUSH   -f origin"]));
        assert!(irreversible(["fine", "Deleted files"]));
        assert!(!irreversible(["Which color?", "Red"]));
    }
}
