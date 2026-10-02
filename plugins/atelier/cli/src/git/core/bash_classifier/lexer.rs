use super::LexError;

#[derive(Debug, Clone, Default)]
pub(super) struct Word {
    pub(super) text: String,
    pub(super) quoted: bool,
    /// Byte index in `text` of the first expansion or glob character.
    pub(super) dyn_at: Option<usize>,
    pub(super) tilde: bool,
    /// Bodies of the `$(…)` and backtick substitutions inside the word.
    pub(super) subs: Vec<String>,
}

impl Word {
    pub(super) fn literal(text: &str) -> Self {
        Word {
            text: text.to_string(),
            ..Word::default()
        }
    }

    pub(super) fn stdin_placeholder() -> Self {
        Word {
            text: "<stdin>".to_string(),
            dyn_at: Some(0),
            ..Word::default()
        }
    }

    fn push_lit(&mut self, c: char) {
        self.text.push(c);
    }

    fn push_dyn(&mut self, c: char) {
        if self.dyn_at.is_none() {
            self.dyn_at = Some(self.text.len());
        }
        self.text.push(c);
    }

    pub(super) fn is(&self, s: &str) -> bool {
        self.text == s
    }

    pub(super) fn is_flag(&self) -> bool {
        self.text.len() > 1 && self.text.starts_with('-')
    }

    pub(super) fn slice_from(&self, offset: usize) -> Word {
        Word {
            text: self.text[offset..].to_string(),
            quoted: self.quoted,
            dyn_at: self.dyn_at.and_then(|i| i.checked_sub(offset)),
            tilde: false,
            subs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sep {
    Semi,
    And,
    Or,
    Pipe,
    Amp,
    Newline,
    LParen,
    RParen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RedirOp {
    Out,
    In,
    HereString,
    HereDoc,
    Dup,
}

impl RedirOp {
    pub(super) fn takes_operand(self) -> bool {
        matches!(self, RedirOp::Out | RedirOp::In | RedirOp::HereString)
    }
}

#[derive(Debug, Clone)]
pub(super) enum Tok {
    Word(Word),
    Sep(Sep),
    Redir(RedirOp),
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    toks: Vec<Tok>,
    heredocs: Vec<(String, bool)>,
}

pub(super) fn lex(src: &str) -> Result<Vec<Tok>, LexError> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        toks: Vec::new(),
        heredocs: Vec::new(),
    };
    while let Some(c) = lx.peek() {
        match c {
            ' ' | '\t' | '\r' => lx.pos += 1,
            '\\' if lx.peek_at(1) == Some('\n') => lx.pos += 2,
            '\n' => {
                lx.pos += 1;
                lx.toks.push(Tok::Sep(Sep::Newline));
                lx.skip_heredoc_bodies();
            }
            '#' => {
                while lx.peek().is_some_and(|c| c != '\n') {
                    lx.pos += 1;
                }
            }
            ';' => {
                lx.pos += 1;
                if lx.peek() == Some(';') {
                    lx.pos += 1;
                }
                lx.toks.push(Tok::Sep(Sep::Semi));
            }
            '&' => lx.ampersand(),
            '|' => lx.pipe(),
            '(' => {
                lx.pos += 1;
                lx.toks.push(Tok::Sep(Sep::LParen));
            }
            ')' => {
                lx.pos += 1;
                lx.toks.push(Tok::Sep(Sep::RParen));
            }
            '<' | '>' => lx.redirect()?,
            _ => {
                let word = lx.read_word()?;
                lx.push_word(word);
            }
        }
    }
    Ok(lx.toks)
}

fn is_word_end(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
    )
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn at(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(i, c)| self.peek_at(i) == Some(c))
    }

    fn push_word(&mut self, word: Word) {
        if word.text.is_empty() && !word.quoted {
            return;
        }
        let is_fd_prefix = !word.quoted
            && word.text.chars().all(|c| c.is_ascii_digit())
            && matches!(self.peek(), Some('<' | '>'));
        if !is_fd_prefix {
            self.toks.push(Tok::Word(word));
        }
    }

