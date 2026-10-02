//! Black-box tests for the pure Bash classifier: command text in, bypass
//! flags and anchored hits out.

use atelier::git::core::bash_classifier::{
    classify, Anchor, BashAnalysis, BranchSwitch, BypassRule, ClassifyInput, HitKind, OpaqueCause,
    Wrapper, WriteRule,
};
use std::path::PathBuf;

const CWD: &str = "/work/proj";
const HOME: &str = "/home/u";

type Pair = (HitKind, Anchor);

fn input(command: &str) -> ClassifyInput {
    ClassifyInput {
        command: command.to_string(),
        cwd: PathBuf::from(CWD),
        home: Some(PathBuf::from(HOME)),
    }
}

fn analyze(command: &str) -> BashAnalysis {
    classify(&input(command))
}

fn pairs(command: &str) -> Vec<Pair> {
    analyze(command)
        .hits
        .into_iter()
        .map(|h| (h.kind, h.anchor))
        .collect()
}

fn path(p: &str) -> Anchor {
    Anchor::Path(PathBuf::from(p))
}

fn unresolved(prefix: Option<&str>, raw: &str) -> Anchor {
    Anchor::Unresolved {
        literal_prefix: prefix.map(PathBuf::from),
        raw: raw.to_string(),
    }
}

fn write(rule: WriteRule, anchor: Anchor) -> Pair {
    (HitKind::Write(rule), anchor)
}

fn opaque(cause: OpaqueCause) -> Pair {
    (HitKind::Opaque(cause), path(CWD))
}

fn interpreter(name: &str) -> Pair {
    opaque(OpaqueCause::Interpreter(name.to_string()))
}

fn wrapper(w: Wrapper) -> Pair {
    opaque(OpaqueCause::Wrapper(w))
}

fn assert_hits(command: &str, expected: Vec<Pair>) {
    assert_eq!(pairs(command), expected, "command: {command:?}");
}

fn assert_clean(command: &str) {
    let a = analyze(command);
    assert!(
        a.hits.is_empty() && a.bypass.is_empty(),
        "expected nothing for {command:?}, got {a:?}"
    );
}

fn assert_bypass(command: &str, rule: BypassRule) {
    let a = analyze(command);
    assert!(
        a.bypass.iter().any(|b| b.rule == rule),
        "expected {rule:?} for {command:?}, got {a:?}"
    );
}

fn assert_no_bypass(command: &str) {
    let a = analyze(command);
    assert!(
        a.bypass.is_empty(),
        "expected no bypass for {command:?}, got {a:?}"
    );
}

fn assert_single_write(command: &str, rule: WriteRule, target: &str) {
    assert_hits(command, vec![write(rule, path(target))]);
}

// ---- hook bypass: --no-verify ----------------------------------------------

#[test]
fn no_verify_flag_is_bypass_for_hook_running_subcommands() {
    for cmd in [
        "git commit --no-verify -m x",
        "git commit -m x --no-verify",
        "git push --no-verify",
        "git push origin main --no-verify",
        "git merge --no-verify feat",
        "git am --no-verify p.patch",
        "git rebase --no-verify main",
        "git pull --no-verify",
        "git pull -s ours --no-verify origin main",
        "git commit --no-veri -m x",
        "git commit --no-verify=1 -m x",
        "git -C /tmp/x commit --no-verify",
        "git -c user.name=a commit --no-verify",
    ] {
        assert_bypass(cmd, BypassRule::NoVerifyFlag);
    }
}

#[test]
fn no_verify_text_that_is_not_a_flag_passes() {
    for cmd in [
        r#"git commit -m "fix --no-verify handling""#,
        "git commit -m '--no-verify'",
        "git commit -m --no-verify",
        "git commit -F --no-verify",
        "git commit -m x -- --no-verify",
        r#"echo "git commit --no-verify""#,
        "echo --no-verify",
        "git commit --no-ver -m x",
        "git log --no-verify",
        "git pull -n",
        "git status",
    ] {
        assert_no_bypass(cmd);
    }
}

// ---- hook bypass: commit -n ------------------------------------------------

#[test]
fn commit_short_n_is_bypass() {
    for cmd in [
        "git commit -n -m x",
        "git commit -an -m x",
        "git commit -nm x",
        "git commit -m x -n",
        "git commit -aS -n",
        "git commit -m x -a -n",
    ] {
        assert_bypass(cmd, BypassRule::CommitShortNoVerify);
    }
}

#[test]
fn n_that_is_a_value_or_another_subcommands_flag_passes() {
    for cmd in [
        "git commit -mn",
        "git commit -m -n",
        "git commit -F -n",
        "git commit -Sn",
        "git commit -uno -m x",
        "git commit -m x",
        "git log -n 5",
        "git push -n",
        "git push --dry-run",
        "git diff -n",
    ] {
        assert_no_bypass(cmd);
    }
}

// ---- hook bypass: hooksPath and env ----------------------------------------

#[test]
fn hooks_path_config_is_bypass() {
    for cmd in [
        "git -c core.hooksPath=/dev/null commit -m x",
        "git -c core.hookspath=/x commit -m x",
        "git -c CORE.HOOKSPATH=/x push",
        "git --config-env=core.hooksPath=HP commit -m x",
        "git --config-env core.hooksPath=HP commit -m x",
        "git config core.hooksPath /dev/null",
        "git config --global core.hooksPath /x",
        "git config --local --unset core.hooksPath",
        "git config --file .git/config core.hookspath x",
        "git config set core.hooksPath /x",
        "git config unset core.hooksPath",
    ] {
        assert_bypass(cmd, BypassRule::HooksPathConfig);
    }
}

#[test]
fn hooks_path_env_is_bypass() {
    for cmd in [
        r#"GIT_CONFIG_PARAMETERS="'core.hookspath'='/x'" git commit -m x"#,
        "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit -m x",
        "env GIT_CONFIG_KEY_0=core.hooksPath git commit -m x",
    ] {
        assert_bypass(cmd, BypassRule::HooksPathEnv);
    }
}

#[test]
fn hook_skip_env_is_bypass() {
    for cmd in [
        "HUSKY=0 git commit -m x",
        "SKIP=lint git commit -m x",
        "env HUSKY=0 git commit -m x",
        "env SKIP=lint,fmt git push",
        "sudo -u bob env SKIP=x git push",
        "bash -c 'HUSKY=0 git commit -m x'",
        "HUSKY=0 bash -c 'git commit -m x'",
    ] {
        assert_bypass(cmd, BypassRule::HookSkipEnv);
    }
}

#[test]
fn harmless_config_and_env_pass() {
    for cmd in [
        "git config --get core.hooksPath",
        "git config core.hooksPath",
        "git config --list",
        "git config user.name x",
        "git config --global user.email a@b",
        "git config get core.hooksPath",
        "git -c user.name=x commit -m y",
        "env FOO=1 git commit -m x",
        "HUSKY=1 git commit -m x",
        "SKIP= git commit -m x",
        "HUSKY=0 npm test",
        "SKIP=1 make build",
        "HUSKY=0 git status",
    ] {
        assert_no_bypass(cmd);
    }
}

#[test]
fn bypass_and_commit_are_both_reported() {
    let a = analyze("git commit --no-verify -m x");
    assert_eq!(a.bypass.len(), 1);
    assert_eq!(a.bypass[0].rule, BypassRule::NoVerifyFlag);
    assert_eq!(a.bypass[0].token, "--no-verify");
    assert_hits(
        "git commit --no-verify -m x",
        vec![(HitKind::Commit, path(CWD))],
    );
}

