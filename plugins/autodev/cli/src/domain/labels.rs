// ─── GitHub Label 상수 ───

pub const ANALYZE: &str = "autodev:analyze"; // 트리거 (사람이 추가)
pub const WIP: &str = "autodev:wip";
pub const DONE: &str = "autodev:done";
pub const SKIP: &str = "autodev:skip";

// v2: 분석 리뷰 게이트 + Issue-PR 연동
pub const ANALYZED: &str = "autodev:analyzed";
pub const APPROVED_ANALYSIS: &str = "autodev:approved-analysis";
pub const IMPLEMENTING: &str = "autodev:implementing";

// v2.1: PR 전용 라벨
pub const CHANGES_REQUESTED: &str = "autodev:changes-requested";

// v2: 리뷰 반복 횟수 라벨 (예: "autodev:iteration/1")
pub const ITERATION_PREFIX: &str = "autodev:iteration/";

/// "autodev:iteration/{n}" 라벨 생성
pub fn iteration_label(n: u32) -> String {
    format!("{ITERATION_PREFIX}{n}")
}

/// 라벨 목록에서 "autodev:iteration/{n}" 파싱. 없으면 0 반환.
pub fn parse_iteration(label_names: &[&str]) -> u32 {
    label_names
        .iter()
        .find_map(|l| l.strip_prefix(ITERATION_PREFIX)?.parse::<u32>().ok())
        .unwrap_or(0)
}
