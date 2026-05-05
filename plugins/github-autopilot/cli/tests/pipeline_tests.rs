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

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", None).unwrap();
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

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", None).unwrap();
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

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", None).unwrap();
    assert_eq!(code, 1);
}

// --- Capacity check (`--max-parallel`) ----------------------------------
// Exit codes:
//   0 = idle (ready + wip + prs == 0)
//   3 = at-capacity (wip >= max_parallel) — only when --max-parallel is set
//   1 = active and has capacity (or no --max-parallel given)

#[test]
fn idle_returns_3_when_wip_at_capacity() {
    // wip == max_parallel, ready > 0 → at-capacity (skip new dispatch)
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing(
                "autopilot:ready",
                vec![json!({"number": 10}), json!({"number": 11})],
            )
            .on_list_containing(
                "autopilot:wip",
                vec![
                    json!({"number": 1}),
                    json!({"number": 2}),
                    json!({"number": 3}),
                ],
            )
            .on_list_containing("autopilot:auto", vec![]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", Some(3)).unwrap();
    assert_eq!(code, 3);
}

#[test]
fn idle_returns_3_when_wip_exceeds_capacity() {
    // wip > max_parallel — still at-capacity
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing("autopilot:ready", vec![])
            .on_list_containing(
                "autopilot:wip",
                vec![
                    json!({"number": 1}),
                    json!({"number": 2}),
                    json!({"number": 3}),
                    json!({"number": 4}),
                ],
            )
            .on_list_containing("autopilot:auto", vec![]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", Some(3)).unwrap();
    assert_eq!(code, 3);
}

#[test]
fn idle_returns_1_when_wip_below_capacity_with_ready() {
    // wip=2 < max_parallel=3, ready>0 → active, has capacity
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing("autopilot:ready", vec![json!({"number": 10})])
            .on_list_containing(
                "autopilot:wip",
                vec![json!({"number": 1}), json!({"number": 2})],
            )
            .on_list_containing("autopilot:auto", vec![]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", Some(3)).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn idle_returns_0_when_fully_idle_regardless_of_capacity() {
    // wip=0, ready=0, prs=0 → idle wins even with --max-parallel set
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing("autopilot:ready", vec![])
            .on_list_containing("autopilot:wip", vec![])
            .on_list_containing("autopilot:auto", vec![]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", Some(3)).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn idle_without_max_parallel_treats_high_wip_as_active() {
    // No --max-parallel → backward compat: any wip is just "active" (exit 1).
    let gh = Arc::new(
        MockGh::new()
            .on_list_containing("autopilot:ready", vec![])
            .on_list_containing(
                "autopilot:wip",
                vec![
                    json!({"number": 1}),
                    json!({"number": 2}),
                    json!({"number": 3}),
                    json!({"number": 4}),
                    json!({"number": 5}),
                    json!({"number": 6}),
                    json!({"number": 7}),
                    json!({"number": 8}),
                    json!({"number": 9}),
                    json!({"number": 10}),
                ],
            )
            .on_list_containing("autopilot:auto", vec![]),
    );

    let code = autopilot::cmd::pipeline::idle(gh, "autopilot:", None).unwrap();
    assert_eq!(code, 1);
}