// ---- commit anchors --------------------------------------------------------

#[test]
fn commit_anchors_at_cwd_or_dash_c_dir() {
    assert_hits("git commit -m x", vec![(HitKind::Commit, path(CWD))]);
    assert_hits(
        "git -C /other/repo commit -m x",
        vec![(HitKind::Commit, path("/other/repo"))],
    );
    assert_hits(
        "git -C sub commit -m x",
        vec![(HitKind::Commit, path("/work/proj/sub"))],
    );
    assert_hits(
        "git -C a -C b commit -m x",
        vec![(HitKind::Commit, path("/work/proj/a/b"))],
    );
    assert_hits(
        "git -c user.name=a --no-pager commit -m x",
        vec![(HitKind::Commit, path(CWD))],
    );
    assert_hits(
        "/usr/bin/git commit -m x",
        vec![(HitKind::Commit, path(CWD))],
    );
    assert_hits(
        "git status && git commit -m x",
        vec![(HitKind::Commit, path(CWD))],
    );
    assert_hits(
        "cd /other && git commit -m x",
        vec![(HitKind::Commit, path("/other"))],
    );
    assert_hits(
        r#"bash -c "git commit -m x""#,
        vec![(HitKind::Commit, path(CWD))],
    );
}

#[test]
fn text_mentioning_commit_is_not_a_commit() {
    for cmd in [
        r#"gh issue create --body "git commit -m x""#,
        "echo 'git commit'",
        "git commit-tree abc",
        "git log --oneline",
        "git status",
        "git push origin main",
    ] {
        assert!(
            !pairs(cmd).iter().any(|(k, _)| *k == HitKind::Commit),
            "{cmd:?}"
        );
    }
}

// ---- writes: redirects -----------------------------------------------------

#[test]
fn redirect_targets_are_writes() {
    for (cmd, target) in [
        ("echo hi > f.txt", "/work/proj/f.txt"),
        ("echo hi >> /abs/f.txt", "/abs/f.txt"),
        ("echo hi >| f.txt", "/work/proj/f.txt"),
        ("echo hi &> out.log", "/work/proj/out.log"),
        ("echo hi &>> out.log", "/work/proj/out.log"),
        ("make 2> err.log", "/work/proj/err.log"),
        ("echo hi 1>out.txt", "/work/proj/out.txt"),
        ("> trunc.txt", "/work/proj/trunc.txt"),
        ("echo hi > ~/note.txt", "/home/u/note.txt"),
        ("echo hi > sub/../f.txt", "/work/proj/f.txt"),
        ("echo hi > 'a b.txt'", "/work/proj/a b.txt"),
        ("cat <<EOF > out.txt\nbody\nEOF", "/work/proj/out.txt"),
    ] {
        assert_single_write(cmd, WriteRule::Redirect, target);
    }
}

#[test]
fn non_file_redirects_pass() {
    for cmd in [
        "echo hi > /dev/null",
        "echo hi >> /dev/null",
        "echo hi > /dev/stderr",
        "echo hi > /dev/stdout",
        "ls 2>/dev/null",
        "make 2>&1",
        "make >&2",
        "make 2>&1 | cat",
        "cat < in.txt",
        "cat <<< 'text'",
        "cat <<EOF\nbody\nEOF",
        "echo a > /dev/null 2>&1",
    ] {
        assert_clean(cmd);
    }
}

#[test]
fn redirect_to_dynamic_target_is_unresolved() {
    assert_hits(
        r#"echo a > "$TMPDIR/x""#,
        vec![write(WriteRule::Redirect, unresolved(None, "$TMPDIR/x"))],
    );
}

// ---- writes: in-place editors ----------------------------------------------

#[test]
fn in_place_editors_write_their_file_arguments() {
    for (cmd, targets) in [
        ("sed -i 's/a/b/' f", vec!["/work/proj/f"]),
        ("sed -i.bak 's/a/b/' f", vec!["/work/proj/f"]),
        ("sed --in-place 's/a/b/' f", vec!["/work/proj/f"]),
        ("sed --in-place=.bak 's/a/b/' f", vec!["/work/proj/f"]),
        ("sed -i '' 's/a/b/' f", vec!["/work/proj/f"]),
        ("sed -ni 's/a/b/p' f", vec!["/work/proj/f"]),
        (
            "sed -i -e 's/a/b/' a b",
            vec!["/work/proj/a", "/work/proj/b"],
        ),
        ("sed -i -f script.sed f", vec!["/work/proj/f"]),
        ("sed -i 's/a/b/' /abs/f", vec!["/abs/f"]),
        ("perl -pi -e 's/a/b/' f", vec!["/work/proj/f"]),
        ("perl -i.bak -pe 's/a/b/' f", vec!["/work/proj/f"]),
        ("awk -i inplace '{print}' f", vec!["/work/proj/f"]),
        ("awk -v x=1 -i inplace '{print x}' f", vec!["/work/proj/f"]),
    ] {
        let expected = targets
            .into_iter()
            .map(|t| write(WriteRule::InPlaceEdit, path(t)))
            .collect();
        assert_hits(cmd, expected);
    }
}

#[test]
fn editors_without_in_place_pass() {
    for cmd in [
        "sed 's/a/b/' f",
        "sed -n p f",
        "sed -e 's/a/b/' f > /dev/null",
        "awk '{print}' f",
        "awk -f prog.awk f",
        "grep -r x .",
    ] {
        assert_clean(cmd);
    }
}

// ---- writes: file operations -----------------------------------------------

#[test]
fn file_ops_write_all_their_path_arguments() {
    for (cmd, targets) in [
        ("rm -rf build", vec!["/work/proj/build"]),
        ("rm a b", vec!["/work/proj/a", "/work/proj/b"]),
        ("rm -- -weird", vec!["/work/proj/-weird"]),
        ("rmdir d", vec!["/work/proj/d"]),
        ("mv a b", vec!["/work/proj/a", "/work/proj/b"]),
        ("touch x", vec!["/work/proj/x"]),
        ("touch -d yesterday f", vec!["/work/proj/f"]),
        ("mkdir -p a/b", vec!["/work/proj/a/b"]),
        ("mkdir -m 755 d", vec!["/work/proj/d"]),
        ("truncate -s 0 log", vec!["/work/proj/log"]),
        ("tee out.txt", vec!["/work/proj/out.txt"]),
        ("echo x | tee -a out.txt", vec!["/work/proj/out.txt"]),
        ("rm ../x", vec!["/work/x"]),
        ("rm ./a/../b", vec!["/work/proj/b"]),
        ("rm /a/./b", vec!["/a/b"]),
        ("rm ~/x", vec!["/home/u/x"]),
        ("rm 'a b'", vec!["/work/proj/a b"]),
        ("rm a\\ b", vec!["/work/proj/a b"]),
        ("rm 'a'\"b\"", vec!["/work/proj/ab"]),
        ("rm '*.rs'", vec!["/work/proj/*.rs"]),
        ("rm 'a$b'", vec!["/work/proj/a$b"]),
    ] {
        let expected = targets
            .into_iter()
            .map(|t| write(WriteRule::FileOp, path(t)))
            .collect();
        assert_hits(cmd, expected);
    }
}

#[test]
fn tee_to_device_and_missing_arguments_pass() {
    for cmd in ["tee /dev/null", "rm", "touch", "echo x | tee"] {
        assert_clean(cmd);
    }
}

