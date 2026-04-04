mod mock_gh;

use mock_gh::MockGh;
use serde_json::json;
use std::sync::Arc;

#[test]
fn idle_returns_0_when_no_active_items() {
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing("autopilot:ready", vec![])
            .on_list_containing("autopilot:wip", vec![])
            .on_list_containing("autopilot:auto", vec![]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn idle_returns_1_when_ready_issues_exist() {
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing(
                "autopilot:ready",
                vec![json!({"number": 1}), json!({"number": 2})],
            )
            .on_list_containing("autopilot:wip", vec![])
            .on_list_containing("autopilot:auto", vec![]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:").unwrap();
    assert_eq!(code, 1);
}

#[test]
fn idle_returns_1_when_only_prs_exist() {
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing("autopilot:ready", vec![])
            .on_list_containing("autopilot:wip", vec![])
            .on_list_containing("autopilot:auto", vec![json!({"number": 5})]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:").unwrap();
    assert_eq!(code, 1);
}
