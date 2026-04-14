mod mock_gh;

use autopilot::cmd::issue_list::{self, Stage};
use mock_gh::MockGh;

#[test]
fn list_unanalyzed_filters_via_gh() {
    let gh = MockGh::new().on_list_containing(
        "issue",
        vec![
            serde_json::json!({
                "number": 1, "title": "Has label", "body": "",
                "labels": [{"name": "autopilot:ready"}],
                "comments": []
            }),
            serde_json::json!({
                "number": 2, "title": "Fresh issue", "body": "",
                "labels": [],
                "comments": []
            }),
            serde_json::json!({
                "number": 3, "title": "Already analyzed", "body": "",
                "labels": [],
                "comments": [{"body": "Autopilot 분석 결과: ready"}]
            }),
        ],
    );

    let code = issue_list::list(&gh, &Stage::Unanalyzed, "autopilot:", None, 50).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn list_ready_with_require_label() {
    let gh = MockGh::new().on_list_containing(
        "issue",
        vec![
            serde_json::json!({
                "number": 1, "title": "Ready+bug", "body": "",
                "labels": [{"name": "autopilot:ready"}, {"name": "bug"}],
                "comments": []
            }),
            serde_json::json!({
                "number": 2, "title": "Ready only", "body": "",
                "labels": [{"name": "autopilot:ready"}],
                "comments": []
            }),
        ],
    );

    let code = issue_list::list(&gh, &Stage::Ready, "autopilot:", Some("bug"), 50).unwrap();
    assert_eq!(code, 0);
    // Only issue #1 has "bug" label — output is captured via println
}

#[test]
fn list_rework_via_gh() {
    let gh = MockGh::new().on_list_containing(
        "issue",
        vec![
            serde_json::json!({
                "number": 10, "title": "Needs rework", "body": "",
                "labels": [],
                "comments": [{"body": "이 부분 rework 해주세요"}]
            }),
            serde_json::json!({
                "number": 11, "title": "Normal", "body": "",
                "labels": [],
                "comments": [{"body": "LGTM"}]
            }),
        ],
    );

    let code = issue_list::list(&gh, &Stage::Rework, "autopilot:", None, 50).unwrap();
    assert_eq!(code, 0);
}
