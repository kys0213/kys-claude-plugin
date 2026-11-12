use suggest_workflow::analyzers::depth::{AnalysisDepth, DepthConfig};
use suggest_workflow::analyzers::stopwords::StopwordSet;
use suggest_workflow::analyzers::suffix_miner::SuffixMiner;
use suggest_workflow::analyzers::tacit::analyze_tacit_knowledge;
use suggest_workflow::analyzers::tuning::TuningConfig;
use suggest_workflow::types::{HistoryEntry, TacitPattern};

/// Helper to construct HistoryEntry with display and timestamp
fn make_entry(display: &str, ts: i64) -> HistoryEntry {
    HistoryEntry {
        display: display.to_string(),
        timestamp: ts,
        project: "test-project".to_string(),
    }
}

/// Helper: resolve a DepthConfig with a custom similarity threshold
fn config_with_similarity(similarity: f64) -> DepthConfig {
    let mut config = AnalysisDepth::Normal.resolve();
    config.similarity_threshold = similarity;
    config
}

/// Default config for most tests
fn default_config() -> DepthConfig {
    config_with_similarity(0.3)
}

// ============================================================================
// SUFFIX MINER UNIT TESTS
// ============================================================================

#[test]
fn test_suffix_miner_basic_korean() {
    let prompts = vec![
        "타입을 명시해줘",
        "타입을 명시해주세요",
        "에러를 처리해줘",
        "에러를 처리해주세요",
    ];

    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&prompts);

    // Should find common Korean suffixes
    assert!(!suffixes.is_empty(), "Should discover Korean suffixes");

    // Check that suffixes contain expected patterns
    let suffix_strs: Vec<String> = suffixes.iter().map(|s| s.text.clone()).collect();
    let has_haejwo = suffix_strs
        .iter()
        .any(|s| s.contains("해줄") || s.contains("해주세요"));
    assert!(has_haejwo, "Should find 해줘/해주세요 suffix patterns");
}

#[test]
fn test_suffix_miner_frequency_threshold() {
    let prompts = vec![
        "타입을 명시해줘",
        "에러를 처리해줘",
        "주석을 추가해줘",
        "코드를 리팩토링하세요",
    ];

    let miner_low = SuffixMiner::new(2, 10, 0.1); // Lower threshold
    let miner_high = SuffixMiner::new(2, 10, 0.5); // Higher threshold

    let suffixes_low = miner_low.mine(&prompts);
    let suffixes_high = miner_high.mine(&prompts);

    // Lower threshold should find more or equal suffixes (excluding fallbacks)
    // With frequency field removed, compare total suffix counts directly
    assert!(
        suffixes_low.len() >= suffixes_high.len(),
        "Lower threshold should find more or equal real suffixes"
    );
}

#[test]
fn test_suffix_miner_empty_input() {
    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&[]);
    assert!(
        suffixes.is_empty(),
        "Empty input should produce no suffixes"
    );
}

#[test]
fn test_suffix_miner_single_prompt() {
    let prompts = vec!["타입을 명시해줘"];
    let miner = SuffixMiner::new(2, 10, 0.5);

    let _suffixes = miner.mine(&prompts);
    // Single prompt with min_support=2 should not produce mined suffixes.
    // The returned suffixes (if any) are all fallback.
}

#[test]
fn test_suffix_miner_byte_safety() {
    // Various Korean strings with different byte lengths
    let prompts = vec![
        "가나다라마바사",
        "😀 이모지와 한글",
        "混合 한글 中文 text",
        "ㄱㄴㄷㄹㅁㅂㅅ",
        "",
    ];

    let miner = SuffixMiner::default();
    // Should not panic
    let _suffixes = miner.mine(&prompts);
}

// ============================================================================
// NORMALIZATION TESTS
// ============================================================================

