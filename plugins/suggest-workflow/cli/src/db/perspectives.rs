use super::repository::{ParamDef, ParamType, PerspectiveInfo};

pub fn register_perspectives() -> Vec<PerspectiveInfo> {
    vec![
        // tool-frequency: Top N tools by usage count
        // Supports --session-filter via {SF:session_id}
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
                WHERE 1=1 {SF:session_id} \
                GROUP BY classified_name \
                ORDER BY frequency DESC \
                LIMIT :top"
                .into(),
        },
        // transitions: Tools that follow a specific tool
        // Derived table — session filter not applicable
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
        // Derived table — session filter not applicable
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
        // Derived table — session filter not applicable
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
        // repetition: Anomaly detection via z-score² on per-session tool counts
        // Supports --session-filter via {SF:session_id}
        PerspectiveInfo {
            name: "repetition".into(),
            description: "반복/이상치 탐지 (z-score² 기반)".into(),
            params: vec![ParamDef {
                name: "z_threshold".into(),
                param_type: ParamType::Float,
                required: false,
                default: Some("2.0".into()),
                description: "z-score 임계값".into(),
            }],
            sql: "\
                SELECT session_id, classified_name AS tool, cnt, \
                       ROUND((cnt - avg_cnt) * ABS(cnt - avg_cnt) \
                             / CASE WHEN var_cnt < 0.001 THEN 1.0 ELSE var_cnt END, 2) AS deviation_score \
                FROM ( \
                    SELECT session_id, classified_name, \
                           COUNT(*) AS cnt, \
                           AVG(COUNT(*)) OVER (PARTITION BY classified_name) AS avg_cnt, \
                           AVG(COUNT(*) * COUNT(*)) OVER (PARTITION BY classified_name) \
                           - AVG(COUNT(*)) OVER (PARTITION BY classified_name) \
                           * AVG(COUNT(*)) OVER (PARTITION BY classified_name) AS var_cnt \
                    FROM tool_uses \
                    WHERE 1=1 {SF:session_id} \
                    GROUP BY session_id, classified_name \
                ) sub \
                WHERE (cnt - avg_cnt) * (cnt - avg_cnt) \
                      / CASE WHEN var_cnt < 0.001 THEN 1.0 ELSE var_cnt END \
                      >= :z_threshold * :z_threshold \
                ORDER BY ABS(deviation_score) DESC"
                .into(),
        },
        // prompts: Search prompts by keyword
        // Supports --session-filter via {SF:p.session_id}
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
                WHERE p.text LIKE '%' || :search || '%' {SF:p.session_id} \
                ORDER BY p.timestamp DESC \
                LIMIT :top"
                .into(),
        },
        // session-links: Sessions sharing edited files
        // Derived table — session filter not applicable
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
        // Derived table — session filter not applicable
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
        // Supports --session-filter via {SF:id}
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
                WHERE first_ts IS NOT NULL {SF:id} \
                ORDER BY first_ts DESC \
                LIMIT :top"
                .into(),
        },
        // filtered-sessions: Find sessions by first prompt pattern
        PerspectiveInfo {
            name: "filtered-sessions".into(),
            description: "첫 프롬프트 패턴으로 세션 검색".into(),
            params: vec![
                ParamDef {
                    name: "prompt_pattern".into(),
                    param_type: ParamType::Text,
                    required: true,
                    default: None,
                    description: "첫 프롬프트 검색 패턴".into(),
                },
                ParamDef {
                    name: "since".into(),
                    param_type: ParamType::Date,
                    required: false,
                    default: Some("2020-01-01".into()),
                    description: "시작 날짜 (YYYY-MM-DD)".into(),
                },
                ParamDef {
                    name: "top".into(),
                    param_type: ParamType::Integer,
                    required: false,
                    default: Some("50".into()),
                    description: "상위 N개".into(),
                },
            ],
            sql: "\
                SELECT id, prompt_count, tool_use_count, \
                       SUBSTR(first_prompt_snippet, 1, 100) AS first_prompt, \
                       datetime(first_ts / 1000, 'unixepoch', 'localtime') AS started_at, \
                       datetime(last_ts / 1000, 'unixepoch', 'localtime') AS ended_at, \
                       ROUND((last_ts - first_ts) / 60000.0, 1) AS duration_minutes \
                FROM sessions \
                WHERE first_prompt_snippet LIKE '%' || :prompt_pattern || '%' \
                  AND datetime(first_ts / 1000, 'unixepoch') >= :since \
                ORDER BY first_ts DESC \
                LIMIT :top"
                .into(),
        },
    ]
}
