---
paths:
  - "**/hooks/**"
  - "**/scripts/**"
  - "**/cmd/hook/**"
---

# 도구 층 경계: CLI · 스크립트 · 훅

> CLAUDE.md "책임 경계(CLI vs Skill/Agent)"의 **도구 층 내부** 세분화 규칙. 결정적 로직을 CLI 서브커맨드 · 셸 스크립트 · 훅 중 어디에 둘지 결정한다. 판단/지능(Skill·Agent)과 도구의 경계는 CLAUDE.md를 따른다.

## 원칙

1. **도구 층에는 judgment 없는 결정적 코드만**. 판단·우선순위·자연어 해석은 Skill/Agent로 올린다.
2. **결정적 로직의 1차 거처는 CLI**. 같은 plugin에 CLI 바이너리가 있으면 결정적 로직은 셸이 아니라 **CLI 서브커맨드**로 둔다 (단일 책임 + 크로스플랫폼 + 테스트 가능).
3. **셸은 두 경우에만**: (a) **부트스트랩** — 바이너리 존재/버전 보장처럼 *바이너리가 생기기 전에* 도는 코드, (b) **외부 도구 얇은 래퍼** — 단순 shell-out.
4. **외부 시스템 어댑터**(LLM·git·gh)는 응답이 비결정적이어도 *어댑터 자체는 결정적* → 도구 층에 둔다. 단 모델 선택·응답 해석 같은 판단이 끼면 Skill/Agent로 올린다.

## 훅: 로직은 CLI 서브커맨드, 등록은 thin shim

훅 설정은 결국 **"명령 문자열"을 실행**할 뿐이라, 그 자리에 `.sh`가 아니라 CLI 바이너리를 직접 박아도 된다.

- ✅ **훅 로직** → `<cli> hook <name>` 서브커맨드 (stdin JSON 읽고 결정 JSON 출력). 크로스플랫폼 + assert_cmd 테스트 가능.
- ✅ **등록 진입점** → thin POSIX shim. 바이너리 감지 → 위임, 부재 시 graceful skip. 부트스트랩 순서 때문에 이 한 겹만 불가피.
- ❌ semver 비교·config 파싱·PR base 가드·statusline 렌더링 같은 **결정적 로직을 bash에 내장**. `.sh`는 POSIX/bash 한정이라 Windows 네이티브 미지원이고 테스트가 없다.

### DO

```jsonc
// settings.json 훅 등록 — 바이너리 직접 호출, 부재 시 skip
"command": "command -v autopilot >/dev/null 2>&1 && autopilot hook guard-pr-base || exit 0"
```
```rust
// src/cmd/hook/guard_pr_base.rs — 로직은 CLI에. stdin(JSON) 읽고 결정 JSON 출력.
```

### DON'T

```bash
# 훅 .sh 안에 결정 로직을 내장
EXPECTED_BASE=$(grep ... github-autopilot.local.md)   # config 파싱
if [[ "$base" != "$EXPECTED_BASE" ]]; then block; fi   # 판정/차단
# ↑ 결정적 도메인 로직이 bash에 흩어짐 → 비이식적(Windows 불가) + 테스트 부재
```

## LLM 어댑터: 도구 층, 단 judgment-free

**"두 번 호출하면 같은가?"는 *코드의 결정*에 대한 질문이지 외부 stochastic 서비스의 바이트에 대한 질문이 아니다.** HTTP 클라이언트가 서버 응답이 매번 달라도 결정적인 것과 같다. LLM 어댑터도 응답이 흔들려도 judgment-free한 도구다.

- ✅ "주어진 model+prompt로 invoke해서 raw 반환" → 도구 층 (CLI 서브커맨드 또는 thin 셸)
- ❌ "응답이 이상하면 다른 모델로 재시도" / "diff 크기 보고 모델 자동 선택" → judgment → Agent로 올림
- **셸 vs CLI는 도구 층 *내부*의 이식성/테스트성 비용 결정**이지 책임 경계가 아니다. 어댑터가 부르는 외부 CLI(gemini/codex) 자체가 비-크로스플랫폼이면 얇은 셸로 둬도 원칙 위반이 아니다.

## 판단 기준 (요약)

| 코드 성격 | 거처 |
|----------|------|
| 결정적 도메인 로직 (상태전이·검증·파싱·계산) | **CLI 서브커맨드** |
| 바이너리 존재/버전 보장 | **부트스트랩 셸** (ensure-binary 류) |
| 훅 등록 진입점 | **thin POSIX shim** → CLI 위임 |
| 외부 도구 shell-out 래퍼 | 얇은 셸 또는 CLI |
| 모델 선택·응답 해석·우선순위 결정 | **Skill/Agent** |

## 체크리스트

- [ ] 훅/스크립트에 들어간 로직이 결정적인데 같은 plugin에 CLI가 있다면 → 서브커맨드로 옮겼는가?
- [ ] 셸에 남은 것은 부트스트랩 / thin shim / 외부도구 래퍼뿐인가?
- [ ] 외부 시스템 어댑터에 모델 선택·응답 해석 같은 judgment가 섞이지 않았는가?
- [ ] 훅 등록은 CLI를 직접 호출하되 바이너리 부재 시 graceful skip 하는가?
- [ ] CLI로 옮긴 결정 로직에 블랙박스 테스트(assert_cmd)를 추가했는가?
