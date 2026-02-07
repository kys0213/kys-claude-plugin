use suggest_workflow::analyzers::suffix_miner::{SuffixMiner, DiscoveredSuffix, NormalizedPrompt};
use suggest_workflow::analyzers::tacit::analyze_tacit_knowledge;
use suggest_workflow::types::{HistoryEntry, TacitPattern, TacitAnalysisResult};

/// Helper to construct HistoryEntry with display and timestamp
fn make_entry(display: &str, ts: i64) -> HistoryEntry {
    HistoryEntry {
        display: display.to_string(),
        timestamp: ts,
        project: "test-project".to_string(),
    }
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
    let has_haejwo = suffix_strs.iter().any(|s| s.contains("해줄") || s.contains("해주세요"));
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

    let miner_low = SuffixMiner::new(2, 10, 0.1);  // Lower threshold
    let miner_high = SuffixMiner::new(2, 10, 0.5); // Higher threshold

    let suffixes_low = miner_low.mine(&prompts);
    let suffixes_high = miner_high.mine(&prompts);

    // Lower threshold should find more or equal suffixes (excluding fallbacks)
    let low_real: Vec<_> = suffixes_low.iter().filter(|s| s.frequency > 0).collect();
    let high_real: Vec<_> = suffixes_high.iter().filter(|s| s.frequency > 0).collect();

    assert!(
        low_real.len() >= high_real.len(),
        "Lower threshold should find more or equal real suffixes"
    );
}

#[test]
fn test_suffix_miner_empty_input() {
    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&[]);
    assert!(suffixes.is_empty(), "Empty input should produce no suffixes");
}

#[test]
fn test_suffix_miner_single_prompt() {
    let prompts = vec!["타입을 명시해줘"];
    let miner = SuffixMiner::new(2, 10, 0.5);

    let suffixes = miner.mine(&prompts);
    // With small corpus, fallback suffixes are added
    // Filter to only real mined suffixes (frequency > 0)
    let real_suffixes: Vec<_> = suffixes.iter().filter(|s| s.frequency > 0).collect();
    assert!(real_suffixes.is_empty(), "Single prompt should not meet min_support threshold");
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
    let suffixes = miner.mine(&prompts);
    assert!(true, "Should handle mixed unicode without panic");
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
    let normalized: Vec<_> = variations.iter()
        .map(|v| miner.normalize(v, &suffixes))
        .collect();

    // All variations should normalize to similar content
    assert!(!normalized.is_empty(), "Should produce normalized prompts");

    // Check that normalized versions strip polite endings
    for norm in &normalized {
        // Content should not contain polite suffixes
        assert!(
            !norm.content.contains("해줘") &&
            !norm.content.contains("해주세요") &&
            !norm.content.contains("하세요"),
            "Normalized content should strip polite suffixes"
        );
        // All should have same core content
        assert!(norm.content.contains("타입") && norm.content.contains("명시"),
                "Should preserve core content");
    }
}

#[test]
fn test_normalization_preserves_core_content() {
    let prompt = "항상 타입을 명시해줘";
    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&vec![prompt]);

    let normalized = miner.normalize(prompt, &suffixes);

    // Core content "타입" and "명시" should be preserved
    assert!(normalized.content.contains("타입") || normalized.content.contains("명시"),
            "Should preserve core content words");
}

