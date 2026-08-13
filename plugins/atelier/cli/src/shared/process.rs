//! The process edge: stdin and the working directory. Both subsystems run as
//! Claude Code hooks, so both read a payload off stdin and both have to decide
//! which directory their git reads are anchored to.

/// Reads stdin to a string (empty on read failure). Parsing the payload is
/// command logic (`HookPayload::parse` / `SessionPayload::parse`); only the I/O
/// lives here (#778).
pub fn read_stdin_raw() -> String {
    use std::io::Read as _;
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    buf
}

/// Resolves the directory a command's git reads are anchored to: the explicit
/// value when the caller has one, otherwise the process cwd.
///
/// The last fallback is `"."`, not the empty string: a hook's cwd is readable
/// in every situation we know of, but if it ever is not, `"."` still names the
/// same directory, while `""` makes `git -C` and `settings.json` paths resolve
/// somewhere else entirely (`"/.claude/settings.json"`).
pub fn default_project_dir(explicit: Option<String>) -> String {
    explicit.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    })
}
