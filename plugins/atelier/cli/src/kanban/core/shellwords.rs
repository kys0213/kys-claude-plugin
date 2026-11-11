//! A shell command line read the way a POSIX shell splits it into words,
//! without expanding anything: what would be expanded or would join commands
//! is reported instead.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellIssue {
    /// An operator, a redirection, a subshell, a line break or an unclosed
    /// quote: the line is not one simple command.
    Compound,
    /// A parameter, command, tilde, glob or brace expansion, or a leading
    /// variable assignment.
    Expansion,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellLine {
    /// The words of each simple command, split at the operators.
    pub commands: Vec<Vec<String>>,
    pub issues: Vec<ShellIssue>,
}

impl ShellLine {
    pub fn is_simple(&self) -> bool {
        self.issues.is_empty() && self.commands.len() == 1
    }

    fn flag(&mut self, issue: ShellIssue) {
        if !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
    }
}

#[derive(Default)]
struct Word {
    text: String,
    started: bool,
    /// Char index of the first quoted or escaped char.
    quoted_from: Option<usize>,
    brace_open: bool,
    last_unquoted: Option<char>,
}

impl Word {
    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn push_quoted(&mut self, c: char) {
        self.started = true;
        self.quoted_from.get_or_insert(self.len());
        self.text.push(c);
        self.last_unquoted = None;
    }

    fn open_quote(&mut self) {
        self.started = true;
        self.quoted_from.get_or_insert(self.len());
        self.last_unquoted = None;
    }

    /// Whether this unquoted char would make the shell expand the word.
    fn expands(&self, c: char) -> bool {
        match c {
            '*' | '?' | '[' => true,
            '~' => self.len() == 0 || matches!(self.last_unquoted, Some('=' | ':')),
            ',' => self.brace_open,
            '.' => self.brace_open && self.last_unquoted == Some('.'),
            _ => false,
        }
    }

    fn push_unquoted(&mut self, c: char) {
        self.started = true;
        if c == '{' {
            self.brace_open = true;
        }
        self.text.push(c);
        self.last_unquoted = Some(c);
    }

    /// `NAME=…` with the name and `=` unquoted.
    fn is_assignment(&self) -> bool {
        let Some(eq) = self.text.find('=') else {
            return false;
        };
        let name = &self.text[..eq];
        let eq_chars = name.chars().count();
        let unquoted = self.quoted_from.is_none_or(|q| q > eq_chars);
        let mut chars = name.chars();
        let valid = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        unquoted && valid
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

struct Splitter {
    line: ShellLine,
    command: Vec<String>,
    word: Word,
}

impl Splitter {
    fn end_word(&mut self) {
        let word = std::mem::take(&mut self.word);
        if !word.started {
            return;
        }
        if self.command.is_empty() && word.is_assignment() {
            self.line.flag(ShellIssue::Expansion);
        }
        self.command.push(word.text);
    }

    fn end_command(&mut self) {
        self.end_word();
        let command = std::mem::take(&mut self.command);
        if !command.is_empty() {
            self.line.commands.push(command);
        }
    }

    fn operator(&mut self) {
        self.line.flag(ShellIssue::Compound);
        self.end_command();
    }
}

pub fn split(command: &str) -> ShellLine {
    let mut s = Splitter {
        line: ShellLine::default(),
        command: Vec::new(),
        word: Word::default(),
    };
    if command.contains(['$', '`']) {
        s.line.flag(ShellIssue::Expansion);
    }
    let mut quote = Quote::None;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Quote::Single => match c {
                '\'' => quote = Quote::None,
                c => s.word.push_quoted(c),
            },
            Quote::Double => match c {
                '"' => quote = Quote::None,
                '\\' => match chars.peek().copied() {
                    Some('\n') => {
                        chars.next();
                    }
                    Some(next @ ('$' | '`' | '"' | '\\')) => {
                        chars.next();
                        s.word.push_quoted(next);
                    }
                    _ => s.word.push_quoted('\\'),
                },
                c => s.word.push_quoted(c),
            },
            Quote::None => match c {
                ' ' | '\t' => s.end_word(),
                '\n' | '\r' | ';' | '&' | '|' | '<' | '>' | '(' | ')' => s.operator(),
                '\'' => {
                    s.word.open_quote();
                    quote = Quote::Single;
                }
                '"' => {
                    s.word.open_quote();
                    quote = Quote::Double;
                }
                '\\' => match chars.next() {
                    Some('\n') | None => {}
                    Some(next) => s.word.push_quoted(next),
                },
                c => {
                    if s.word.expands(c) {
                        s.line.flag(ShellIssue::Expansion);
                    }
                    s.word.push_unquoted(c);
                }
            },
        }
    }
    if quote != Quote::None {
        s.line.flag(ShellIssue::Compound);
    }
    s.end_command();
    s.line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(command: &str) -> Vec<Vec<String>> {
        split(command).commands
    }

    #[test]
    fn quotes_and_escapes_join_into_one_word() {
        assert_eq!(
            words(r#"grep -e 'a b' "c\"d" e\ f"#),
            vec![vec!["grep", "-e", "a b", "c\"d", "e f"]]
        );
        assert!(split("grep 'a|b' x").is_simple());
    }

    #[test]
    fn operators_split_commands_and_flag_the_line() {
        let line = split("cat a && rm b");
        assert_eq!(line.commands, vec![vec!["cat", "a"], vec!["rm", "b"]]);
        assert_eq!(line.issues, vec![ShellIssue::Compound]);
        assert_eq!(split("ls 'a").issues, vec![ShellIssue::Compound]);
    }

    #[test]
    fn expansions_are_flagged_wherever_the_shell_would_expand() {
        for command in [
            "cat '$x'",
            "cat ~/a",
            "cat a=~/b",
            "ls *.rs",
            "cat {a,b}",
            "cat {1..3}",
            "X=1 ls",
        ] {
            assert_eq!(
                split(command).issues,
                vec![ShellIssue::Expansion],
                "{command}"
            );
        }
    }

    #[test]
    fn literal_tildes_braces_and_quoted_assignments_are_words() {
        for command in ["git log HEAD~3", "git show HEAD@{1}", "'X=1' ls", "ls '*'"] {
            assert!(split(command).issues.is_empty(), "{command}");
        }
    }
}