#[test]
fn copy_like_commands_write_only_the_destination() {
    for (cmd, rule, dest) in [
        ("cp a b", WriteRule::CopyDest, "/work/proj/b"),
        ("cp -r a b/", WriteRule::CopyDest, "/work/proj/b"),
        ("cp a b c dest/", WriteRule::CopyDest, "/work/proj/dest"),
        ("cp src/x /tmp/", WriteRule::CopyDest, "/tmp"),
        ("cp -t dest a b", WriteRule::CopyDest, "/work/proj/dest"),
        (
            "cp --target-directory=dest a",
            WriteRule::CopyDest,
            "/work/proj/dest",
        ),
        (
            "install -m 755 a bin/a",
            WriteRule::CopyDest,
            "/work/proj/bin/a",
        ),
        ("ln -s a b", WriteRule::CopyDest, "/work/proj/b"),
        ("ln -t d a", WriteRule::CopyDest, "/work/proj/d"),
        ("ln a", WriteRule::CopyDest, "/work/proj"),
        ("dd if=a of=b", WriteRule::DdOutput, "/work/proj/b"),
        ("dd of=/abs/img bs=1", WriteRule::DdOutput, "/abs/img"),
    ] {
        assert_single_write(cmd, rule, dest);
    }
}

#[test]
fn install_directory_mode_writes_every_directory() {
    assert_hits(
        "install -d d1 d2",
        vec![
            write(WriteRule::FileOp, path("/work/proj/d1")),
            write(WriteRule::FileOp, path("/work/proj/d2")),
        ],
    );
}

#[test]
fn find_delete_writes_each_root() {
    assert_single_write("find . -name x -delete", WriteRule::FindDelete, CWD);
    assert_single_write("find /tmp/x -delete", WriteRule::FindDelete, "/tmp/x");
    assert_single_write(
        "find build -type f -delete",
        WriteRule::FindDelete,
        "/work/proj/build",
    );
    assert_hits(
        "find a b -delete",
        vec![
            write(WriteRule::FindDelete, path("/work/proj/a")),
            write(WriteRule::FindDelete, path("/work/proj/b")),
        ],
    );
    assert_single_write("find -delete", WriteRule::FindDelete, CWD);
    assert_clean("find . -name x");
    assert_clean("find . -name x -print");
}

#[test]
fn patch_writes_dir_or_named_files() {
    assert_single_write("patch -p1 < x.diff", WriteRule::Patch, CWD);
    assert_single_write(
        "patch -d sub -p1 < x.diff",
        WriteRule::Patch,
        "/work/proj/sub",
    );
    assert_single_write("patch -o out.c in.c", WriteRule::Patch, "/work/proj/out.c");
    assert_single_write("patch f.c fix.diff", WriteRule::Patch, "/work/proj/f.c");
    assert_single_write(
        "patch --directory=sub -i x.diff",
        WriteRule::Patch,
        "/work/proj/sub",
    );
}

#[test]
fn tar_extract_writes_cwd_or_dash_c_dir() {
    for (cmd, dir) in [
        ("tar -xf a.tgz", CWD),
        ("tar xzf a.tgz", CWD),
        ("tar xzf a.tgz -C d", "/work/proj/d"),
        ("tar -xf a.tgz -C /tmp/z", "/tmp/z"),
        ("tar -xf a.tgz --directory=/tmp/z", "/tmp/z"),
        ("tar --extract -f a.tgz", CWD),
        ("tar -C d -xf a.tgz", "/work/proj/d"),
    ] {
        assert_single_write(cmd, WriteRule::Extract, dir);
    }
    for cmd in ["tar -tf a.tgz", "tar tzf a.tgz", "tar -czf out.tgz src"] {
        assert_clean(cmd);
    }
}

#[test]
fn unzip_extract_writes_cwd_or_dash_d_dir() {
    for (cmd, dir) in [
        ("unzip a.zip", CWD),
        ("unzip a.zip -d out", "/work/proj/out"),
        ("unzip -d /tmp/x a.zip", "/tmp/x"),
        ("unzip -o a.zip", CWD),
    ] {
        assert_single_write(cmd, WriteRule::Extract, dir);
    }
    for cmd in [
        "unzip -l a.zip",
        "unzip -t a.zip",
        "unzip -p a.zip f",
        "unzip -Z a.zip",
    ] {
        assert_clean(cmd);
    }
}

#[test]
fn read_only_commands_are_clean() {
    for cmd in [
        "cat f",
        "ls -la",
        "grep -r x .",
        "echo hello",
        "pwd",
        "cargo test",
        "cargo build --release",
        "npm test",
        "git status",
        "git diff HEAD~1",
        "git log -n 5",
        "git stash list",
        "gh pr view 1",
        "",
        "   ",
    ] {
        assert_clean(cmd);
    }
}

// ---- writes: git working-tree writers --------------------------------------

#[test]
fn git_working_tree_writers_anchor_at_cwd_or_dash_c() {
    for (cmd, dir) in [
        ("git apply p.diff", CWD),
        ("git apply --index p.diff", CWD),
        ("git -C /o apply p.diff", "/o"),
        ("git am p.mbox", CWD),
        ("git rm f", CWD),
        ("git mv a b", CWD),
        ("git stash pop", CWD),
        ("git stash apply", CWD),
        ("git -C sub stash pop", "/work/proj/sub"),
    ] {
        assert_single_write(cmd, WriteRule::GitWrite, dir);
    }
}

#[test]
fn git_read_only_forms_pass() {
    for cmd in [
        "git apply --check p.diff",
        "git apply --stat p.diff",
        "git apply --numstat p.diff",
        "git apply --summary p.diff",
        "git stash list",
        "git stash",
        "git stash show",
        "git status",
        "git diff",
        "git log",
        "git checkout main",
        "git pull",
        "git fetch",
    ] {
        assert_clean(cmd);
    }
}

// ---- unresolved targets ----------------------------------------------------

