use atelier::code_style::core::profile::Profile;
use atelier::code_style::core::record::InstallRecord;
use atelier::code_style::core::render::render_common;
use atelier::code_style::core::role::Role;
use std::collections::BTreeMap;

fn profile(source: &[&str], test: &[&str]) -> Profile {
    let list = |globs: &[&str]| {
        globs
            .iter()
            .map(|g| format!("\"{g}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Profile::parse(&format!(
        "{{\"roles\": {{\"source\": [{}], \"config\": [], \"boundary\": [], \"test\": [{}]}}}}",
        list(source),
        list(test)
    ))
    .unwrap()
}

fn profiles(entries: &[(&str, Profile)]) -> BTreeMap<String, Profile> {
    entries
        .iter()
        .map(|(lang, p)| (lang.to_string(), p.clone()))
        .collect()
}

#[test]
fn role_parses_every_known_name() {
    for (name, role) in [
        ("source", Role::Source),
        ("config", Role::Config),
        ("boundary", Role::Boundary),
        ("test", Role::Test),
    ] {
        assert_eq!(Role::parse(name), Ok(role));
        assert_eq!(role.as_str(), name);
    }
}

#[test]
fn unknown_role_is_an_error() {
    assert_eq!(
        Role::parse("docs"),
        Err("unknown role: docs — expected one of: source, config, boundary, test".to_string())
    );
}

#[test]
fn profile_missing_a_role_is_an_error() {
    let err = Profile::parse("{\"roles\": {\"source\": [], \"config\": [], \"boundary\": []}}")
        .unwrap_err();
    assert!(err.contains("test"), "{err}");
}

#[test]
fn profile_with_unknown_role_is_an_error() {
    let err = Profile::parse(
        "{\"roles\": {\"source\": [], \"config\": [], \"boundary\": [], \"test\": [], \"docs\": []}}",
    )
    .unwrap_err();
    assert!(err.contains("docs"), "{err}");
}

#[test]
fn render_replaces_roles_with_paths_in_language_then_role_then_profile_order() {
    let source = "---\nname: x\nroles: [test, source]\ndescription: d\n---\n\n# body\n";
    let langs = profiles(&[
        ("rust", profile(&["**/*.rs"], &["**/tests/**/*.rs"])),
        ("go", profile(&["**/*.go"], &["**/*_test.go", "**/*.go"])),
    ]);

    let out = render_common(source, &langs).unwrap();

    assert_eq!(
        out,
        "---\nname: x\npaths:\n  - \"**/*_test.go\"\n  - \"**/*.go\"\n  \
         - \"**/tests/**/*.rs\"\n  - \"**/*.rs\"\ndescription: d\n---\n\n# body\n"
    );
}

#[test]
fn render_ignores_a_roles_line_outside_the_frontmatter() {
    let source = "---\nroles: [source]\n---\nroles: [test]\n";
    let langs = profiles(&[("go", profile(&["**/*.go"], &["**/*_test.go"]))]);

    let out = render_common(source, &langs).unwrap();

    assert_eq!(out, "---\npaths:\n  - \"**/*.go\"\n---\nroles: [test]\n");
}

#[test]
fn render_with_no_matching_path_is_an_error() {
    let source = "---\nroles: [test]\n---\n";
    let langs = profiles(&[("go", profile(&["**/*.go"], &[]))]);

    assert_eq!(
        render_common(source, &langs),
        Err("roles [test] match no path for languages: go".to_string())
    );
}

#[test]
fn render_without_frontmatter_is_an_error() {
    let langs = profiles(&[("go", profile(&["**/*.go"], &[]))]);
    assert_eq!(
        render_common("# body\n", &langs),
        Err("a common rule needs a frontmatter with roles".to_string())
    );
}

#[test]
fn render_without_roles_is_an_error() {
    let langs = profiles(&[("go", profile(&["**/*.go"], &[]))]);
    assert_eq!(
        render_common("---\nname: x\n---\n", &langs),
        Err("a common rule needs a frontmatter with roles".to_string())
    );
}

#[test]
fn render_with_empty_roles_is_an_error() {
    let langs = profiles(&[("go", profile(&["**/*.go"], &[]))]);
    assert_eq!(
        render_common("---\nroles: []\n---\n", &langs),
        Err("roles must list at least one role, e.g. roles: [source]".to_string())
    );
}

#[test]
fn record_round_trips_sorted_and_deduplicated() {
    let mut record = InstallRecord::default();
    record.add_lang("rust");
    record.add_lang("go");
    record.set_group_rules("go", ["b.md".to_string(), "a.md".to_string()]);
    record.set_group_rules("common", ["c.md".to_string()]);

    let json = record.render();

    assert_eq!(
        json,
        "{\n  \"langs\": [\n    \"go\",\n    \"rust\"\n  ],\n  \"rules\": [\n    \
         \"common/c.md\",\n    \"go/a.md\",\n    \"go/b.md\"\n  ]\n}\n"
    );
    assert_eq!(InstallRecord::parse(&json), Ok(record));
}

#[test]
fn record_set_group_rules_replaces_only_that_group() {
    let mut record = InstallRecord::default();
    record.set_group_rules("go", ["a.md".to_string()]);
    record.set_group_rules("common", ["c.md".to_string()]);

    record.set_group_rules("go", ["b.md".to_string()]);

    assert!(record.has_rule("go", "b.md"));
    assert!(!record.has_rule("go", "a.md"));
    assert!(record.has_rule("common", "c.md"));
}

#[test]
fn record_remove_lang_drops_its_rules() {
    let mut record = InstallRecord::default();
    record.add_lang("go");
    record.set_group_rules("go", ["a.md".to_string()]);

    record.remove_lang("go");

    assert!(record.langs().is_empty());
    assert!(!record.has_rule("go", "a.md"));
}

#[test]
fn record_with_unknown_field_is_an_error() {
    assert!(InstallRecord::parse("{\"langs\": [], \"rules\": [], \"extra\": 1}").is_err());
}