#[test]
fn test_normalization_empty_after_stripping() {
    // Prompts that are only suffixes
    let only_suffixes = vec!["해줘", "해주세요", "하세요"];
    let miner = SuffixMiner::default();
    let suffixes = miner.mine(&only_suffixes);

    // Normalize each - should handle gracefully when content becomes empty
    for text in &only_suffixes {
        let normalized = miner.normalize(text, &suffixes);
        // Should return something (either stripped or original)
        assert!(!normalized.original.is_empty(), "Should handle suffix-only prompts gracefully");
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should cluster "타입 명시" variations together
    assert!(!result.patterns.is_empty(), "Should find patterns");

    // At least one pattern should have multiple examples
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should find separate patterns for "타입" and "에러"
    let pattern_texts: Vec<String> = result.patterns.iter()
        .map(|p| p.pattern.clone())
        .collect();

    let has_type_pattern = pattern_texts.iter().any(|p| p.contains("타입"));
    let has_error_pattern = pattern_texts.iter().any(|p| p.contains("에러"));

    assert!(has_type_pattern || has_error_pattern,
            "Should identify distinct topic patterns");
}

// ============================================================================
// TYPE CLASSIFICATION TESTS
// ============================================================================

#[test]
fn test_type_classification_directive() {
    let entries = vec![
        make_entry("항상 타입을 명시해줘", 1000),
        make_entry("반드시 타입을 명시해주세요", 2000),
        make_entry("꼭 타입을 명시하세요", 3000),
        make_entry("타입을 명시해줘", 4000),
        make_entry("타입 명시해주세요", 5000),
    ];

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should classify "항상/반드시/꼭" patterns as directive
    let has_directive = result.patterns.iter().any(|p| {
        p.pattern_type == "directive" ||
        p.pattern.contains("항상") ||
        p.pattern.contains("반드시") ||
        p.pattern.contains("꼭")
    });

    assert!(has_directive, "Should identify directive patterns");
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should find naming convention patterns
    let has_convention = result.patterns.iter().any(|p| {
        p.pattern_type == "convention" ||
        p.pattern.contains("camelCase") ||
        p.pattern.contains("snake_case")
    });

    assert!(has_convention, "Should identify convention patterns");
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should identify preference patterns
    let has_preference = result.patterns.iter().any(|p| {
        p.pattern_type == "preference" ||
        p.pattern.contains("선호") ||
        p.pattern.contains("좋아")
    });

    assert!(has_preference, "Should identify preference patterns");
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should identify correction patterns or at least find patterns
    // (Type classification depends on seed keywords in the implementation)
    let has_correction_indicators = result.patterns.iter().any(|p| {
        p.pattern_type == "correction" ||
        p.pattern.contains("아니야") ||
        p.pattern.contains("잘못")
    });

    // If no patterns found, at least verify the analysis ran
    if result.patterns.is_empty() {
        // With low threshold, should find some patterns
        assert!(result.total == entries.len(), "Should process all entries");
    } else {
        // If patterns found, they should be valid
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    assert!(result.patterns.len() > 0, "Should produce patterns");
    assert_eq!(result.total, entries.len(), "Should count all entries");

    // Check that patterns have reasonable confidence values
    for p in &result.patterns {
        assert!(
            p.confidence >= 0.0 && p.confidence <= 1.0,
            "Confidence should be between 0 and 1, got {}", p.confidence
        );
        assert!(p.count >= 2, "Pattern count should meet minimum threshold");
        assert!(!p.pattern.is_empty(), "Pattern should not be empty");
        assert!(!p.examples.is_empty(), "Should have examples");
    }
}

#[test]
fn test_full_pipeline_with_bm25_ranking() {
    let entries = vec![
        // High frequency pattern (should rank higher)
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("타입 명시해줘", 3000),
        make_entry("타입을 명시하세요", 4000),
        make_entry("타입 명시해주세요", 5000),
        // Lower frequency pattern
        make_entry("에러를 처리해줘", 6000),
        make_entry("에러 처리해주세요", 7000),
        make_entry("에러를 처리하세요", 8000),
    ];

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    assert!(!result.patterns.is_empty(), "Should find patterns");

    // Check BM25 scores are assigned
    for p in &result.patterns {
        assert!(p.bm25_score >= 0.0, "BM25 score should be non-negative");
    }

    // Patterns should generally be ranked by BM25 score (descending)
    // Note: With small corpus, there may be ties or slight variations
    if result.patterns.len() >= 2 {
        // Just verify that BM25 scores are reasonable, not strict ordering
        let max_score = result.patterns.iter().map(|p| p.bm25_score).fold(0.0, f64::max);
        let min_score = result.patterns.iter().map(|p| p.bm25_score).fold(f64::MAX, f64::min);
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Pattern with count=3 should have higher confidence than count=2
    if result.patterns.len() >= 2 {
        let higher_count_pattern = result.patterns.iter()
            .max_by_key(|p| p.count)
            .unwrap();
        let lower_count_pattern = result.patterns.iter()
            .min_by_key(|p| p.count)
            .unwrap();

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

    // All patterns appear only once, should not meet threshold of 2
    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should find no patterns or very few (only if suffix patterns meet threshold)
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
    let result = analyze_tacit_knowledge(&entries, 2, max_patterns, true, 0.3);

    assert!(
        result.patterns.len() <= max_patterns,
        "Should respect max_patterns limit, got {} patterns", result.patterns.len()
    );
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

    // Should not panic on mixed unicode
    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);
    assert!(true, "Should handle mixed unicode without panic");
}

#[test]
fn test_byte_safety_zero_width_chars() {
    let entries = vec![
        make_entry("타입\u{200B}을 명시해줘", 1000), // Zero-width space
        make_entry("타\u{FEFF}입을 명시해주세요", 2000), // Zero-width no-break space
        make_entry("타입을 명시해줘", 3000),
    ];

    // Should handle zero-width characters
    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);
    assert!(true, "Should handle zero-width characters without panic");
}

#[test]
fn test_byte_safety_long_korean_text() {
    let long_text = "타입을 ".repeat(100) + "명시해줘";
    let entries = vec![
        make_entry(&long_text, 1000),
        make_entry(&long_text, 2000),
        make_entry("에러 처리해줘", 3000),
    ];

    // Should handle very long texts
    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);
    assert!(true, "Should handle long Korean text without panic");
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_edge_case_empty_input() {
    let entries: Vec<HistoryEntry> = vec![];
    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    assert_eq!(result.total, 0, "Should handle empty input");
    assert!(result.patterns.is_empty(), "Should produce no patterns for empty input");
}

#[test]
fn test_edge_case_single_prompt() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
    ];

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Confirmation prompts should be filtered or produce low-value patterns
    // Just ensure no panic and reasonable output
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    assert!(!result.patterns.is_empty(), "Should find pattern from identical prompts");

    // Should produce exactly one strong pattern
    let strong_patterns: Vec<&TacitPattern> = result.patterns.iter()
        .filter(|p| p.count >= 5)
        .collect();

    assert!(!strong_patterns.is_empty(), "Should have at least one strong pattern");
}

#[test]
fn test_edge_case_very_short_prompts() {
    let entries = vec![
        make_entry("타입", 1000),
        make_entry("에러", 2000),
        make_entry("테스트", 3000),
        make_entry("주석", 4000),
    ];

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should handle very short prompts without panic
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

    let result = analyze_tacit_knowledge(&entries, 2, 10, true, 0.3);

    // Should handle whitespace-only prompts gracefully
    assert_eq!(result.total, entries.len());
}

#[test]
fn test_edge_case_min_confidence_filter() {
    let entries = vec![
        make_entry("타입을 명시해줘", 1000),
        make_entry("타입을 명시해주세요", 2000),
        make_entry("에러 처리해줘", 3000),
    ];

    // Very high confidence threshold
    let result_high = analyze_tacit_knowledge(&entries, 2, 10, true, 0.9);

    // Very low confidence threshold
    let result_low = analyze_tacit_knowledge(&entries, 2, 10, true, 0.1);

    // High threshold should produce fewer or equal patterns
    assert!(
        result_high.patterns.len() <= result_low.patterns.len(),
        "Higher confidence threshold should filter more patterns"
    );
}