#[test]
fn dynamic_targets_carry_their_literal_prefix() {
    for (cmd, prefix, raw) in [
        ("rm -rf /tmp/foo*", Some("/tmp"), "/tmp/foo*"),
        ("rm /tmp/$X/f", Some("/tmp"), "/tmp/$X/f"),
        ("rm *.rs", Some(CWD), "*.rs"),
        ("rm src/*.rs", Some("/work/proj/src"), "src/*.rs"),
        ("rm a{b,c}", Some(CWD), "a{b,c}"),
        ("rm file?.txt", Some(CWD), "file?.txt"),
        ("rm [ab].txt", Some(CWD), "[ab].txt"),
        ("rm $X", None, "$X"),
        ("rm ${X}/a", None, "${X}/a"),
        (r#"rm "$DIR/a""#, None, "$DIR/a"),
        ("rm $(ls)", None, "$(ls)"),
        ("rm `ls`", None, "`ls`"),
        ("rm \"$(pwd)\"/x", None, "$(pwd)/x"),
        (r#"rm "$HOME/x""#, None, "$HOME/x"),
    ] {
        assert_hits(cmd, vec![write(WriteRule::FileOp, unresolved(prefix, raw))]);
    }
}

#[test]
fn tilde_needs_home() {
    let a = classify(&ClassifyInput {
        command: "rm ~/x".to_string(),
        cwd: PathBuf::from(CWD),
        home: None,
    });
    assert_eq!(
        a.hits
            .into_iter()
            .map(|h| (h.kind, h.anchor))
            .collect::<Vec<_>>(),
        vec![write(WriteRule::FileOp, unresolved(None, "~/x"))]
    );
    assert_hits(
        "rm ~other/x",
        vec![write(WriteRule::FileOp, unresolved(None, "~other/x"))],
    );
    assert_single_write("rm \"~/x\"", WriteRule::FileOp, "/work/proj/~/x");
}

// ---- cd semantics ---------------------------------------------------------

#[test]
fn cd_applies_to_following_segments_of_the_same_shell() {
    for (cmd, target) in [
        ("cd sub && rm f", "/work/proj/sub/f"),
        ("cd sub; rm f", "/work/proj/sub/f"),
        ("cd sub\nrm f", "/work/proj/sub/f"),
        ("cd /a; cd b; rm f", "/a/b/f"),
        ("cd .. && rm f", "/work/f"),
        ("cd ~/x && rm f", "/home/u/x/f"),
        ("cd && rm f", "/home/u/f"),
        ("cd -P sub && rm f", "/work/proj/sub/f"),
        ("{ cd sub; rm f; }", "/work/proj/sub/f"),
        ("(cd sub && rm f)", "/work/proj/sub/f"),
        ("cd sub && cd .. && rm f", "/work/proj/f"),
    ] {
        assert_single_write(cmd, WriteRule::FileOp, target);
    }
}

#[test]
fn cd_does_not_leak_out_of_subshells_pipes_or_background() {
    assert_hits(
        "(cd sub && rm f); rm g",
        vec![
            write(WriteRule::FileOp, path("/work/proj/sub/f")),
            write(WriteRule::FileOp, path("/work/proj/g")),
        ],
    );
    assert_single_write("cd sub | rm f", WriteRule::FileOp, "/work/proj/f");
    assert_single_write("cd sub & rm f", WriteRule::FileOp, "/work/proj/f");
    assert_single_write("echo | cd sub; rm f", WriteRule::FileOp, "/work/proj/f");
}

#[test]
fn cd_to_unknown_place_makes_relative_targets_unresolved() {
    for (cmd, raw) in [
        ("cd $X && rm f", "$X/f"),
        ("cd \"$X\"; rm f", "$X/f"),
        ("cd - && rm f", "-/f"),
        ("pushd sub && rm f", "pushd/f"),
        ("cd $(mktemp -d) && rm f", "$(mktemp -d)/f"),
    ] {
        assert_hits(cmd, vec![write(WriteRule::FileOp, unresolved(None, raw))]);
    }
    assert_single_write("cd $X && rm /abs/f", WriteRule::FileOp, "/abs/f");
    assert_hits(
        "cd /tmp/sub* && rm f",
        vec![write(
            WriteRule::FileOp,
            unresolved(Some("/tmp"), "/tmp/sub*/f"),
        )],
    );
}

// ---- wrappers ------------------------------------------------------------

#[test]
fn transparent_wrappers_classify_the_inner_command() {
    for cmd in [
        "sudo rm f",
        "sudo -u bob rm f",
        "sudo -E -H rm f",
        "sudo -- rm f",
        "nohup rm f &",
        "timeout 5 rm f",
        "timeout -s KILL 5 rm f",
        "timeout --signal=KILL 5s rm f",
        "time rm f",
        "time -p rm f",
        "command rm f",
        "command -p rm f",
        "env FOO=1 rm f",
        "env -i rm f",
        "env -u X rm f",
        "FOO=1 rm f",
        "sudo -E env X=1 rm f",
        "bash -c 'rm f'",
        "sh -c \"rm f\"",
        "bash -lc 'rm f'",
        "bash -c 'echo hi; rm f'",
        "exec rm f",
        "exec -a name rm f",
        "builtin rm f",
    ] {
        assert_single_write(cmd, WriteRule::FileOp, "/work/proj/f");
    }
}

#[test]
fn wrapper_directory_options_move_the_inner_cwd() {
    assert_single_write("env -C sub rm f", WriteRule::FileOp, "/work/proj/sub/f");
    assert_single_write("sudo -D sub rm f", WriteRule::FileOp, "/work/proj/sub/f");
    assert_single_write(
        "bash -c 'cd sub && rm f'",
        WriteRule::FileOp,
        "/work/proj/sub/f",
    );
    assert_single_write(
        "cd sub && bash -c 'rm f'",
        WriteRule::FileOp,
        "/work/proj/sub/f",
    );
}

#[test]
fn wrappers_around_harmless_commands_are_clean() {
    for cmd in [
        "sudo ls",
        "nohup sleep 1 &",
        "timeout 5 sleep 1",
        "time ls",
        "command -v rm",
        "command -V git",
        "env",
        "env FOO=1",
        "sudo",
        "sudo -l",
        "bash -c 'echo hi'",
        "xargs echo",
        "xargs -0 -n 1 echo",
        "find . -exec echo {} \\;",
    ] {
        assert_clean(cmd);
    }
}

#[test]
fn xargs_inner_command_gets_unknown_stdin_targets() {
    assert_hits(
        "xargs rm",
        vec![write(WriteRule::FileOp, unresolved(None, "<stdin>"))],
    );
    assert_hits(
        "find . -name x | xargs rm -f",
        vec![write(WriteRule::FileOp, unresolved(None, "<stdin>"))],
    );
    assert_hits(
        "git ls-files | xargs -0 -n 5 rm",
        vec![write(WriteRule::FileOp, unresolved(None, "<stdin>"))],
    );
    for cmd in [
        "xargs -I{} rm {}",
        "xargs -I % rm %",
        "xargs -i rm {}",
        "xargs --replace=@ rm @",
        "xargs --replace rm {}",
        "xargs -n1 -I{} rm -f {}",
    ] {
        assert_hits(
            cmd,
            vec![write(WriteRule::FileOp, unresolved(None, "<stdin>"))],
        );
    }
    assert_hits(
        "xargs -I{} rm dir/{}",
        vec![write(
            WriteRule::FileOp,
            unresolved(Some("/work/proj/dir"), "dir/<stdin>"),
        )],
    );
    assert_single_write("xargs -I{} rm fixed", WriteRule::FileOp, "/work/proj/fixed");
    assert_hits(
        "xargs -I{} cp {} /tmp/out",
        vec![write(WriteRule::CopyDest, path("/tmp/out"))],
    );
}

#[test]
fn find_exec_inner_command_targets_files_under_the_roots() {
    assert_hits(
        "find . -exec rm {} \\;",
        vec![write(WriteRule::FileOp, unresolved(Some(CWD), "./*"))],
    );
    assert_hits(
        "find /tmp/x -exec rm {} +",
        vec![write(
            WriteRule::FileOp,
            unresolved(Some("/tmp/x"), "/tmp/x/*"),
        )],
    );
    assert_hits(
        "find src -execdir rm {} \\;",
        vec![write(
            WriteRule::FileOp,
            unresolved(Some("/work/proj/src"), "src/*"),
        )],
    );
    assert_hits(
        "find . -name '*.o' -exec sed -i s/a/b/ {} \\;",
        vec![write(WriteRule::InPlaceEdit, unresolved(Some(CWD), "./*"))],
    );
}

#[test]
fn wrapper_that_cannot_be_unwrapped_is_opaque() {
    for (cmd, expected) in [
        ("find . -exec rm {}", wrapper(Wrapper::FindExec)),
        ("timeout rm f", wrapper(Wrapper::Timeout)),
        ("sudo -u", wrapper(Wrapper::Sudo)),
        ("sudo -s", wrapper(Wrapper::Sudo)),
        ("env -S 'a b' rm", wrapper(Wrapper::Env)),
        ("env --bogus rm f", wrapper(Wrapper::Env)),
        ("bash -c", wrapper(Wrapper::ShellC)),
        ("bash -c \"unterminated 'quote\"", wrapper(Wrapper::ShellC)),
    ] {
        assert_hits(cmd, vec![expected]);
    }
}

#[test]
fn wrapper_nesting_is_limited_to_depth_two() {
    assert_single_write("sudo env rm f", WriteRule::FileOp, "/work/proj/f");
    assert_hits("sudo env nohup rm f", vec![wrapper(Wrapper::Nohup)]);
    assert_hits("nohup time command rm f", vec![wrapper(Wrapper::Command)]);
    assert_hits("sudo env xargs rm", vec![wrapper(Wrapper::Xargs)]);
    assert_hits("sudo env time rm f", vec![wrapper(Wrapper::Time)]);
    assert_hits(
        "sudo env find . -exec rm {} \\;",
        vec![wrapper(Wrapper::FindExec)],
    );
    assert_single_write(
        r#"bash -c "bash -c 'rm f'""#,
        WriteRule::FileOp,
        "/work/proj/f",
    );
    assert_hits(
        r##"bash -c "bash -c \"bash -c 'rm f'\"""##,
        vec![wrapper(Wrapper::ShellC)],
    );
}

#[test]
fn bypass_inside_wrappers_is_found() {
    for (cmd, rule) in [
        (
            "bash -c \"git commit --no-verify -m x\"",
            BypassRule::NoVerifyFlag,
        ),
        ("sudo git commit -n -m x", BypassRule::CommitShortNoVerify),
        (
            "env -i git -c core.hooksPath=/x commit",
            BypassRule::HooksPathConfig,
        ),
        ("xargs git commit --no-verify", BypassRule::NoVerifyFlag),
        ("nohup git push --no-verify &", BypassRule::NoVerifyFlag),
    ] {
        assert_bypass(cmd, rule);
    }
}

// ---- opaque executions -----------------------------------------------------

#[test]
fn interpreters_running_code_are_opaque_at_the_effective_cwd() {
    for (cmd, name) in [
        ("node x.js", "node"),
        ("node -e 'console.log(1)'", "node"),
        ("python3 script.py", "python3"),
        ("python -c 'print(1)'", "python"),
        ("python3.11 x.py", "python3.11"),
        ("/usr/bin/python3 x.py", "python3"),
        ("python -m pip install x", "python"),
        ("ruby x.rb", "ruby"),
        ("perl script.pl", "perl"),
        ("perl -e 'print 1'", "perl"),
        ("deno run x.ts", "deno"),
        ("bun x.ts", "bun"),
        ("php x.php", "php"),
        ("bash script.sh", "bash"),
        ("sh x.sh arg", "sh"),
        ("zsh x.zsh", "zsh"),
        ("node", "node"),
        ("echo 'print(1)' | python3", "python3"),
        ("PYTHONPATH=. python3 x.py", "python3"),
    ] {
        assert_hits(cmd, vec![interpreter(name)]);
    }
}

#[test]
fn opaque_anchor_is_the_effective_cwd_not_the_script_path() {
    assert_hits("node /other/repo/x.js", vec![interpreter("node")]);
    assert_hits(
        "cd sub && python3 /elsewhere/x.py",
        vec![(
            HitKind::Opaque(OpaqueCause::Interpreter("python3".to_string())),
            path("/work/proj/sub"),
        )],
    );
    assert_hits(
        "(cd /tmp && node x.js); node y.js",
        vec![
            (
                HitKind::Opaque(OpaqueCause::Interpreter("node".to_string())),
                path("/tmp"),
            ),
            interpreter("node"),
        ],
    );
}

#[test]
fn script_paths_eval_and_dynamic_commands_are_opaque() {
    assert_hits(
        "./x",
        vec![opaque(OpaqueCause::ScriptFile("./x".to_string()))],
    );
    assert_hits(
        "scripts/run.sh --flag",
        vec![opaque(OpaqueCause::ScriptFile(
            "scripts/run.sh".to_string(),
        ))],
    );
    assert_hits(
        "../tools/run.sh",
        vec![opaque(OpaqueCause::ScriptFile(
            "../tools/run.sh".to_string(),
        ))],
    );
    assert_hits(
        "a/b",
        vec![opaque(OpaqueCause::ScriptFile("a/b".to_string()))],
    );
    assert_hits("eval \"$X\"", vec![opaque(OpaqueCause::Eval)]);
    assert_hits("$CMD arg", vec![opaque(OpaqueCause::DynamicCommand)]);
    assert_hits(
        "bash -c \"$CMD\"",
        vec![opaque(OpaqueCause::DynamicCommand)],
    );
}

#[test]
fn trusted_tools_pass() {
    for cmd in [
        "python -m pytest",
        "python3 -m pytest -q tests",
        "python -m unittest discover",
        "node --test",
        "node --test tests/",
        "node --version",
        "python --version",
        "python3 -V",
        "ruby --version",
        "bash --version",
        "bun test",
        "deno test",
        "cargo test",
        "cargo clippy --all-targets",
        "npm test",
        "npm run build",
        "npm install",
        "npm ci",
        "pnpm run lint",
        "pnpm install",
        "pnpm dlx create-x",
        "yarn test",
        "npx vitest",
        "make build",
        "make",
    ] {
        assert_clean(cmd);
    }
}

// ---- lexer ----------------------------------------------------------------

#[test]
fn heredoc_body_is_never_tokenized() {
    for cmd in [
        "cat <<EOF\nrm -rf /\nEOF",
        "cat <<'EOF'\nit's\nEOF",
        "cat <<\"EOF\"\nit's \"unterminated\nEOF",
        "cat <<\\EOF\nrm x\nEOF",
        "cat <<-EOF\n\trm x\n\tEOF",
        "cat <<'EOF'\n$(rm x)\n`rm y`\nEOF",
        "cat <<EOF\n\\$(rm x) \\`rm y\\`\nEOF",
        "cat <<EOF\nline\nEOF\n",
    ] {
        assert_clean(cmd);
    }
}

#[test]
fn substitutions_in_an_expanding_heredoc_body_are_classified() {
    assert_hits(
        "cat <<EOF\n$(rm x)\n`rm y`\nEOF",
        vec![
            write(WriteRule::FileOp, path("/work/proj/x")),
            write(WriteRule::FileOp, path("/work/proj/y")),
        ],
    );
    assert_hits(
        "cd sub && cat <<EOF > /tmp/f\nsay \"hi\" $(rm a)\nEOF",
        vec![
            write(WriteRule::FileOp, path("/work/proj/sub/a")),
            write(WriteRule::Redirect, path("/tmp/f")),
        ],
    );
}

#[test]
fn commands_around_heredocs_are_still_classified() {
    assert_hits(
        "cat <<EOF > f\nbody\nEOF\nrm g",
        vec![
            write(WriteRule::Redirect, path("/work/proj/f")),
            write(WriteRule::FileOp, path("/work/proj/g")),
        ],
    );
    assert_hits(
        "cat <<A <<B > f\none\nA\ntwo\nB\nrm g",
        vec![
            write(WriteRule::Redirect, path("/work/proj/f")),
            write(WriteRule::FileOp, path("/work/proj/g")),
        ],
    );
    assert_hits(
        "rm a <<EOF\nbody\nEOF",
        vec![write(WriteRule::FileOp, path("/work/proj/a"))],
    );
}

#[test]
fn unterminated_heredoc_swallows_the_rest() {
    assert_clean("cat <<EOF\nrm x");
}

#[test]
fn commit_message_heredoc_inside_substitution_is_a_plain_commit() {
    for body in [
        "fix: don't break it's flow",
        "fix: don't break flow",
        "fix: \"quoted",
    ] {
        let cmd = format!("git commit -m \"$(cat <<'EOF'\n{body}\nEOF\n)\"");
        let a = classify(&input(&cmd));
        assert!(a.bypass.is_empty(), "{a:?}");
        assert!(
            !a.hits
                .iter()
                .any(|h| h.kind == HitKind::Opaque(OpaqueCause::Unparsed)),
            "heredoc body {body:?} broke the lexer: {a:?}"
        );
        assert!(
            a.hits.iter().any(|h| h.kind == HitKind::Commit),
            "expected a commit hit, got {a:?}"
        );
    }
}

#[test]
fn comments_run_to_end_of_line() {
    for cmd in [
        "ls # rm -rf x",
        "# git commit --no-verify",
        "ls #it's fine",
        "echo a;# rm x",
    ] {
        assert_clean(cmd);
    }
    assert_single_write("echo a#b > f", WriteRule::Redirect, "/work/proj/f");
    assert_single_write(
        "echo \"# not a comment\" > f",
        WriteRule::Redirect,
        "/work/proj/f",
    );
    assert_single_write("echo '#' > f", WriteRule::Redirect, "/work/proj/f");
    assert_hits(
        "# note\nrm f",
        vec![write(WriteRule::FileOp, path("/work/proj/f"))],
    );
    assert_single_write("echo ${#X} > f", WriteRule::Redirect, "/work/proj/f");
}

#[test]
fn ansi_c_quotes_are_literal() {
    assert_single_write("rm $'a b'", WriteRule::FileOp, "/work/proj/a b");
    assert_single_write("rm $'a\\tb'", WriteRule::FileOp, "/work/proj/a\tb");
    assert_single_write("echo $'it\\'s' > f", WriteRule::Redirect, "/work/proj/f");
    assert_single_write("rm $'a$b'", WriteRule::FileOp, "/work/proj/a$b");
}

#[test]
fn separators_split_commands() {
    for cmd in [
        "rm a; rm b",
        "rm a && rm b",
        "rm a || rm b",
        "rm a & rm b",
        "rm a\nrm b",
        "rm a | rm b",
        "(rm a) ; (rm b)",
        "{ rm a; rm b; }",
        "if true; then rm a; rm b; fi",
        "! rm a; rm b",
    ] {
        assert_hits(
            cmd,
            vec![
                write(WriteRule::FileOp, path("/work/proj/a")),
                write(WriteRule::FileOp, path("/work/proj/b")),
            ],
        );
    }
}

#[test]
fn line_continuation_joins_words() {
    assert_single_write("rm \\\nf", WriteRule::FileOp, "/work/proj/f");
}

#[test]
fn quoted_flags_are_still_flags() {
    for (cmd, rule) in [
        ("git commit '--no-verify'", BypassRule::NoVerifyFlag),
        (r#"git commit "--no-verify""#, BypassRule::NoVerifyFlag),
        (r"git commit --no\-verify", BypassRule::NoVerifyFlag),
        (r#"git commit "-n""#, BypassRule::CommitShortNoVerify),
        (
            r#"git "-c" core.hooksPath=x commit"#,
            BypassRule::HooksPathConfig,
        ),
        (
            "git '-c' core.hooksPath=x commit -m x",
            BypassRule::HooksPathConfig,
        ),
    ] {
        assert_bypass(cmd, rule);
    }
    assert_hits(
        "git '-C' dir commit -m x",
        vec![(HitKind::Commit, path("/work/proj/dir"))],
    );
    assert_single_write("sed '-i' s/a/b/ f", WriteRule::InPlaceEdit, "/work/proj/f");
    assert_single_write(
        "find build '-delete'",
        WriteRule::FindDelete,
        "/work/proj/build",
    );
}

#[test]
fn quoted_option_values_are_still_not_flags() {
    for cmd in [
        r#"git commit -m "-n""#,
        "git commit -m '--no-verify'",
        r#"git commit -m "--no-verify""#,
        "git commit -F '--no-verify'",
    ] {
        assert_no_bypass(cmd);
    }
    assert_single_write("sed -i '' s/a/b/ f", WriteRule::InPlaceEdit, "/work/proj/f");
}

#[test]
fn quoted_check_flag_makes_git_apply_read_only() {
    assert_clean("git apply '--check' p");
    assert_clean(r#"git apply "--stat" p"#);
}

#[test]
fn quoted_tar_and_find_options_are_still_options() {
    assert_single_write("tar '-xf' a.tgz", WriteRule::Extract, CWD);
    assert_hits(
        "find . '-exec' rm {} \\;",
        vec![write(WriteRule::FileOp, unresolved(Some(CWD), "./*"))],
    );
}

#[test]
fn unterminated_constructs_are_read_to_the_end_and_flagged_unparsed() {
    for cmd in [
        "echo \"abc",
        "echo 'abc",
        "echo $'abc",
        "echo $(abc",
        "echo `abc",
        "echo ${abc",
    ] {
        assert_hits(cmd, vec![opaque(OpaqueCause::Unparsed)]);
    }
    assert_hits(
        "git commit -m \"msg",
        vec![(HitKind::Commit, path(CWD)), opaque(OpaqueCause::Unparsed)],
    );
}

#[test]
fn hook_bypass_is_caught_despite_an_unterminated_quote() {
    for cmd in [
        "git commit --no-verify -m \"x",
        "git commit --no-verify -m x\necho \"oops",
        "HUSKY=0 git commit -m 'x",
    ] {
        let a = analyze(cmd);
        assert!(!a.bypass.is_empty(), "{cmd:?}: {a:?}");
    }
}

#[test]
fn writes_before_an_unterminated_quote_are_still_classified() {
    assert_hits(
        "rm f\necho \"oops",
        vec![
            write(WriteRule::FileOp, path("/work/proj/f")),
            opaque(OpaqueCause::Unparsed),
        ],
    );
}

#[test]
fn escaped_quote_does_not_open_a_string() {
    assert_clean("echo \\\"git commit");
    assert_clean("echo \\'x");
    assert_single_write("echo \\\"a > f", WriteRule::Redirect, "/work/proj/f");
}

// ---- purity -----------------------------------------------------------

#[test]
fn same_input_gives_same_output() {
    let cmd = "cd sub && sed -i 's/a/b/' f; git commit -n -m x; node x.js";
    assert_eq!(analyze(cmd), analyze(cmd));
}

#[test]
fn nonexistent_cwd_and_relative_cwd_do_not_matter() {
    let a = classify(&ClassifyInput {
        command: "rm f".to_string(),
        cwd: PathBuf::from("/definitely/not/here"),
        home: None,
    });
    assert_eq!(a.hits[0].anchor, path("/definitely/not/here/f"));
}

#[test]
fn hit_reports_the_program_that_matched() {
    let a = analyze("sed -i s/a/b/ f");
    assert_eq!(a.hits[0].program, "sed");
    let a = analyze("git -C /o apply p");
    assert_eq!(a.hits[0].program, "git apply");
    let a = analyze("echo x > f");
    assert_eq!(a.hits[0].program, ">");
}

#[test]
fn mixed_pipeline_reports_every_hit_in_order() {
    assert_hits(
        "cd sub && sed -i 's/a/b/' f && git commit -m x && node run.js",
        vec![
            write(WriteRule::InPlaceEdit, path("/work/proj/sub/f")),
            (HitKind::Commit, path("/work/proj/sub")),
            (
                HitKind::Opaque(OpaqueCause::Interpreter("node".to_string())),
                path("/work/proj/sub"),
            ),
        ],
    );
}

// ---- persistent environment ------------------------------------------------

#[test]
fn exported_hook_env_applies_to_later_segments() {
    for (cmd, rule) in [
        ("export HUSKY=0; git commit -m x", BypassRule::HookSkipEnv),
        ("export HUSKY=0 && git push", BypassRule::HookSkipEnv),
        ("HUSKY=0; git commit -m x", BypassRule::HookSkipEnv),
        ("SKIP=lint\ngit commit -m x", BypassRule::HookSkipEnv),
        (
            "declare -x HUSKY=0; git commit -m x",
            BypassRule::HookSkipEnv,
        ),
        (
            "export FOO=1 SKIP=x; git commit -m x",
            BypassRule::HookSkipEnv,
        ),
        (
            "export GIT_CONFIG_KEY_0=core.hooksPath; git commit -m x",
            BypassRule::HooksPathEnv,
        ),
        (
            "export HUSKY=0; bash -c 'git commit -m x'",
            BypassRule::HookSkipEnv,
        ),
        (
            "export HUSKY=0; env git commit -m x",
            BypassRule::HookSkipEnv,
        ),
        (
            "{ export HUSKY=0; git commit -m x; }",
            BypassRule::HookSkipEnv,
        ),
    ] {
        assert_bypass(cmd, rule);
    }
}

#[test]
fn persistent_env_does_not_leak_or_misfire() {
    for cmd in [
        "(export HUSKY=0); git commit -m x",
        "export HUSKY=0 | git commit -m x",
        "HUSKY=0 & git commit -m x",
        "export HUSKY=0; export HUSKY=1; git commit -m x",
        "HUSKY=0; HUSKY=1; git commit -m x",
        "export HUSKY=0; git status",
        "export HUSKY=1; git commit -m x",
        "declare HUSKY=0; git commit -m x",
        "export FOO=1; git commit -m x",
    ] {
        assert_no_bypass(cmd);
    }
}

#[test]
fn persistent_env_assignments_are_not_hits() {
    assert_hits(
        "export HUSKY=0; git commit -m x",
        vec![(HitKind::Commit, path(CWD))],
    );
    assert_clean("export FOO=bar");
}

// ---- absolute-path programs ------------------------------------------------

#[test]
fn absolute_path_programs_are_classified_by_basename() {
    assert_clean("/usr/bin/ls");
    assert_clean("/usr/local/bin/cargo test");
    assert_clean("/opt/tool/run.sh");
    assert_single_write("/bin/rm f", WriteRule::FileOp, "/work/proj/f");
    assert_hits("/usr/bin/python3 x.py", vec![interpreter("python3")]);
}

// ---- command substitution --------------------------------------------------

#[test]
fn command_substitution_bodies_are_classified() {
    for cmd in [
        "echo $(rm f)",
        "echo `rm f`",
        "echo \"$(rm f)\"",
        "echo \"`rm f`\"",
        "echo x > /dev/null $(rm f)",
        "echo pre$(rm f)post",
    ] {
        assert_single_write(cmd, WriteRule::FileOp, "/work/proj/f");
    }
    assert_single_write(
        "cd sub && echo $(rm f)",
        WriteRule::FileOp,
        "/work/proj/sub/f",
    );
    assert_single_write(
        "echo $(cd sub && rm f)",
        WriteRule::FileOp,
        "/work/proj/sub/f",
    );
    assert_single_write("echo $(echo `rm f`)", WriteRule::FileOp, "/work/proj/f");
}

#[test]
fn command_substitution_finds_bypass_commits_and_opaque_runs() {
    assert_bypass(
        "echo $(git commit --no-verify -m x)",
        BypassRule::NoVerifyFlag,
    );
    assert_bypass("echo `git commit -n -m x`", BypassRule::CommitShortNoVerify);
    assert_hits(
        "echo $(git commit -m x)",
        vec![(HitKind::Commit, path(CWD))],
    );
    assert_hits("echo $(node x.js)", vec![interpreter("node")]);
}

#[test]
fn substitution_does_not_leak_cwd_or_run_inside_single_quotes_or_heredocs() {
    assert_single_write("echo $(cd sub); rm f", WriteRule::FileOp, "/work/proj/f");
    assert_clean("echo '$(rm f)'");
    assert_clean("echo '`rm f`'");
    assert_clean("echo $((1 + 2))");
    assert_clean("echo $((a > b))");
    assert_clean("cat <<'EOF'\n$(rm f)\nEOF");
}

#[test]
fn substitution_nesting_beyond_depth_two_is_opaque() {
    assert_single_write("echo $(echo $(rm f))", WriteRule::FileOp, "/work/proj/f");
    assert_hits(
        "echo $(echo $(echo $(rm f)))",
        vec![opaque(OpaqueCause::Substitution)],
    );
    assert_hits(
        "bash -c 'echo $(echo $(rm f))'",
        vec![opaque(OpaqueCause::Substitution)],
    );
}

#[test]
fn unlexable_substitution_body_is_opaque() {
    assert_hits("echo `echo 'x`", vec![opaque(OpaqueCause::Substitution)]);
}

// ---- source ------------------------------------------------------------------

#[test]
fn source_and_dot_are_opaque_script_files() {
    assert_hits(
        "source x.sh",
        vec![opaque(OpaqueCause::ScriptFile("x.sh".to_string()))],
    );
    assert_hits(
        ". ./env.sh",
        vec![opaque(OpaqueCause::ScriptFile("./env.sh".to_string()))],
    );
    assert_hits(
        "cd sub && source x.sh",
        vec![(
            HitKind::Opaque(OpaqueCause::ScriptFile("x.sh".to_string())),
            path("/work/proj/sub"),
        )],
    );
}

// ---- writes: downloads and rsync -------------------------------------------

#[test]
fn download_tools_write_their_output_targets() {
    for (cmd, target) in [
        ("curl -o out.bin http://x", "/work/proj/out.bin"),
        ("curl http://x -o out.bin", "/work/proj/out.bin"),
        ("curl --output out.bin http://x", "/work/proj/out.bin"),
        ("curl --output=out.bin http://x", "/work/proj/out.bin"),
        ("curl -sSLo out.bin http://x", "/work/proj/out.bin"),
        ("curl -o /abs/f http://x", "/abs/f"),
        ("curl -H 'X: a' -o out.bin http://x", "/work/proj/out.bin"),
        ("curl --output-dir d -o f http://x", "/work/proj/d/f"),
        ("wget -O out.bin http://x", "/work/proj/out.bin"),
        (
            "wget --output-document=out.bin http://x",
            "/work/proj/out.bin",
        ),
        (
            "wget --output-document out.bin http://x",
            "/work/proj/out.bin",
        ),
        ("wget -P dl http://x", "/work/proj/dl"),
        ("wget --directory-prefix=dl http://x", "/work/proj/dl"),
    ] {
        assert_single_write(cmd, WriteRule::Download, target);
    }
    assert_hits(
        "curl -o a http://x -o b",
        vec![
            write(WriteRule::Download, path("/work/proj/a")),
            write(WriteRule::Download, path("/work/proj/b")),
        ],
    );
}

#[test]
fn downloads_to_stdout_or_nowhere_pass() {
    for cmd in [
        "curl http://x",
        "curl -s http://x | cat",
        "curl -o - http://x",
        "curl -o /dev/null http://x",
        "curl -HContent-Type:x http://x",
        "wget -O - http://x",
        "wget -qO- http://x",
        "wget http://x",
    ] {
        assert_clean(cmd);
    }
}

#[test]
fn rsync_writes_its_destination() {
    for (cmd, dest) in [
        ("rsync -a src/ dest/", "/work/proj/dest"),
        ("rsync -av a b dest", "/work/proj/dest"),
        ("rsync -a --exclude x src/ /abs/dest", "/abs/dest"),
        ("rsync -e ssh -a src dest", "/work/proj/dest"),
        ("rsync a b", "/work/proj/b"),
    ] {
        assert_single_write(cmd, WriteRule::CopyDest, dest);
    }
}

#[test]
fn rsync_without_local_destination_passes() {
    for cmd in [
        "rsync -a src/",
        "rsync -an src dest",
        "rsync --dry-run src dest",
        "rsync --list-only src dest",
        "rsync -a src host:dest",
        "rsync -a src user@host:/dest",
        "rsync -a src rsync://host/mod",
    ] {
        assert_clean(cmd);
    }
}

// ---- conditional cd ----------------------------------------------------------

#[test]
fn cd_that_may_not_have_run_leaves_the_cwd_unknown() {
    for cmd in [
        "cd /tmp || true; rm a",
        "true && cd /tmp; rm a",
        "cd /tmp || echo no\nrm a",
        "cd /tmp || { echo no; }; rm a",
        "cd /tmp || { false && exit 1; }; rm a",
        "cd /tmp || { [ -n \"$F\" ] || exit 1; }; rm a",
    ] {
        assert_hits(
            cmd,
            vec![write(WriteRule::FileOp, unresolved(None, "cd/a"))],
        );
    }
}

#[test]
fn cd_that_certainly_ran_moves_the_cwd() {
    for cmd in [
        "cd /tmp && rm a",
        "cd /tmp; rm a",
        "cd /tmp || exit 1; rm a",
        "cd /tmp || return; rm a",
        "cd /tmp || { echo no; exit 1; }; rm a",
        "cd /tmp || { exit 1; }; rm a",
        "{ cd /tmp; }; rm a",
        "f() (true); { cd /tmp; }; rm a",
    ] {
        assert_single_write(cmd, WriteRule::FileOp, "/tmp/a");
    }
}

#[test]
fn cd_inside_a_function_body_does_not_move_the_caller() {
    for cmd in [
        "f() { cd /tmp; }; rm a",
        "f () { cd /tmp; }\nrm a",
        "function f { cd /tmp; }; rm a",
        "function f() { cd /tmp; }; rm a",
    ] {
        assert_single_write(cmd, WriteRule::FileOp, "/work/proj/a");
    }
    assert_hits(
        "f() { cd /tmp; rm x; }; rm a",
        vec![
            write(WriteRule::FileOp, path("/tmp/x")),
            write(WriteRule::FileOp, path("/work/proj/a")),
        ],
    );
}

// ---- hooks directory -----------------------------------------------------------

#[test]
fn writes_into_the_hooks_directory_are_hook_bypass() {
    for cmd in [
        "rm .git/hooks/pre-commit",
        "rm -f /work/proj/.git/hooks/*",
        "mv .git/hooks/pre-commit /tmp/",
        "chmod -x .git/hooks/pre-commit",
        "rm -f .git/hoo*/pre-commit",
        "cp a .git/hook?/pre-commit",
        "echo hi > .git/hoo*/pre-commit",
        "rm -rf .git/*",
        "rm -f .git/$D/pre-commit",
        "echo exit 0 > .git/hooks/pre-commit",
        "cd .git && rm hooks/commit-msg",
    ] {
        assert_bypass(cmd, BypassRule::HooksDirWrite);
    }
    for cmd in [
        "cat .git/hooks/pre-commit",
        "ls .git/hooks",
        "rm .github/hooks.md",
        "rm -f .git/*.lock",
        "rm -f .git/refs/*/old",
    ] {
        assert_no_bypass(cmd);
    }
}

#[test]
fn chmod_writes_its_files_but_not_its_mode() {
    assert_single_write("chmod +x run.sh", WriteRule::FileOp, "/work/proj/run.sh");
    assert_single_write("chmod -R 755 dir", WriteRule::FileOp, "/work/proj/dir");
    assert_single_write("chmod -x run.sh", WriteRule::FileOp, "/work/proj/run.sh");
    assert_single_write("chmod --reference=a b", WriteRule::FileOp, "/work/proj/b");
    assert_single_write("chmod 644 /tmp/x", WriteRule::FileOp, "/tmp/x");
}

// ---- branch switch -------------------------------------------------------------

fn branches(command: &str) -> Vec<Option<BranchSwitch>> {
    analyze(command)
        .hits
        .into_iter()
        .map(|h| h.on_branch)
        .collect()
}

fn switched(repo: &str, branch: &str) -> Option<BranchSwitch> {
    Some(BranchSwitch {
        repo: path(repo),
        branch: branch.to_string(),
    })
}

#[test]
fn effects_chained_after_a_branch_switch_land_on_the_new_branch() {
    for cmd in [
        "git switch -c feat && rm a",
        "git switch --create=feat && rm a",
        "git switch feat && rm a",
        "git checkout -b feat && rm a",
        "git checkout -B feat origin/main && rm a",
    ] {
        assert_eq!(branches(cmd), vec![switched(CWD, "feat")], "{cmd:?}");
    }
    assert_eq!(
        branches("git switch -c feat && git commit -m x && echo > f"),
        vec![switched(CWD, "feat"), switched(CWD, "feat")]
    );
    assert_eq!(
        branches("git -C /other switch -c feat && rm a"),
        vec![switched("/other", "feat")]
    );
}

#[test]
fn a_switch_that_may_have_failed_does_not_carry_over() {
    for cmd in [
        "git switch -c feat; rm a",
        "git switch -c feat || rm a",
        "git switch -c feat\nrm a",
        "git checkout feat && rm a",
        "git switch - && rm a",
        "git switch --detach && rm a",
        "bash -c 'git switch -c feat' && rm a",
    ] {
        assert_eq!(branches(cmd), vec![None], "{cmd:?}");
    }
    assert_eq!(
        branches("git switch -c feat && rm a; rm b"),
        vec![switched(CWD, "feat"), None]
    );
}

#[test]
fn a_later_switch_to_an_unnamed_branch_ends_the_carried_switch() {
    for cmd in [
        "git switch -c feat && git checkout main && rm a",
        "git checkout -b feat && git checkout main && rm a",
        "git switch -c feat && git switch - && rm a",
        "git switch -c feat && git switch \"$B\" && rm a",
    ] {
        assert_eq!(branches(cmd), vec![None], "{cmd:?}");
    }
    assert_eq!(
        branches("git switch -c feat && git checkout -- a && rm a"),
        vec![switched(CWD, "feat")]
    );
}
