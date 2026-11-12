use serde_json::{json, Value};

mod kanban_support;
use kanban_support::{atelier, stdout_json};

fn prune(stdin: &str) -> Value {
    stdout_json(
        atelier()
            .args(["kanban", "prune-nonces"])
            .write_stdin(stdin),
    )
}

fn keys(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("kanban:nonce:s-1:answer:n-{i}"))
        .collect()
}

#[test]
fn only_the_newest_32_nonces_are_kept_and_the_older_ones_dropped() {
    let left = keys(34);

    let out = prune(&json!({ "keys": left }).to_string());

    assert_eq!(out["drop"], json!(left[..2]));
}

#[test]
fn up_to_32_nonces_are_all_kept() {
    for count in [0, 1, 32] {
        let out = prune(&json!({ "keys": keys(count) }).to_string());

        assert_eq!(out["drop"], json!([]), "{count}");
    }
}

#[test]
fn keys_that_are_not_nonces_are_never_dropped() {
    let others = ["kanban:session:s-1", "kanban:answer:s-1:tu-1", "unrelated"];
    let mut store: Vec<String> = others.iter().map(|k| k.to_string()).collect();
    store.extend(keys(34));

    let out = prune(&json!({ "keys": store }).to_string());

    assert_eq!(out["drop"], json!(keys(34)[..2]));
}

#[test]
fn input_without_a_list_of_keys_is_refused() {
    for stdin in ["not json", "{}", "{\"keys\": [1]}"] {
        assert!(prune(stdin)["error"].is_string(), "{stdin}");
    }
}
