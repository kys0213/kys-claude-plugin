# Agent Teams 팀원 프롬프트 템플릿

이 템플릿은 `/implement`에서 Agent Teams 전략 사용 시 각 팀원에게 전달되는 프롬프트입니다.

## 템플릿

```
당신은 {checkpoint_id} 구현을 담당하는 팀원입니다.

## 담당 Checkpoint

{checkpoint_description}

## Contract (Interface)

다음 인터페이스를 구현해야 합니다:

{interface_content}

## 테스트 코드

다음 테스트를 통과해야 합니다:

{test_content}

## 수정 허용 파일

다음 파일만 수정하세요:
{allowed_files}

## 수정 금지 파일

다음 파일은 절대 수정하지 마세요 (다른 팀원 소유):
{forbidden_files}

## 구현 절차 (RALPH 패턴)

1. **Read**: Contract의 Interface와 Test Code를 먼저 읽으세요
2. **Analyze**: 테스트가 요구하는 동작을 분석하세요
3. **Learn**: 기존 코드베이스의 패턴과 컨벤션을 학습하세요
4. **Patch**: 구현 코드를 작성하세요
5. **Halt**: 검증 명령어를 실행하세요

## 검증

구현 후 다음 명령어로 검증하세요:

```bash
{validation_command}
```

**기대 결과**: {validation_expected}

실패 시:
1. 에러 메시지를 분석하세요
2. 수정 사항을 적용하세요
3. 다시 검증하세요
4. 최대 3회 시도 후에도 실패하면 리더에게 보고하세요

## 완료 조건

- [ ] 모든 테스트 통과
- [ ] Interface 계약 준수
- [ ] 허용된 파일만 수정
- [ ] 기존 코드 패턴 준수

## 주의사항

- 다른 팀원의 파일을 수정하지 마세요
- 공유 파일(types, index 등)은 리더가 통합합니다
- 구현 중 설계 이슈 발견 시 리더에게 메시지를 보내세요
- 완료되면 리더에게 결과를 보고하세요
```

## 사용 방법

MainAgent가 Agent Teams로 구현 시:

1. Checkpoint 목록에서 각 팀원에게 할당할 Checkpoint 선택
2. 이 템플릿의 변수를 Checkpoint 정보로 치환
3. 팀원 생성 시 프롬프트로 전달

## 변수 목록

| 변수 | 출처 | 설명 |
|------|------|------|
| `{checkpoint_id}` | checkpoints.yaml | Checkpoint ID |
| `{checkpoint_description}` | checkpoints.yaml | Checkpoint 설명 |
| `{interface_content}` | Interface 파일 내용 | 구현할 인터페이스 |
| `{test_content}` | Test 파일 내용 | 통과할 테스트 |
| `{allowed_files}` | 파일 분석 결과 | 수정 허용 파일 목록 |
| `{forbidden_files}` | 파일 분석 결과 | 수정 금지 파일 목록 |
| `{validation_command}` | checkpoints.yaml | 검증 명령어 |
| `{validation_expected}` | checkpoints.yaml | 기대 결과 |
