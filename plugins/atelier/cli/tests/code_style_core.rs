use atelier::code_style::core::record::InstallRecord;
use atelier::code_style::core::types::Group;

fn lang(name: &str) -> Group {
    Group::Lang(name.to_string())
}

fn app(name: &str) -> Group {
    Group::App(name.to_string())
}

#[test]
fn groups_sort_base_then_languages_then_apps() {
    let mut groups = vec![app("cli"), lang("rust"), Group::Base, lang("go")];
    groups.sort();
    assert_eq!(
        groups,
        vec![Group::Base, lang("go"), lang("rust"), app("cli")]
    );
}

#[test]
fn record_round_trips_sorted_and_deduplicated() {
    let mut record = InstallRecord::default();
    record.add(&lang("rust"));
    record.add(&lang("go"));
    record.add(&app("cli"));
    record.set_rules(&lang("go"), ["b.md".to_string(), "a.md".to_string()]);
    record.set_rules(&Group::Base, ["c.md".to_string()]);

    let json = record.render();

    assert_eq!(
        json,
        "{\n  \"langs\": [\n    \"go\",\n    \"rust\"\n  ],\n  \"apps\": [\n    \"cli\"\n  ],\n  \
         \"rules\": [\n    \"base/c.md\",\n    \"go/a.md\",\n    \"go/b.md\"\n  ]\n}\n"
    );
    assert_eq!(InstallRecord::parse(&json), Ok(record));
}

#[test]
fn set_rules_replaces_only_that_group() {
    let mut record = InstallRecord::default();
    record.set_rules(&lang("go"), ["a.md".to_string()]);
    record.set_rules(&Group::Base, ["c.md".to_string()]);

    record.set_rules(&lang("go"), ["b.md".to_string()]);

    assert!(record.has_rule(&lang("go"), "b.md"));
    assert!(!record.has_rule(&lang("go"), "a.md"));
    assert!(record.has_rule(&Group::Base, "c.md"));
}

#[test]
fn remove_drops_the_group_and_its_rules() {
    let mut record = InstallRecord::default();
    record.add(&app("cli"));
    record.set_rules(&app("cli"), ["a.md".to_string()]);

    record.remove(&app("cli"));

    assert!(record.apps().is_empty());
    assert!(!record.has_rule(&app("cli"), "a.md"));
}

#[test]
fn installed_groups_list_languages_then_apps() {
    let mut record = InstallRecord::default();
    record.add(&app("cli"));
    record.add(&lang("go"));

    assert_eq!(record.groups(), vec![lang("go"), app("cli")]);
}

#[test]
fn record_with_unknown_field_is_an_error() {
    assert!(
        InstallRecord::parse("{\"langs\": [], \"apps\": [], \"rules\": [], \"extra\": 1}").is_err()
    );
}

#[test]
fn record_without_apps_is_an_error() {
    assert!(InstallRecord::parse("{\"langs\": [], \"rules\": []}").is_err());
}