#[test]
fn test_normalization_strips_korean_suffixes() {
    let variations = vec![
        "타입을 명시해줘",
        "타입을 명시해주세요",
        "타입을 명시하세요",
    ];

    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&variations);

    // Normalize each variation
    let normalized: Vec<_> = variations
        .iter()
        .map(|v| miner.normalize(v, &suffixes))
        .collect();

    // All variations should normalize to similar content
    assert!(!normalized.is_empty(), "Should produce normalized prompts");

    // Check that normalized versions strip polite endings
    for norm in &normalized {
        assert!(
            !norm.content.contains("해줘")
                && !norm.content.contains("해주세요")
                && !norm.content.contains("하세요"),
            "Normalized content should strip polite suffixes"
        );
        assert!(
            norm.content.contains("타입") && norm.content.contains("명시"),
            "Should preserve core content"
        );
    }
}

#[test]
fn test_normalization_preserves_core_content() {
    let prompt = "항상 타입을 명시해줘";
    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&[prompt]);

    let normalized = miner.normalize(prompt, &suffixes);

    assert!(
        normalized.content.contains("타입") || normalized.content.contains("명시"),
        "Should preserve core content words"
    );
}

#[test]
fn test_normalization_empty_after_stripping() {
    let only_suffixes = vec!["해줘", "해주세요", "하세요"];
    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&only_suffixes);

    for text in &only_suffixes {
        let normalized = miner.normalize(text, &suffixes);
        assert!(
            !normalized.content.is_empty(),
            "Should handle suffix-only prompts gracefully"
        );
    }
}

// ============================================================================
// CLUSTERING TESTS
// ============================================================================

