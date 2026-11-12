//! Values that look secret, hidden before the ledger keeps a text.

use regex::{Captures, Regex};
use std::sync::LazyLock;

const MASK: &str = "[masked]";

/// `name: value` with a name that ends in a secret-sounding word, so
/// `DB_PASSWORD=` and `"client_secret":` count: the name and separator stay,
/// and a quoted value keeps its quotes.
static NAMED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"(?i)(api[_-]?key|token|secret|password|passwd|authorization)(["']?\s*[:=]\s*)"#,
        r#"(?:bearer\s+)?("[^"]*"|'[^']*'|\S+)"#,
    ))
    .unwrap()
});
static BEARER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(bearer\s+)\S+").unwrap());
/// The password of a URL's `user:password@`.
static USERINFO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(://[^/\s:@]+:)[^/\s@]+@").unwrap());
/// mysql's `-p<password>`, glued to the flag, on a line that runs a mysql or
/// mariadb client; `-p` alone prompts and `-P` is the port.
static MYSQL_P: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\b(?:mysql|mariadb)[\w-]*\b[^\n|;&]*?\s(?-i:-p))\S+").unwrap()
});
static SHAPED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"sk-[A-Za-z0-9_-]{16,}",
        r"|gh[pousr]_[A-Za-z0-9]{20,}",
        r"|AKIA[0-9A-Z]{16}",
        r"|xox[abpr]-\S+",
    ))
    .unwrap()
});
static RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/=_-]{40,}").unwrap());

fn random_looking(run: &str) -> bool {
    let has = |class: fn(&char) -> bool| run.chars().any(|c| class(&c));
    let hex = run.chars().all(|c| c.is_ascii_hexdigit())
        && has(char::is_ascii_digit)
        && has(char::is_ascii_alphabetic);
    let mixed =
        has(char::is_ascii_uppercase) && has(char::is_ascii_lowercase) && has(char::is_ascii_digit);
    hex || mixed
}

fn masked_value(value: &str) -> String {
    match value.chars().next() {
        Some(quote @ ('"' | '\'')) => format!("{quote}{MASK}{quote}"),
        _ => MASK.to_string(),
    }
}

pub fn mask(text: &str) -> String {
    let named = NAMED.replace_all(text, |c: &Captures| {
        format!("{}{}{}", &c[1], &c[2], masked_value(&c[3]))
    });
    let userinfo = USERINFO.replace_all(&named, |c: &Captures| format!("{}{MASK}@", &c[1]));
    let mysql = MYSQL_P.replace_all(&userinfo, |c: &Captures| format!("{}{MASK}", &c[1]));
    let bearer = BEARER.replace_all(&mysql, |c: &Captures| format!("{}{MASK}", &c[1]));
    let shaped = SHAPED.replace_all(&bearer, MASK);
    RUN.replace_all(&shaped, |c: &Captures| {
        if random_looking(&c[0]) {
            MASK.to_string()
        } else {
            c[0].to_string()
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_masked_value_is_not_masked_again() {
        assert_eq!(mask("token=[masked]"), "token=[masked]");
        assert_eq!(mask(&mask("Bearer abc")), "Bearer [masked]");
    }

    #[test]
    fn a_run_of_one_case_stays() {
        assert_eq!(mask(&"a".repeat(50)), "a".repeat(50));
        assert_eq!(mask(&"Z1".repeat(30)), "Z1".repeat(30));
    }
}
