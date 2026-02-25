# Knowledge PR worktree 격리 (Gap C)

> **Priority**: Low — 안전성 개선, 실제 충돌 확률 낮음
> **출처**: design-v2-gap-analysis-final.md §2 Gap C
> **관련 계획**: IMPROVEMENT-PLAN-v2-gaps.md Phase 5, IMPLEMENTATION-PLAN-v2.md #22

## 배경

DESIGN-v2.md §8에서 knowledge PR 생성 시 main 기반 별도 worktree를 생성하여 구현 worktree와 격리하도록 설계.
현재 구현(`extractor.rs:266-328`)은 `base_path`(구현 worktree)에서 직접 branch를 생성하고 파일을 작성함.

done 전이 직전(커밋 완료 후) 실행되므로 실제 충돌 확률은 낮지만,
uncommitted 변경과의 충돌 가능성이 존재.

## 항목

- [ ] **1. `create_task_knowledge_prs()` 에 Workspace 파라미터 추가**
  - `knowledge/extractor.rs` — `workspace: &dyn Workspace` 파라미터 추가
  - main 기반 별도 worktree 생성 (`workspace.create_worktree()`)

- [ ] **2. 격리된 worktree에서 branch + 파일 작성 + PR 생성**
  - 기존 `base_path` 대신 별도 worktree 경로 사용

- [ ] **3. worktree 정리**
  - 작업 완료 후 `workspace.remove_worktree()` 호출

- [ ] **4. 호출부 수정**
  - `pipeline/pr.rs` — `extract_task_knowledge()` 호출 시 workspace 전달

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `knowledge/extractor.rs` | Workspace 파라미터 추가, worktree 격리 로직 |
| `pipeline/pr.rs` | 호출부 workspace 전달 |

## 완료 조건

- [ ] knowledge PR이 별도 worktree에서 생성됨
- [ ] 구현 worktree에 영향 없음
- [ ] cargo test 통과
