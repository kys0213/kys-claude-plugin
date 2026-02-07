---
description: Google Gemini CLI를 사용하여 임의의 프롬프트를 실행합니다
argument-hint: "[프롬프트]"
allowed-tools: ["Bash", "Read", "Glob"]
---

# Gemini 호출 커맨드

Google Gemini CLI를 사용하여 임의의 프롬프트를 실행하는 범용 커맨드입니다.

## 사용법

```bash
/invoke-gemini "[프롬프트]"
```

## 워크플로우

1. **프롬프트 받기**: 사용자의 프롬프트를 그대로 전달
2. **파일 경로 수집** (선택): 프롬프트에 파일이 언급되면 Glob으로 수집
3. **스크립트 호출**: common/scripts/call-gemini.sh 실행
4. **결과 반환**: 출력 파일 내용을 사용자에게 전달

## 스크립트 호출

```bash
${CLAUDE_PLUGIN_ROOT}/../../common/scripts/call-gemini.sh "[프롬프트]"
```

스크립트가 자동으로:
- "대상 파일:" 섹션에서 파일 경로 추출
- 파일 내용 읽기
- 프롬프트에 파일 내용 추가
- Gemini CLI에 전달

## 예시

### 코드 생성
```bash
/invoke-gemini "Python으로 간단한 웹 서버를 만들어줘"
```

### 파일 분석
```bash
/invoke-gemini "대상 파일:
- src/main.py

이 파일의 성능 최적화 방법을 제안해줘"
```

### 질문
```bash
/invoke-gemini "Kubernetes와 Docker Swarm의 차이점을 설명해줘"
```

## 출력

스크립트가 `.review-output/gemini-YYYYMMDD_HHMMSS.txt` 파일을 생성하고, 그 내용을 반환합니다.

## 주의사항

- **CLI 필요**: `gemini` CLI가 설치되어 있어야 합니다
- **API 키 필요**: Google API 키가 환경변수에 설정되어 있어야 합니다
- **실행 시간**: 요청에 따라 30초-2분 소요
- **Sandbox 모드**: 보안을 위해 --sandbox 플래그로 실행됩니다

## 에러 처리

```
Error: Google Gemini 호출에 실패했습니다.

가능한 원인:
- gemini CLI가 설치되지 않음
- API 키가 설정되지 않음
- 네트워크 연결 문제

해결 방법:
1. gemini CLI 설치 확인: gemini --version
2. API 키 확인: echo $GOOGLE_API_KEY
3. 네트워크 연결 확인
```
