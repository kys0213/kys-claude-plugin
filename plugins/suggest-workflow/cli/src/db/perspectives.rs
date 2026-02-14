use super::repository::{ParamDef, ParamType, PerspectiveInfo};

pub fn register_perspectives() -> Vec<PerspectiveInfo> {
    vec![
        // tool-frequency: Top N tools by usage count
        PerspectiveInfo {
            name: "tool-frequency".into(),
            description: "도구 사용 빈도 (분류명 기준)".into(),
            params: vec![ParamDef {
                name: "top".into(),
                param_type: ParamType::Integer,
                required: false,
                default: Some("10".into()),
                description: "상위 N개".into(),
            }],
            sql: "\
                SELECT classified_name AS tool, \
                       COUNT(*) AS frequency, \
                       COUNT(DISTINCT session_id) AS sessions \
                FROM tool_uses \
                GROUP BY classified_name \
                ORDER BY frequency DESC \
                LIMIT :top"
                .into(),
        },
        // transitions: Tools that follow a specific tool
        PerspectiveInfo {
            name: "transitions".into(),
            description: "특정 도구 이후 전이 확률".into(),
            params: vec![ParamDef {
                name: "tool".into(),
                param_type: ParamType::Text,
                required: true,
                default: None,
                description: "기준 도구 (예: Bash:git, Edit)".into(),
            }],
            sql: "\
                SELECT to_tool, count, probability \
                FROM tool_transitions \
                WHERE from_tool = :tool \
                ORDER BY probability DESC"
                .into(),
        },
        // trends: Weekly tool usage trends
        PerspectiveInfo {
            name: "trends".into(),
            description: "주간 도구 사용 트렌드".into(),
            params: vec![ParamDef {
                name: "since".into(),
                param_type: ParamType::Date,
                required: false,
                default: Some("2020-01-01".into()),
                description: "시작 날짜 (YYYY-MM-DD)".into(),
            }],
            sql: "\
                SELECT week_start, tool_name, count, session_count \
                FROM weekly_buckets \
                WHERE week_start >= :since \
                ORDER BY week_start, count DESC"
                .into(),
        },
        // hotfiles: Most frequently edited files
        PerspectiveInfo {
            name: "hotfiles".into(),
            description: "자주 편집되는 파일 핫스팟".into(),
            params: vec![ParamDef {
                name: "top".into(),
                param_type: ParamType::Integer,
                required: false,
                default: Some("20".into()),
                description: "상위 N개".into(),
            }],
            sql: "\
                SELECT file_path, edit_count, session_count \
                FROM file_hotspots \
                ORDER BY edit_count DESC \
                LIMIT :top"
                .into(),
        },
        // repetition: Anomaly detection via z-score on per-session tool counts
        PerspectiveInfo {
            name: "repetition".into(),
            description: "반복/이상치 탐지 (z-score 기반)".into(),
            params: vec![ParamDef {
                name: "z_threshold".into(),
                param_type: ParamType::Float,
                required: false,
                default: Some("2.0".into()),
                description: "z-score 임계값".into(),
            }],
            sql: "\
                SELECT session_id, classified_name AS tool, cnt, \
                       ROUND((cnt - avg_cnt) / CASE WHEN std_cnt < 0.001 THEN 1.0 ELSE std_cnt END, 2) AS z_score \
                FROM ( \
                    SELECT session_id, classified_name, \
                           COUNT(*) AS cnt, \
                           AVG(COUNT(*)) OVER (PARTITION BY classified_name) AS avg_cnt, \
                           SQRT(AVG(COUNT(*) * COUNT(*)) OVER (PARTITION BY classified_name) \
                                - AVG(COUNT(*)) OVER (PARTITION BY classified_name) \
                                * AVG(COUNT(*)) OVER (PARTITION BY classified_name)) AS std_cnt \
                    FROM tool_uses \
                    GROUP BY session_id, classified_name \
                ) sub \
                WHERE ABS((cnt - avg_cnt) / CASE WHEN std_cnt < 0.001 THEN 1.0 ELSE std_cnt END) >= :z_threshold \
                ORDER BY z_score DESC"
                .into(),
        },
        // prompts: Search prompts by keyword
        PerspectiveInfo {
            name: "prompts".into(),
            description: "프롬프트 키워드 검색".into(),
            params: vec![
                ParamDef {
                    name: "search".into(),
                    param_type: ParamType::Text,
                    required: true,
                    default: None,
                    description: "검색어".into(),
                },
                ParamDef {
                    name: "top".into(),
                    param_type: ParamType::Integer,
                    required: false,
                    default: Some("20".into()),
                    description: "상위 N개".into(),
                },
            ],
            sql: "\
                SELECT p.session_id, p.timestamp, p.char_count, \
                       SUBSTR(p.text, 1, 200) AS snippet \
                FROM prompts p \
                WHERE p.text LIKE '%' || :search || '%' \
                ORDER BY p.timestamp DESC \
                LIMIT :top"
                .into(),
        },
        // session-links: Sessions sharing edited files
        PerspectiveInfo {
            name: "session-links".into(),
            description: "파일 공유 기반 세션 연결".into(),
            params: vec![ParamDef {
                name: "min_overlap".into(),
                param_type: ParamType::Float,
                required: false,
                default: Some("0.3".into()),
                description: "최소 overlap 비율".into(),
            }],
            sql: "\
                SELECT session_a, session_b, shared_files, \
                       ROUND(overlap_ratio, 2) AS overlap_ratio, \
                       time_gap_minutes \
                FROM session_links \
                WHERE overlap_ratio >= :min_overlap \
                ORDER BY overlap_ratio DESC"
                .into(),
        },
        // sequences: Common tool sequences (bigrams)
        PerspectiveInfo {
            name: "sequences".into(),
            description: "자주 등장하는 도구 시퀀스 (2-gram)".into(),
            params: vec![ParamDef {
                name: "min_count".into(),
                param_type: ParamType::Integer,
                required: false,
                default: Some("3".into()),
                description: "최소 등장 횟수".into(),
            }],
            sql: "\
                SELECT from_tool || ' → ' || to_tool AS sequence, \
                       count, ROUND(probability, 3) AS probability \
                FROM tool_transitions \
                WHERE count >= :min_count \
                ORDER BY count DESC"
                .into(),
        },
        // sessions: Session overview
        PerspectiveInfo {
            name: "sessions".into(),
            description: "세션 목록 및 요약".into(),
            params: vec![ParamDef {
                name: "top".into(),
                param_type: ParamType::Integer,
                required: false,
                default: Some("20".into()),
                description: "상위 N개".into(),
            }],
            sql: "\
                SELECT id, prompt_count, tool_use_count, \
                       datetime(first_ts / 1000, 'unixepoch', 'localtime') AS started_at, \
                       datetime(last_ts / 1000, 'unixepoch', 'localtime') AS ended_at, \
                       ROUND((last_ts - first_ts) / 60000.0, 1) AS duration_minutes \
                FROM sessions \
                WHERE first_ts IS NOT NULL \
                ORDER BY first_ts DESC \
                LIMIT :top"
                .into(),
        },
    ]
}
