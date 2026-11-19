# suggest-workflow CLI 레퍼런스

suggest-insight 가 필요할 때만 읽는 CLI 상세입니다. 바이너리 경로: `${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow`.

## 서브커맨드

| 커맨드 | 설명 |
|--------|------|
| `index --project PATH [--full]` | 인크리멘털 인덱싱 (`--full` 은 스키마 변경 시 전체 재구축) |
| `query --perspective NAME [--param K=V]...` | perspective 쿼리 |
| `query --sql-file PATH` | 커스텀 SQL 실행 (SELECT 만) |
| `query --list-perspectives` | 사용 가능한 perspective 목록 |

## 빌트인 perspectives

| Perspective | 파라미터 | 설명 |
|-------------|---------|------|
| `tool-frequency` | `top=10` | 도구 사용 빈도 (분류명 기준) |
| `transitions` | `tool` (필수) | 특정 도구 이후 전이 확률 |
| `trends` | `since=2020-01-01` | 주간 도구 사용 트렌드 |
| `hotfiles` | `top=20` | 자주 편집되는 파일 핫스팟 |
| `repetition` | `z_threshold=2.0` | 반복/이상치 탐지 (z-score² 기반) |
| `prompts` | `search` (필수), `top=20` | 프롬프트 키워드 검색 |
| `session-links` | `min_overlap=0.3` | 파일 공유 기반 세션 연결 |
| `sequences` | `min_count=3` | 자주 등장하는 도구 시퀀스 (2-gram) |
| `sessions` | `top=20` | 세션 목록 및 요약 |

## 인덱스

- 위치: `~/.claude/suggest-workflow-index/{project-encoded}/index.db`
- 변경 감지: 세션 파일의 size + mtime
- `--cache` 실행 시 DB 도 함께 갱신됩니다

## 캐시 (v2 호환)

```
~/.claude/suggest-workflow-cache/{project-encoded}/
  ├── index.json                    # 세션 메타 인덱스
  ├── sessions/
  │   └── {session-id}.summary.json # 세션별 통계 요약
  └── analysis-snapshot.json        # 전체 분석 스냅샷
```

### analysis-snapshot.json 분석 축

| 분석 | 키 | 데이터 소스 | 설명 |
|------|-----|-----------|------|
| 프롬프트 빈도 | `promptAnalysis` | in-memory | Top 반복 프롬프트 (BM25) |
| 도구 시퀀스 | `workflowAnalysis` | in-memory | 공통 시퀀스 패턴 |
| 프롬프트 클러스터 | `tacitKnowledge` | in-memory | BM25 클러스터 (분류는 LLM 몫) |
| 의존성 그래프 | `dependencyGraph` | in-memory | 도구 의존성 그래프 |
| 도구 전이 행렬 | `toolTransitions` | SQLite | A→B 전이 확률 |
| 반복/이상치 | `repetitionStats` | SQLite | 세션별 도구 사용 카운트 |
| 주간 트렌드 | `weeklyTrends` | SQLite | 도구별 주간 카운트 |
| 파일 핫스팟 | `fileAnalysis` | SQLite | 파일별 편집 횟수 |
| 세션 연결 | `sessionLinks` | SQLite | 파일 겹침 + 시간 근접 |

### summary.json 필드

| 필드 | 설명 |
|------|------|
| `prompts[]` | 사용자 프롬프트 + 타임스탬프 |
| `toolUseCount` | 총 도구 사용 횟수 |
| `toolSequences[]` | 도구 시퀀스 문자열 (예: `"Read → Edit → Bash:test"`) |
| `filesMutated[]` | 수정된 파일 경로 |
| `stats` | 순수 정량 통계 (프롬프트 수, 도구 수, 전이 횟수, 파일 편집 수 등) |

## v2 호환 옵션 (`--cache` 와 함께)

| 옵션 | 값 | 기본값 | 설명 |
|------|----|--------|------|
| `--project` | PATH | cwd | 프로젝트 경로 |
| `--cache` | flag | off | 캐시 생성 (DB 도 자동 갱신) |
| `--depth` | `narrow`, `normal`, `wide` | `normal` | 클러스터링 깊이 |
| `--threshold` | N | 3 | 최소 반복 횟수 |
| `--top` | N | 10 | 상위 N개 결과 |
| `--decay` | flag | off | 시간 감쇠 가중치 |
| `--exclude-words` | WORD,WORD,... | - | 분석에서 제외할 노이즈 단어 |
