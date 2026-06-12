# 이슈 우선순위 판단

"뭐부터 작업할까" / 이슈 우선순위 요청 시 열린 GitHub issue 를 분석해 다음 작업을 추천하는 판단 로직 (`git` skill 이 로드). tool result 크기 폭발을 막기 위해 메타데이터 → 필터링 → 후보 압축 → 본문 조회 → 분석 순으로 진행한다.

## 실행 흐름

```
Step 1a: 메타데이터 수집 (body 제외)
   ↓ Step 1b: 머지/PR 상태로 필터링
   ↓ Step 1c: 상위 N개 후보 선정 후 body 조회
   ↓ Step 2: 우선순위 분석 + 의존성 그래프
   ↓ Step 3: 코드베이스 연관성 분석 (4단계)
   ↓ Step 4: 결과 출력 → Step 5: 다음 액션 제안
```

## Step 1a: 메타데이터 수집 (body 제외)

body 는 issue 당 수 KB 라 50개 이상이면 tool result 가 잘린다. body 제외 메타데이터만 먼저:

```bash
gh issue list --state open --json number,title,labels,createdAt,comments --limit 50
```

## Step 1b: 머지/PR 상태 필터링

**(1) 최근 머지 자동 감지 (단축 필터)** — per-issue API 비용 절감 위해 로컬 git log 로 먼저:

```bash
git log --oneline -100 | grep -oE '#[0-9]+' | sort -u
```

매칭 issue 는 `[머지됨]` 마커 + close 제안 + 후보·per-issue 조회 대상에서 제외.

**(2) 연결 PR 상태 체크** — (1)에서 안 걸린 후보만:

```bash
gh issue view ${NUM} --json closedByPullRequests,number,title
# 폴백: gh pr list --state all --search "${NUM} in:body"
```

| 상태 | 마커 | 처리 |
|------|------|------|
| PR open 존재 | `[PR 진행 중 #<pr>]` | 우선순위 제외 |
| PR closed/merged | `[머지됨]` | 제외 + close 제안 |
| 작업 브랜치만 (PR 없음) | `[작업 시작됨 #<branch>]` | 유지 (가중치 감점) |
| 연결 없음 | (없음) | 유지 |

> N개 후보 × 1회 = N회 호출. 50개 이상이면 `gh api graphql` 의 `closingIssuesReferences` 일괄 조회로 N+1 회피. "작업 브랜치만 존재"는 `gh issue develop --list ${NUM}` 로 확인하되, 레포 컨벤션상 issue 번호가 브랜치명에 없을 수 있어 false negative 가능.

## Step 1c: 상위 N개 후보 body 조회

Step 2 가중치 기준 **상위 10개**(또는 `--limit N`)로 압축한 뒤 body 조회 — body 총량을 50개분 → 10개분으로 축소:

```bash
for num in $CANDIDATE_NUMBERS; do
  gh issue view "$num" --json number,title,body,labels
done
```

## Step 2: 우선순위 가중치

| 기준 | 가중치 | 설명 |
|------|--------|------|
| 긴급도 | 높음 | bug > enhancement > documentation |
| 영향 범위 | 높음 | 핵심 기능 영향 여부 |
| 의존성(피의존) | 중간 | 다른 issue 가 이 issue 참조 → 가산 |
| 의존성(선행) | 중간 | 이 issue 가 미해결 issue 에 의존 → 감점 |
| 난이도 | 중간 | 구현 복잡도 (낮을수록 우선) |
| 오래된 정도 | 낮음 | 오래 방치 가점 |
| 댓글 수 | 낮음 | 관심도 |
| 작업 진행 흔적 | 낮음 | `[작업 시작됨]` 마커 감점 |

**의존성 그래프**: 각 body 에서 `#\d+` 추출 (`echo "$BODY" | grep -oE '#[0-9]+' | sort -u`) → `references` / `referenced_by` 맵 구축. `referenced_by` 많을수록 가산, 열린 `references` 있으면 감점. 출력 시 `← #526이 의존 / → #634 선행 요구` 형태로 시각화.

## Step 3: 코드베이스 연관성 분석 (4단계)

1. **경로 패턴 추출** — body 에서 코드 경로 토큰. 디렉토리 후보는 실제 최상위에서 동적 수집(정규식 메타문자 escape):
   ```bash
   ROOTS=$(ls -d */ 2>/dev/null | tr -d '/' | sed 's/[][\\.^$*+?()|]/\\&/g' | paste -sd '|' -)
   echo "$BODY" | grep -oE "(${ROOTS})/[A-Za-z0-9_./-]+" | sort -u
   ```
2. **존재 검증** — `ls -d <path>` / Glob 으로 거짓 양성 제거.
3. **활동 빈도** — 단일 git log 로 최근 변경 파일 카운트 맵 (per-path 반복 호출 회피):
   ```bash
   git log --since='3 months ago' --name-only --pretty=format: | grep -v '^$' | sort | uniq -c | sort -rn
   ```
   후보 경로(또는 prefix) 카운트 합산 → 활동 점수.
4. **현재 작업 교차** — default 브랜치 동적 조회 후 `git diff "${DEFAULT_BRANCH}...HEAD" --name-only` 와 issue 경로 교집합 → 연속성 가산.

산출물: issue 별 관련 파일/모듈, 활동 빈도 점수, 현재 작업 연관성 점수, 난이도 추정.

## Step 4: 결과 출력

상위 5개를 추천 순위로, 머지/PR-진행 중은 "정리 권장" 별도 섹션으로 분리:

```
## 추천 작업 순위
### 1위: #{번호} {제목}
- 긴급도/영향범위/난이도: ★ 표기
- 관련 파일 / 최근 활동 / 현재 작업 연관성
- 의존성: ← 피의존 / → 선행 요구
- 추천 이유: ...

## 참고: 정리 권장 issue
### [머지됨] #NNN — close 권장
### [PR 진행 중 #MMM] #NNN — 우선순위 제외
### [작업 시작됨 #<branch>] #NNN — 브랜치 존재, PR 미생성
```

## Step 5: 다음 액션 제안 (AskUserQuestion)

특정 issue 작업 시작(브랜치 생성) / `[머지됨]` 일괄 close / `[작업 시작됨]` 브랜치 전환 / 코멘트 추가 / 라벨 업데이트.

## 에러 처리

- gh 인증 실패 → `gh auth login` 안내
- issue 0개 → "열린 issue 없음" 후 종료
- Step 1a 결과 truncation → `--limit` 절반으로 재시도 + 사용자 알림
- `closedByPullRequests` 미지원 gh → `gh pr list --search` 폴백 자동 전환
