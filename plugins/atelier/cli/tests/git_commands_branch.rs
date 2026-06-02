//! Port of `git-utils/tests/commands/branch.test.ts`.
#![allow(clippy::field_reassign_with_default)]

mod git_mocks;

use atelier::git::commands::branch::{run, BranchDeps};
use atelier::git::core::git::BranchLocation;
use atelier::git::types::{BranchInput, CmdResult};
use git_mocks::{MockGit, Recorder};
use std::rc::Rc;

fn input(name: &str) -> BranchInput {
    BranchInput {
        branch_name: name.to_string(),
        base_branch: None,
    }
}

#[test]
fn detects_default_branch_when_base_unspecified() {
    let detected = Rc::new(std::cell::RefCell::new(false));
    let d = detected.clone();
    let mut git = MockGit::default();
    git.detect_default_branch = Box::new(move || {
        *d.borrow_mut() = true;
        Ok("main".to_string())
    });
    git.branch_exists = Box::new(|name, loc| name == "main" && matches!(loc, BranchLocation::Any));
    let deps = BranchDeps { git: &git };
    let out = run(&deps, &input("feat/new-feature")).unwrap();
    assert!(*detected.borrow());
    match out {
        CmdResult::Ok(d) => assert_eq!(d.base_branch, "main"),
        _ => panic!("expected ok"),
    }
}

#[test]
fn uses_specified_base_branch() {
    let mut git = MockGit::default();
    git.branch_exists = Box::new(|name, _| name == "develop");
    let deps = BranchDeps { git: &git };
    let mut i = input("feat/new-feature");
    i.base_branch = Some("develop".to_string());
    match run(&deps, &i).unwrap() {
        CmdResult::Ok(d) => assert_eq!(d.base_branch, "develop"),
        _ => panic!("expected ok"),
    }
}

#[test]
fn output_has_branch_and_base() {
    let mut git = MockGit::default();
    git.branch_exists = Box::new(|name, _| name == "main");
    let deps = BranchDeps { git: &git };
    match run(&deps, &input("feat/login")).unwrap() {
        CmdResult::Ok(d) => {
            assert_eq!(d.branch_name, "feat/login");
            assert_eq!(d.base_branch, "main");
        }
        _ => panic!("expected ok"),
    }
}

#[test]
fn call_order_fetch_checkout_pull_create() {
    let rec = Rc::new(Recorder::default());
    let r1 = rec.clone();
    let r2 = rec.clone();
    let r3 = rec.clone();
    let mut git = MockGit::default();
    git.branch_exists = Box::new(|name, loc| {
        name == "main" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
    });
    git.fetch = Box::new(move || {
        r1.push("fetch");
        Ok(())
    });
    git.checkout = Box::new(move |branch, opts| {
        if opts.map(|o| o.create).unwrap_or(false) {
            r2.push(format!("checkout-create:{branch}"));
        } else {
            r2.push(format!("checkout:{branch}"));
        }
        Ok(())
    });
    git.pull = Box::new(move |branch| {
        r3.push(format!("pull:{branch}"));
        Ok(())
    });
    let deps = BranchDeps { git: &git };
    run(&deps, &input("feat/new")).unwrap();
    assert_eq!(
        rec.snapshot(),
        vec![
            "fetch",
            "checkout:main",
            "pull:main",
            "checkout-create:feat/new"
        ]
    );
}

#[test]
fn base_remote_only_creates_tracking_branch() {
    let rec = Rc::new(Recorder::default());
    let r = rec.clone();
    let mut git = MockGit::default();
    git.branch_exists = Box::new(|name, loc| name == "main" && matches!(loc, BranchLocation::Any));
    git.checkout = Box::new(move |branch, opts| {
        if branch == "main" {
            let create = opts.map(|o| o.create).unwrap_or(false);
            let track = opts.and_then(|o| o.track.clone());
            r.push(format!("main:create={create}:track={track:?}"));
        }
        Ok(())
    });
    let deps = BranchDeps { git: &git };
    run(&deps, &input("feat/new")).unwrap();
    let snap = rec.snapshot();
    assert!(snap
        .iter()
        .any(|c| c.contains("main:create=true:track=Some(\"origin/main\")")));
}

#[test]
fn uncommitted_changes_fails() {
    let mut git = MockGit::default();
    git.has_uncommitted_changes = Box::new(|| true);
    let deps = BranchDeps { git: &git };
    match run(&deps, &input("feat/new")).unwrap() {
        CmdResult::Err(e) => assert!(e.contains("Uncommitted changes")),
        _ => panic!("expected err"),
    }
}

#[test]
fn base_missing_fails() {
    let mut git = MockGit::default();
    git.branch_exists = Box::new(|_, _| false);
    let deps = BranchDeps { git: &git };
    match run(&deps, &input("feat/new")).unwrap() {
        CmdResult::Err(e) => assert!(e.contains("does not exist")),
        _ => panic!("expected err"),
    }
}

#[test]
fn existing_target_fails() {
    let mut git = MockGit::default();
    git.branch_exists = Box::new(|name, loc| {
        (name == "main" && matches!(loc, BranchLocation::Any))
            || (name == "feat/existing" && matches!(loc, BranchLocation::Local))
    });
    let deps = BranchDeps { git: &git };
    match run(&deps, &input("feat/existing")).unwrap() {
        CmdResult::Err(e) => assert!(e.contains("already exists")),
        _ => panic!("expected err"),
    }
}

#[test]
fn fetch_failure_is_ignored() {
    let mut git = MockGit::default();
    git.fetch = Box::new(|| Err("network error".to_string()));
    git.branch_exists = Box::new(|name, loc| {
        name == "main" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
    });
    let deps = BranchDeps { git: &git };
    assert!(run(&deps, &input("feat/new")).unwrap().is_ok());
}

#[test]
fn checkout_create_failure_returns_handled_error() {
    let mut git = MockGit::default();
    git.branch_exists = Box::new(|name, loc| {
        name == "main" && matches!(loc, BranchLocation::Any | BranchLocation::Local)
    });
    git.checkout = Box::new(|_, opts| {
        if opts.map(|o| o.create).unwrap_or(false) {
            Err("checkout failed".to_string())
        } else {
            Ok(())
        }
    });
    let deps = BranchDeps { git: &git };
    match run(&deps, &input("feat/new")).unwrap() {
        CmdResult::Err(e) => assert_eq!(e, "checkout failed"),
        _ => panic!("expected err"),
    }
}

#[test]
fn empty_branch_name_fails() {
    let git = MockGit::default();
    let deps = BranchDeps { git: &git };
    match run(&deps, &input("")).unwrap() {
        CmdResult::Err(e) => assert!(e.contains("Branch name is required")),
        _ => panic!("expected err"),
    }
}
