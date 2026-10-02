use super::analyzer::Analyzer;
use super::args::{assignment, long_value, parse_args, short_value, split_eq};
use super::lexer::Word;
use super::{Anchor, WriteRule};

impl Analyzer<'_> {
    pub(super) fn sed_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let mut in_place = false;
        let mut has_script = false;
        let mut positionals: Vec<&Word> = Vec::new();
        let mut options_ended = false;
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            if options_ended || !w.is_flag() {
                positionals.push(w);
                continue;
            }
            if w.is("--") {
                options_ended = true;
                continue;
            }
            if let Some(long) = w.text.strip_prefix("--") {
                let (name, attached) = split_eq(long);
                match name {
                    "in-place" => in_place = true,
                    "expression" | "file" => {
                        has_script = true;
                        i += usize::from(!attached);
                    }
                    "line-length" => i += usize::from(!attached),
                    _ => {}
                }
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'i' => {
                        in_place = true;
                        let bsd_suffix = w.text.len() == 2
                            && args.get(i).is_some_and(|n| n.quoted && n.text.is_empty());
                        i += usize::from(bsd_suffix);
                        break;
                    }
                    'e' | 'f' => {
                        has_script = true;
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    'l' => {
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    _ => {}
                }
            }
        }
        if !in_place {
            return;
        }
        let files = if has_script {
            &positionals[..]
        } else {
            positionals.get(1..).unwrap_or(&[])
        };
        for file in files {
            self.write_target(WriteRule::InPlaceEdit, "sed", file, cwd);
        }
    }

    pub(super) fn awk_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let parsed = parse_args(
            args,
            "ifeEvF",
            &[
                "include",
                "file",
                "source",
                "exec",
                "assign",
                "field-separator",
            ],
        );
        let in_place = parsed
            .values(&["i", "include"])
            .iter()
            .any(|v| v.text == "inplace");
        if !in_place {
            return;
        }
        let has_program = ["f", "e", "E", "file", "source", "exec"]
            .iter()
            .any(|n| parsed.has(n));
        let files = if has_program {
            &parsed.positionals[..]
        } else {
            parsed.positionals.get(1..).unwrap_or(&[])
        };
        for file in files.iter().filter(|f| assignment(f).is_none()) {
            self.write_target(WriteRule::InPlaceEdit, "awk", file, cwd);
        }
    }

    pub(super) fn perl_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let mut in_place = false;
        let mut has_code = false;
        let mut positionals: Vec<&Word> = Vec::new();
        let mut options_ended = false;
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            if options_ended || !w.is_flag() {
                positionals.push(w);
                options_ended = true;
                continue;
            }
            if w.is("--") {
                options_ended = true;
                continue;
            }
            if w.text.starts_with("--") {
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'i' => {
                        in_place = true;
                        break;
                    }
                    'e' | 'E' => {
                        has_code = true;
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    'I' | 'M' | 'm' | 'x' | 'F' | 'C' | 'D' | 'd' => break,
                    _ => {}
                }
            }
        }
        let files = if has_code {
            &positionals[..]
        } else {
            positionals.get(1..).unwrap_or(&[])
        };
        if in_place && !files.is_empty() {
            for file in files {
                self.write_target(WriteRule::InPlaceEdit, "perl", file, cwd);
            }
        } else {
            self.interpreter("perl", args, cwd);
        }
    }

    // ---- file operations ----

    pub(super) fn file_ops(&mut self, name: &str, args: &[Word], cwd: &Anchor) {
        let (short_valued, long_valued): (&str, &[&str]) = match name {
            "touch" => ("drt", &["date", "reference"]),
            "mkdir" => ("m", &["mode"]),
            "truncate" => ("sr", &["size", "reference"]),
            _ => ("", &[]),
        };
        let parsed = parse_args(args, short_valued, long_valued);
        for w in &parsed.positionals {
            self.write_target(WriteRule::FileOp, name, w, cwd);
        }
    }

    pub(super) fn chmod_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let mut operands: Vec<&Word> = Vec::new();
        let mut has_reference = false;
        let mut options_ended = false;
        let mut i = 0;
        while let Some(w) = args.get(i) {
            i += 1;
            if options_ended {
                operands.push(w);
            } else if w.is("--") {
                options_ended = true;
            } else if w.text.starts_with("--reference") {
                has_reference = true;
                i += usize::from(!w.text.contains('='));
            } else if !(w.text.starts_with("--")
                || w.is_flag()
                    && w.text[1..]
                        .chars()
                        .all(|c| matches!(c, 'R' | 'v' | 'c' | 'f')))
            {
                operands.push(w);
            }
        }
        let files = if has_reference {
            &operands[..]
        } else {
            operands.get(1..).unwrap_or(&[])
        };
        for file in files {
            self.write_target(WriteRule::FileOp, "chmod", file, cwd);
        }
    }

    pub(super) fn mv_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let parsed = parse_args(args, "tS", &["target-directory", "suffix"]);
        for w in &parsed.positionals {
            self.write_target(WriteRule::FileOp, "mv", w, cwd);
        }
        if let Some(dest) = parsed.value(&["t", "target-directory"]) {
            self.write_target(WriteRule::FileOp, "mv", dest, cwd);
        }
    }

    pub(super) fn copy_like(&mut self, name: &str, args: &[Word], cwd: &Anchor) {
        let (short_valued, long_valued): (&str, &[&str]) = if name == "install" {
            (
                "tmgoS",
                &["target-directory", "mode", "group", "owner", "suffix"],
            )
        } else {
            ("tS", &["target-directory", "suffix"])
        };
        let parsed = parse_args(args, short_valued, long_valued);
        if name == "install" && parsed.has("d") {
            for w in &parsed.positionals {
                self.write_target(WriteRule::FileOp, name, w, cwd);
            }
            return;
        }
        if let Some(dest) = parsed.value(&["t", "target-directory"]) {
            self.write_target(WriteRule::CopyDest, name, dest, cwd);
            return;
        }
        match parsed.positionals.as_slice() {
            [] => {}
            [_] if name == "ln" => self.write_place(WriteRule::CopyDest, name, cwd),
            [_] => {}
            [.., last] => self.write_target(WriteRule::CopyDest, name, last, cwd),
        }
    }

    pub(super) fn dd_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        for w in args {
            if w.text.starts_with("of=") {
                self.write_target(WriteRule::DdOutput, "dd", &w.slice_from(3), cwd);
            }
        }
    }

    pub(super) fn patch_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let parsed = parse_args(
            args,
            "dioprBVYzFDg",
            &[
                "directory",
                "input",
                "output",
                "strip",
                "reject-file",
                "backup-prefix",
                "basename-prefix",
                "suffix",
                "version-control",
                "fuzz",
                "ifdef",
                "get",
                "reject-format",
            ],
        );
        let base = match parsed.value(&["d", "directory"]) {
            Some(dir) => self.resolve(cwd, dir),
            None => cwd.clone(),
        };
        if let Some(output) = parsed.value(&["o", "output"]) {
            self.write_target(WriteRule::Patch, "patch", output, &base);
        } else if let Some(file) = parsed.positionals.first() {
            self.write_target(WriteRule::Patch, "patch", file, &base);
        } else {
            self.write_place(WriteRule::Patch, "patch", &base);
        }
    }

    pub(super) fn tar_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let mut extract = false;
        let mut dir = cwd.clone();
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            let first = i == 0;
            i += 1;
            if let Some(long) = w.text.strip_prefix("--") {
                if long.is_empty() {
                    break;
                }
                let (name, attached) = split_eq(long);
                match name {
                    "extract" | "get" => extract = true,
                    "directory" => {
                        let (value, extra) = long_value(w, name, attached, args.get(i));
                        i += extra;
                        if let Some(value) = value {
                            dir = self.resolve(&dir, &value);
                        }
                    }
                    "file"
                    | "files-from"
                    | "exclude-from"
                    | "use-compress-program"
                    | "transform"
                    | "exclude"
                    | "to-command"
                    | "suffix"
                    | "index-file" => {
                        i += usize::from(!attached);
                    }
                    _ => {}
                }
                continue;
            }
            let letters = if w.text.starts_with('-') {
                &w.text[1..]
            } else if first && w.text.chars().all(|c| c.is_ascii_alphabetic()) {
                &w.text[..]
            } else {
                continue;
            };
            let offset = w.text.len() - letters.len();
            for (ci, ch) in letters.char_indices() {
                let after = offset + ci + ch.len_utf8();
                match ch {
                    'x' => extract = true,
                    'C' => {
                        let (value, extra) = short_value(w, after, args.get(i));
                        if let Some(value) = value {
                            dir = self.resolve(&dir, &value);
                        }
                        i += extra;
                        break;
                    }
                    'f' | 'T' | 'X' | 'I' | 'H' | 'b' | 'L' | 'N' | 'V' | 'g' => {
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    _ => {}
                }
            }
        }
        if extract {
            self.write_place(WriteRule::Extract, "tar", &dir);
        }
    }

    pub(super) fn unzip_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let mut dir = cwd.clone();
        let mut listing = false;
        let mut operands = 0;
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            if !w.is_flag() {
                operands += 1;
                continue;
            }
            if w.text.starts_with("--") {
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'd' => {
                        let (value, extra) = short_value(w, after, args.get(i));
                        if let Some(value) = value {
                            dir = self.resolve(&dir, &value);
                        }
                        i += extra;
                        break;
                    }
                    'P' | 'O' | 'I' => {
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    'l' | 't' | 'p' | 'v' | 'Z' => listing = true,
                    _ => {}
                }
            }
        }
        if !listing && operands > 0 {
            self.write_place(WriteRule::Extract, "unzip", &dir);
        }
    }

    pub(super) fn curl_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let parsed = parse_args(
            args,
            "AbcCdDeEFHKmoPQrtTuUwxXyYz",
            &["output", "output-dir"],
        );
        let base = match parsed.value(&["output-dir"]) {
            Some(dir) => self.resolve(cwd, dir),
            None => cwd.clone(),
        };
        for target in parsed.values(&["o", "output"]) {
            if !target.is("-") {
                self.write_target(WriteRule::Download, "curl", target, &base);
            }
        }
    }

    pub(super) fn wget_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let parsed = parse_args(args, "OPoaeitTwQ", &["output-document", "directory-prefix"]);
        for target in parsed.values(&["O", "output-document"]) {
            if !target.is("-") {
                self.write_target(WriteRule::Download, "wget", target, cwd);
            }
        }
        for dir in parsed.values(&["P", "directory-prefix"]) {
            self.write_target(WriteRule::Download, "wget", dir, cwd);
        }
    }

    pub(super) fn rsync_cmd(&mut self, args: &[Word], cwd: &Anchor) {
        let parsed = parse_args(
            args,
            "eBfMT",
            &[
                "rsh",
                "rsync-path",
                "filter",
                "exclude",
                "exclude-from",
                "include",
                "include-from",
                "files-from",
                "partial-dir",
                "backup-dir",
                "temp-dir",
                "log-file",
                "bwlimit",
                "port",
                "timeout",
                "max-size",
                "min-size",
                "compare-dest",
                "copy-dest",
                "link-dest",
                "suffix",
                "chmod",
                "chown",
                "usermap",
                "groupmap",
                "block-size",
                "compress-level",
                "skip-compress",
                "out-format",
                "info",
                "debug",
            ],
        );
        if parsed.has("n") || parsed.has("dry-run") || parsed.has("list-only") {
            return;
        }
        if let [_, .., dest] = parsed.positionals.as_slice() {
            if !is_remote_spec(&dest.text) {
                self.write_target(WriteRule::CopyDest, "rsync", dest, cwd);
            }
        }
    }
}

fn is_remote_spec(text: &str) -> bool {
    text.split_once(':')
        .is_some_and(|(host, _)| !host.contains('/'))
}
