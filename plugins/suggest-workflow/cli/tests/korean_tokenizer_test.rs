use suggest_workflow::tokenizer::KoreanTokenizer;

#[cfg(feature = "lindera-korean")]
use lindera::dictionary::{load_embedded_dictionary, DictionaryKind};
#[cfg(feature = "lindera-korean")]
use lindera::mode::Mode;
#[cfg(feature = "lindera-korean")]
use lindera::segmenter::Segmenter;
#[cfg(feature = "lindera-korean")]
use lindera::tokenizer::Tokenizer;

#[test]
fn test_korean_tokenization_basic() {
    let tokenizer = KoreanTokenizer::new().expect("Failed to create tokenizer");

    // 테스트 1: 기본 한국어 문장
    let tokens = tokenizer.tokenize("항상 타입을 명시해줘");
    println!("Tokens: {:?}", tokens);

    // Go 조건: 토큰 수가 문자 수보다 적어야 함 (의미있는 분리)
    assert!(
        tokens.len() < "항상 타입을 명시해줘".chars().count(),
        "토큰화가 문자 단위로 분리됨 - lindera 실패"
    );

    // Go 조건: 최소 2개 이상의 토큰
    assert!(tokens.len() >= 2, "토큰이 너무 적음");
}

#[test]
fn test_korean_noun_extraction() {
    let tokenizer = KoreanTokenizer::new().expect("Failed to create tokenizer");

    // 명사 추출 테스트 - lindera feature에서만 POS tagging 가능
    let nouns = tokenizer.extract_nouns("한국어 형태소 분석기를 테스트합니다");
    println!("Nouns: {:?}", nouns);

    // lindera가 활성화된 경우 명사가 추출되어야 하고,
    // 아닌 경우 전체 토큰이 반환됨
    assert!(!nouns.is_empty(), "토큰이 비어있음");
}

#[test]
fn test_mixed_korean_english() {
    let tokenizer = KoreanTokenizer::new().expect("Failed to create tokenizer");

    let tokens = tokenizer.tokenize("conventional commit으로 커밋해줘");
    println!("Mixed tokens: {:?}", tokens);

    assert!(tokens.len() >= 2, "혼합 텍스트 토큰화 실패");
}

#[cfg(feature = "lindera-korean")]
#[test]
fn test_debug_token_details() {
    let dictionary = load_embedded_dictionary(DictionaryKind::KoDic).unwrap();
    let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
    let tokenizer = Tokenizer::new(segmenter);
    let tokens = tokenizer
        .tokenize("한국어 형태소 분석기를 테스트합니다")
        .unwrap();
    for t in &tokens {
        println!("surface={}, details={:?}", t.surface, t.details);
    }
}