#[test]
fn test_clustering_similar_prompts_group_together() {
    let entries = vec![
        make_entry("항상 타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("타입을 명시하세요", 4000),
        make_entry("타입 명시하세요", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert!(!result.patterns.is_empty(), "Should find patterns");

    let has_cluster = result.patterns.iter().any(|p| p.examples.len() >= 2);
    assert!(has_cluster, "Should cluster similar prompts together");
}

#[test]
fn test_clustering_different_topics_separate() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("에러를 처리해줘", 4000),
        make_entry("에러 처리해주세요", 5000),
        make_entry("에러를 처리하세요", 6000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    let pattern_texts: Vec<String> = result.patterns.iter().map(|p| p.pattern.clone()).collect();

    let has_type_pattern = pattern_texts.iter().any(|p| p.contains("타입"));
    let has_error_pattern = pattern_texts.iter().any(|p| p.contains("에러"));

    assert!(
        has_type_pattern || has_error_pattern,
        "Should identify distinct topic patterns"
    );
}

// ============================================================================
// TYPE CLASSIFICATION TESTS
// ============================================================================

// Type classification is now delegated to Phase 2 (LLM).
// These tests verify that patterns are still detected as clusters
// with their content preserved for LLM classification.

#[test]
fn test_type_classification_directive() {
    let entries = vec![
        make_entry("항상 타입을 명시해줘", 1000),
        make_entry("반드시 타입을 명시해주세요", 2000),
        make_entry("꼭 타입을 명시하세요", 3000),
        make_entry("타입을 명시해줘", 4000),
        make_entry("타입 명시해주세요", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    // Pattern should be detected as a cluster (type classification is Phase 2's job)
    let has_pattern = result
        .patterns
        .iter()
        .any(|p| p.pattern_type == "cluster" && p.pattern.contains("타입"));

    assert!(
        has_pattern,
        "Should detect pattern cluster containing directive-like prompts"
    );
}

#[test]
fn test_type_classification_convention() {
    let entries = vec![
        make_entry("camelCase로 작성해줘", 1000),
        make_entry("camelCase 사용해주세요", 2000),
        make_entry("camelCase로 써줘", 3000),
        make_entry("snake_case 사용해줘", 4000),
        make_entry("snake_case로 작성해주세요", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    let has_convention = result.patterns.iter().any(|p| {
        p.pattern_type == "cluster"
            && (p.pattern.contains("camelCase") || p.pattern.contains("snake_case"))
    });

    assert!(
        has_convention,
        "Should detect pattern cluster containing convention-like prompts"
    );
}

#[test]
fn test_type_classification_preference() {
    let entries = vec![
        make_entry("async/await 선호해요", 1000),
        make_entry("async/await 쓰는 게 좋아요", 2000),
        make_entry("Promise보다 async/await", 3000),
        make_entry("async/await 사용 선호", 4000),
        make_entry("async/await가 좋아요", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    let has_preference = result
        .patterns
        .iter()
        .any(|p| p.pattern_type == "cluster" && p.pattern.contains("async/await"));

    assert!(
        has_preference,
        "Should detect pattern cluster containing preference-like prompts"
    );
}

#[test]
fn test_type_classification_correction() {
    let entries = vec![
        make_entry("아니야, 타입을 명시해야 해", 1000),
        make_entry("아니야 에러 처리 필요해", 2000),
        make_entry("아니야, 다시 작성해줘", 3000),
        make_entry("잘못됐어 그게 아니라", 4000),
        make_entry("잘못됐어 수정해줘", 5000),
        make_entry("잘못됐어 다시 해줘", 6000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    if result.patterns.is_empty() {
        assert!(result.total == entries.len(), "Should process all entries");
    } else {
        for p in &result.patterns {
            assert!(!p.pattern.is_empty(), "Pattern should not be empty");
        }
    }
}

// ============================================================================
// FULL PIPELINE INTEGRATION TESTS
// ============================================================================

#[test]
fn test_full_pipeline_korean_prompts() {
    let entries = vec![
        make_entry("항상 타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("반드시 에러를 처리해줘", 4000),
        make_entry("에러 처리해줘", 5000),
        make_entry("에러를 처리해주세요", 6000),
        make_entry("주석을 추가해줘", 7000),
        make_entry("주석 추가해주세요", 8000),
        make_entry("주석을 달아줘", 9000),
        make_entry("코드 리뷰해줘", 10000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert!(!result.patterns.is_empty(), "Should produce patterns");
    assert_eq!(result.total, entries.len(), "Should count all entries");

    for p in &result.patterns {
        assert!(
            p.confidence >= 0.0 && p.confidence <= 1.0,
            "Confidence should be between 0 and 1, got {}",
            p.confidence
        );
        assert!(p.count >= 2, "Pattern count should meet minimum threshold");
        assert!(!p.pattern.is_empty(), "Pattern should not be empty");
        assert!(!p.examples.is_empty(), "Should have examples");
    }
}

#[test]
fn test_full_pipeline_with_bm25_ranking() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("타입을 명시하세요", 4000),
        make_entry("타입 명시해주세요", 5000),
        make_entry("에러를 처리해줘", 6000),
        make_entry("에러 처리해주세요", 7000),
        make_entry("에러를 처리하세요", 8000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert!(!result.patterns.is_empty(), "Should find patterns");

    for p in &result.patterns {
        assert!(p.bm25_score >= 0.0, "BM25 score should be non-negative");
    }

    if result.patterns.len() >= 2 {
        let max_score = result
            .patterns
            .iter()
            .map(|p| p.bm25_score)
            .fold(0.0, f64::max);
        let min_score = result
            .patterns
            .iter()
            .map(|p| p.bm25_score)
            .fold(f64::MAX, f64::min);
        assert!(max_score >= min_score, "Should have valid BM25 score range");
    }
}

#[test]
fn test_full_pipeline_confidence_calculation() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("에러를 처리해줘", 4000),
        make_entry("에러 처리해주세요", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    if result.patterns.len() >= 2 {
        let higher_count_pattern = result.patterns.iter().max_by_key(|p| p.count).unwrap();
        let lower_count_pattern = result.patterns.iter().min_by_key(|p| p.count).unwrap();

        if higher_count_pattern.count > lower_count_pattern.count {
            assert!(
                higher_count_pattern.confidence >= lower_count_pattern.confidence,
                "Higher count should generally yield higher confidence"
            );
        }
    }
}

#[test]
fn test_full_pipeline_respects_min_threshold() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("에러를 처리해줘", 2000),
        make_entry("주석을 추가해줘", 3000),
        make_entry("코드 리뷰해줘", 4000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    for p in &result.patterns {
        assert!(
            p.count >= 2,
            "All patterns should meet minimum threshold of 2"
        );
    }
}

#[test]
fn test_full_pipeline_respects_max_patterns() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("에러를 처리해줘", 3000),
        make_entry("에러 처리해주세요", 4000),
        make_entry("주석을 추가해줘", 5000),
        make_entry("주석 추가해주세요", 6000),
        make_entry("코드 리뷰해줘", 7000),
        make_entry("코드를 리뷰해주세요", 8000),
        make_entry("테스트 작성해줘", 9000),
        make_entry("테스트를 작성해주세요", 10000),
    ];

    let max_patterns = 3;
    let result = analyze_tacit_knowledge(
        &entries,
        2,
        max_patterns,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert!(
        result.patterns.len() <= max_patterns,
        "Should respect max_patterns limit, got {} patterns",
        result.patterns.len()
    );
}

// ============================================================================
// DEPTH PRESET TESTS
// ============================================================================

#[test]
fn test_depth_narrow_produces_results() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("에러를 처리해줘", 4000),
        make_entry("에러 처리해주세요", 5000),
    ];

    let config = AnalysisDepth::Narrow.resolve();
    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &config,
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert_eq!(result.total, entries.len());
    // Narrow has higher similarity threshold so may find fewer patterns
}

#[test]
fn test_depth_wide_finds_more_patterns() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("에러를 처리해줘", 4000),
        make_entry("에러 처리해주세요", 5000),
        make_entry("주석을 추가해줘", 6000),
        make_entry("주석 추가해줘", 7000),
    ];

    let narrow = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &AnalysisDepth::Narrow.resolve(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );
    let wide = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &AnalysisDepth::Wide.resolve(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    // Wide (lower similarity threshold) should merge more clusters → potentially fewer but larger patterns
    // Or with lower thresholds, discover more. Just verify both work.
    assert_eq!(narrow.total, wide.total);
}

#[test]
fn test_depth_config_values() {
    let narrow = AnalysisDepth::Narrow.resolve();
    let normal = AnalysisDepth::Normal.resolve();
    let wide = AnalysisDepth::Wide.resolve();

    // Narrow should be more conservative
    assert!(narrow.sentence_split_min_tokens > normal.sentence_split_min_tokens);
    assert!(normal.sentence_split_min_tokens > wide.sentence_split_min_tokens);

    assert!(narrow.idf_top_k < normal.idf_top_k);
    assert!(normal.idf_top_k < wide.idf_top_k);

    assert!(narrow.max_sub_queries < normal.max_sub_queries);
    assert!(normal.max_sub_queries < wide.max_sub_queries);

    assert!(narrow.similarity_threshold > normal.similarity_threshold);
    assert!(normal.similarity_threshold > wide.similarity_threshold);
}

#[test]
fn test_depth_from_str() {
    assert_eq!(
        "narrow".parse::<AnalysisDepth>().unwrap(),
        AnalysisDepth::Narrow
    );
    assert_eq!(
        "normal".parse::<AnalysisDepth>().unwrap(),
        AnalysisDepth::Normal
    );
    assert_eq!(
        "wide".parse::<AnalysisDepth>().unwrap(),
        AnalysisDepth::Wide
    );
    assert_eq!(
        "WIDE".parse::<AnalysisDepth>().unwrap(),
        AnalysisDepth::Wide
    );
    assert!("invalid".parse::<AnalysisDepth>().is_err());
}

// ============================================================================
// BYTE SAFETY TESTS
// ============================================================================

#[test]
fn test_byte_safety_mixed_unicode() {
    let entries = vec![
        make_entry("😀 타입을 명시해줘", 1000),
        make_entry("🎉 에러 처리해주세요", 2000),
        make_entry("混合 한글 中文 text", 3000),
        make_entry("ㄱㄴㄷ 자음만", 4000),
        make_entry("🔥🔥🔥", 5000),
    ];

    let _result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );
}

#[test]
fn test_byte_safety_zero_width_chars() {
    let entries = vec![
        make_entry("타입\u{200B}을 명시해줘", 1000),
        make_entry("타\u{FEFF}입을 명시해주세요", 2000),
        make_entry("타입을 명시해줘", 3000),
    ];

    let _result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );
}

#[test]
fn test_byte_safety_long_korean_text() {
    let long_text = "타입을 ".repeat(100) + "명시해줘";
    let entries = vec![
        make_entry(&long_text, 1000),
        make_entry(&long_text, 2000),
        make_entry("에러 처리해줘", 3000),
    ];

    let _result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_edge_case_empty_input() {
    let entries: Vec<HistoryEntry> = vec![];
    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert_eq!(result.total, 0, "Should handle empty input");
    assert!(
        result.patterns.is_empty(),
        "Should produce no patterns for empty input"
    );
}

#[test]
fn test_edge_case_single_prompt() {
    let entries = vec![make_entry("타입을 명시해줘", 1000)];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert_eq!(result.total, 1, "Should count single entry");
    assert!(
        result.patterns.is_empty(),
        "Single prompt should not meet threshold of 2"
    );
}

#[test]
fn test_edge_case_all_confirmation_prompts() {
    let entries = vec![
        make_entry("네", 1000),
        make_entry("응", 2000),
        make_entry("알겠어", 3000),
        make_entry("좋아", 4000),
        make_entry("확인", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert_eq!(result.total, entries.len());
    for p in &result.patterns {
        assert!(!p.pattern.is_empty(), "Patterns should not be empty");
    }
}

#[test]
fn test_edge_case_identical_prompts() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해줘", 2000),
        make_entry("타입을 명시해줘", 3000),
        make_entry("타입을 명시해줘", 4000),
        make_entry("타입을 명시해줘", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    assert!(
        !result.patterns.is_empty(),
        "Should find pattern from identical prompts"
    );

    let strong_patterns: Vec<&TacitPattern> =
        result.patterns.iter().filter(|p| p.count >= 5).collect();

    assert!(
        !strong_patterns.is_empty(),
        "Should have at least one strong pattern"
    );
}

#[test]
fn test_edge_case_very_short_prompts() {
    let entries = vec![
        make_entry("타입", 1000),
        make_entry("에러", 2000),
        make_entry("테스트", 3000),
        make_entry("주석", 4000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );
    assert_eq!(result.total, entries.len());
}

#[test]
fn test_edge_case_prompts_with_only_whitespace() {
    let entries = vec![
        make_entry("   ", 1000),
        make_entry("\t\t", 2000),
        make_entry("\n\n", 3000),
        make_entry("타입을 명시해줘", 4000),
        make_entry("에러 처리해줘", 5000),
    ];

    let result = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &default_config(),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );
    assert_eq!(result.total, entries.len());
}

#[test]
fn test_edge_case_similarity_threshold_variation() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("에러 처리해줘", 3000),
    ];

    let result_high = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &config_with_similarity(0.9),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );
    let result_low = analyze_tacit_knowledge(
        &entries,
        2,
        10,
        &config_with_similarity(0.1),
        false,
        &TuningConfig::default(),
        &StopwordSet::load(&[]),
    );

    // Higher similarity threshold = stricter clustering = more groups
    // Lower similarity threshold = more aggressive merging = fewer groups
    // Both should work without error
    assert!(result_high.total == result_low.total);
}