    fn ampersand(&mut self) {
        match self.peek_at(1) {
            Some('>') => {
                self.pos += 2;
                if self.peek() == Some('>') {
                    self.pos += 1;
                }
                self.toks.push(Tok::Redir(RedirOp::Out));
            }
            Some('&') => {
                self.pos += 2;
                self.toks.push(Tok::Sep(Sep::And));
            }
            _ => {
                self.pos += 1;
                self.toks.push(Tok::Sep(Sep::Amp));
            }
        }
    }

    fn pipe(&mut self) {
        match self.peek_at(1) {
            Some('|') => {
                self.pos += 2;
                self.toks.push(Tok::Sep(Sep::Or));
            }
            Some('&') => {
                self.pos += 2;
                self.toks.push(Tok::Sep(Sep::Pipe));
            }
            _ => {
                self.pos += 1;
                self.toks.push(Tok::Sep(Sep::Pipe));
            }
        }
    }

    /// Consumes `&` plus an fd number or `-` when present. Returns false and
    /// leaves the position after the `&` when the target is a file name.
    fn dup_target(&mut self) -> bool {
        self.pos += 1;
        if !matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
            return false;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
            self.pos += 1;
        }
        true
    }

    fn redirect(&mut self) -> Result<(), LexError> {
        let c = self.chars[self.pos];
        if self.peek_at(1) == Some('(') {
            let mut word = Word::default();
            word.push_dyn(c);
            self.pos += 1;
            self.scan_parens(&mut word)?;
            self.toks.push(Tok::Word(word));
            return Ok(());
        }
        if c == '>' {
            self.pos += 1;
            match self.peek() {
                Some('>' | '|') => self.pos += 1,
                Some('&') => {
                    if self.dup_target() {
                        self.toks.push(Tok::Redir(RedirOp::Dup));
                        return Ok(());
                    }
                }
                _ => {}
            }
            self.toks.push(Tok::Redir(RedirOp::Out));
        } else if self.at("<<<") {
            self.pos += 3;
            self.toks.push(Tok::Redir(RedirOp::HereString));
        } else if self.at("<<") {
            self.pos += 2;
            let strip_tabs = self.peek() == Some('-');
            if strip_tabs {
                self.pos += 1;
            }
            self.toks.push(Tok::Redir(RedirOp::HereDoc));
            self.read_heredoc_delimiter(strip_tabs)?;
        } else if self.at("<&") {
            self.pos += 1;
            if self.dup_target() {
                self.toks.push(Tok::Redir(RedirOp::Dup));
            } else {
                self.toks.push(Tok::Redir(RedirOp::In));
            }
        } else if self.at("<>") {
            self.pos += 2;
            self.toks.push(Tok::Redir(RedirOp::Out));
        } else {
            self.pos += 1;
            self.toks.push(Tok::Redir(RedirOp::In));
        }
        Ok(())
    }

    fn read_heredoc_delimiter(&mut self, strip_tabs: bool) -> Result<(), LexError> {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.pos += 1;
        }
        if self.peek().is_none_or(is_word_end) {
            return Ok(());
        }
        let word = self.read_word()?;
        self.heredocs.push((word.text, strip_tabs));
        Ok(())
    }

    fn skip_heredoc_bodies(&mut self) {
        for (delimiter, strip_tabs) in std::mem::take(&mut self.heredocs) {
            while self.pos < self.chars.len() {
                let start = self.pos;
                while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                    self.pos += 1;
                }
                let line: String = self.chars[start..self.pos].iter().collect();
                if self.pos < self.chars.len() {
                    self.pos += 1;
                }
                let line = if strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                if line.trim_end_matches('\r') == delimiter {
                    break;
                }
            }
        }
    }

    fn read_word(&mut self) -> Result<Word, LexError> {
        let mut w = Word::default();
        while let Some(c) = self.peek() {
            match c {
                c if is_word_end(c) => break,
                '\\' => {
                    self.pos += 1;
                    match self.next() {
                        None | Some('\n') => {}
                        Some(escaped) => {
                            w.push_lit(escaped);
                            w.quoted = true;
                        }
                    }
                }
                '\'' => {
                    self.pos += 1;
                    w.quoted = true;
                    loop {
                        match self.next() {
                            None => return Err(LexError::UnterminatedQuote),
                            Some('\'') => break,
                            Some(ch) => w.push_lit(ch),
                        }
                    }
                }
                '"' => {
                    self.pos += 1;
                    w.quoted = true;
                    self.read_double_quoted(&mut w)?;
                }
                '$' => self.read_dollar(&mut w, false)?,
                '`' => self.read_backtick(&mut w)?,
                '*' | '?' | '[' => {
                    w.push_dyn(c);
                    self.pos += 1;
                }
                '{' => {
                    if self.brace_expands() {
                        w.push_dyn(c);
                    } else {
                        w.push_lit(c);
                    }
                    self.pos += 1;
                }
                '~' if w.text.is_empty() && !w.quoted => {
                    w.tilde = true;
                    w.push_lit(c);
                    self.pos += 1;
                }
                _ => {
                    w.push_lit(c);
                    self.pos += 1;
                }
            }
        }
        Ok(w)
    }

    fn brace_expands(&self) -> bool {
        let mut saw_separator = false;
        let mut j = self.pos + 1;
        while let Some(&c) = self.chars.get(j) {
            if c == '}' {
                return saw_separator;
            }
            if c.is_whitespace() || is_word_end(c) {
                return false;
            }
            if c == ',' || (c == '.' && self.chars.get(j + 1) == Some(&'.')) {
                saw_separator = true;
            }
            j += 1;
        }
        false
    }

    fn read_double_quoted(&mut self, w: &mut Word) -> Result<(), LexError> {
        loop {
            match self.peek() {
                None => return Err(LexError::UnterminatedQuote),
                Some('"') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.next() {
                        None => return Err(LexError::UnterminatedQuote),
                        Some('\n') => {}
                        Some(n @ ('$' | '`' | '"' | '\\')) => w.push_lit(n),
                        Some(n) => {
                            w.push_lit('\\');
                            w.push_lit(n);
                        }
                    }
                }
                Some('$') => self.read_dollar(w, true)?,
                Some('`') => self.read_backtick(w)?,
                Some(c) => {
                    w.push_lit(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn read_dollar(&mut self, w: &mut Word, in_double_quotes: bool) -> Result<(), LexError> {
        match self.peek_at(1) {
            Some('(') => {
                let arithmetic = self.peek_at(2) == Some('(');
                w.push_dyn('$');
                self.pos += 1;
                let open = w.text.len();
                self.scan_parens(w)?;
                if !arithmetic {
                    let body = w.text[open + 1..w.text.len() - 1].to_string();
                    w.subs.push(body);
                }
                Ok(())
            }
            Some('{') => {
                w.push_dyn('$');
                self.pos += 1;
                loop {
                    match self.next() {
                        None => return Err(LexError::UnterminatedSubstitution),
                        Some(c) => {
                            w.text.push(c);
                            if c == '}' {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            Some('\'') if !in_double_quotes => {
                self.pos += 2;
                w.quoted = true;
                self.read_ansi_c(w)
            }
            Some(n) if n.is_alphanumeric() || "_?$@*#!-".contains(n) => {
                w.push_dyn('$');
                self.pos += 1;
                Ok(())
            }
            _ => {
                w.push_lit('$');
                self.pos += 1;
                Ok(())
            }
        }
    }

    /// Copies a balanced `( … )` group verbatim, starting at the `(`.
    fn scan_parens(&mut self, w: &mut Word) -> Result<(), LexError> {
        let mut depth = 0usize;
        let mut pending_heredocs: Vec<(String, bool)> = Vec::new();
        loop {
            let Some(c) = self.next() else {
                return Err(LexError::UnterminatedSubstitution);
            };
            w.text.push(c);
            match c {
                '<' if self.peek() == Some('<') && self.peek_at(1) != Some('<') => {
                    self.copy_heredoc_opener(w, &mut pending_heredocs);
                }
                '\n' if !pending_heredocs.is_empty() => {
                    for (delimiter, strip_tabs) in std::mem::take(&mut pending_heredocs) {
                        self.copy_heredoc_body(w, &delimiter, strip_tabs);
                    }
                }
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                '\\' => {
                    if let Some(n) = self.next() {
                        w.text.push(n);
                    }
                }
                '\'' | '"' => loop {
                    match self.next() {
                        None => return Err(LexError::UnterminatedSubstitution),
                        Some(q) => {
                            w.text.push(q);
                            if q == '\\' && c == '"' {
                                if let Some(n) = self.next() {
                                    w.text.push(n);
                                }
                            } else if q == c {
                                break;
                            }
                        }
                    }
                },
                _ => {}
            }
        }
    }

    /// Copies the rest of a `<<[-]DELIM` opener (the first `<` is already in
    /// `w`) and records its delimiter so the body can be copied unscanned.
    fn copy_heredoc_opener(&mut self, w: &mut Word, pending: &mut Vec<(String, bool)>) {
        w.text.push('<');
        self.pos += 1;
        let strip_tabs = self.peek() == Some('-');
        if strip_tabs {
            w.text.push('-');
            self.pos += 1;
        }
        while let Some(c @ (' ' | '\t')) = self.peek() {
            w.text.push(c);
            self.pos += 1;
        }
        let mut delimiter = String::new();
        while let Some(c) = self.peek().filter(|c| !is_word_end(*c)) {
            w.text.push(c);
            self.pos += 1;
            if !matches!(c, '\'' | '"' | '\\') {
                delimiter.push(c);
            }
        }
        if !delimiter.is_empty() {
            pending.push((delimiter, strip_tabs));
        }
    }

    /// Copies heredoc body lines verbatim through the delimiter line, so quotes
    /// and parentheses inside the body never reach the paren scanner.
    fn copy_heredoc_body(&mut self, w: &mut Word, delimiter: &str, strip_tabs: bool) {
        while self.pos < self.chars.len() {
            let start = self.pos;
            while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                self.pos += 1;
            }
            let line: String = self.chars[start..self.pos].iter().collect();
            w.text.push_str(&line);
            if self.pos < self.chars.len() {
                self.pos += 1;
                w.text.push('\n');
            }
            let line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line.as_str()
            };
            if line.trim_end_matches('\r') == delimiter {
                break;
            }
        }
    }

    fn read_backtick(&mut self, w: &mut Word) -> Result<(), LexError> {
        w.push_dyn('`');
        self.pos += 1;
        let open = w.text.len();
        loop {
            let Some(c) = self.next() else {
                return Err(LexError::UnterminatedSubstitution);
            };
            w.text.push(c);
            if c == '\\' {
                if let Some(n) = self.next() {
                    w.text.push(n);
                }
            } else if c == '`' {
                w.subs
                    .push(unescape_backtick(&w.text[open..w.text.len() - 1]));
                return Ok(());
            }
        }
    }

    fn read_ansi_c(&mut self, w: &mut Word) -> Result<(), LexError> {
        loop {
            match self.next() {
                None => return Err(LexError::UnterminatedQuote),
                Some('\'') => return Ok(()),
                Some('\\') => {
                    let Some(e) = self.next() else {
                        return Err(LexError::UnterminatedQuote);
                    };
                    match e {
                        'n' => w.push_lit('\n'),
                        't' => w.push_lit('\t'),
                        'r' => w.push_lit('\r'),
                        'a' => w.push_lit('\x07'),
                        'b' => w.push_lit('\x08'),
                        'e' | 'E' => w.push_lit('\x1b'),
                        'f' => w.push_lit('\x0c'),
                        'v' => w.push_lit('\x0b'),
                        'x' => {
                            let value = self.read_digits(2, 16);
                            match value.and_then(char::from_u32) {
                                Some(ch) => w.push_lit(ch),
                                None => w.push_lit('x'),
                            }
                        }
                        '0'..='7' => {
                            self.pos -= 1;
                            match self.read_digits(3, 8).and_then(char::from_u32) {
                                Some(ch) => w.push_lit(ch),
                                None => w.push_lit(e),
                            }
                        }
                        other => w.push_lit(other),
                    }
                }
                Some(ch) => w.push_lit(ch),
            }
        }
    }

    fn read_digits(&mut self, max: usize, radix: u32) -> Option<u32> {
        let mut value = 0u32;
        let mut count = 0;
        while count < max {
            let Some(d) = self.peek().and_then(|c| c.to_digit(radix)) else {
                break;
            };
            value = value * radix + d;
            self.pos += 1;
            count += 1;
        }
        (count > 0).then_some(value)
    }
}

fn unescape_backtick(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\\', Some('`' | '\\' | '$')) => {
                out.extend(chars.next());
            }
            _ => out.push(c),
        }
    }
    out
}
