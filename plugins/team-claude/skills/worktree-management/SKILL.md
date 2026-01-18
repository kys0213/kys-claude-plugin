---
name: worktree-management
description: Git worktree를 활용한 병렬 개발 환경 관리 기술
version: 1.0.0
---

# Git Worktree Management

Git worktree를 활용하여 여러 Worker Claude 인스턴스가 동시에 독립적으로 작업할 수 있는 환경을 관리합니다.

## Git Worktree 개요

Git worktree는 하나의 git 저장소에서 여러 작업 디렉토리를 생성할 수 있게 해주는 기능입니다.

```
main-repo/
├── .git/              # 공유되는 Git 데이터
├── (main 브랜치 파일들)
│
../worktrees/
├── feature-auth/      # auth 브랜치 작업 공간
│   └── (auth 브랜치 파일들)
│
└── feature-payment/   # payment 브랜치 작업 공간
    └── (payment 브랜치 파일들)
```

## 핵심 명령어

### Worktree 생성

```bash
# 기본 생성
git worktree add <path> <branch>

# 새 브랜치와 함께 생성
git worktree add <path> -b <new-branch> <start-point>

# 예시: feature-auth worktree 생성
git worktree add ../worktrees/feature-auth -b feature/auth origin/main
```

### Worktree 목록

```bash
# 모든 worktree 목록
git worktree list

# 출력 예시
/home/user/project           abc1234 [main]
/home/user/worktrees/auth    def5678 [feature/auth]
/home/user/worktrees/payment ghi9012 [feature/payment]

# 상세 정보 (porcelain 형식)
git worktree list --porcelain
```

### Worktree 제거

```bash
# 정상 제거 (변경사항 없어야 함)
git worktree remove <path>

# 강제 제거 (변경사항 무시)
git worktree remove --force <path>

# 삭제된 worktree 정리
git worktree prune
```

### Worktree 이동

```bash
git worktree move <old-path> <new-path>
```

## Team Claude에서의 활용

### Worker 생성 시

```bash
# 1. worktrees 디렉토리 생성
mkdir -p ../worktrees

# 2. 최신 main 가져오기
git fetch origin main

# 3. 새 worktree + 브랜치 생성
git worktree add ../worktrees/feature-auth -b feature/auth origin/main

# 4. worker 설정 파일 복사
mkdir -p ../worktrees/feature-auth/.claude
cp templates/worker-claude.md ../worktrees/feature-auth/.claude/CLAUDE.md
```

### Worker 완료 시

```bash
# 1. 변경사항 커밋
cd ../worktrees/feature-auth
git add .
git commit -m "feat: complete auth feature"
git push origin feature/auth

# 2. (PR 머지 후) worktree 제거
git worktree remove ../worktrees/feature-auth

# 3. 로컬 브랜치 정리 (선택)
git branch -d feature/auth
```

## 주의사항

### 브랜치 잠금

같은 브랜치를 여러 worktree에서 체크아웃할 수 없습니다:

```bash
# 오류 발생
$ git worktree add ../another main
fatal: 'main' is already checked out at '/home/user/project'
```

### 상태 동기화

각 worktree는 독립적이지만 `.git` 데이터는 공유됩니다:
- `git fetch`는 모든 worktree에 영향
- 브랜치 생성/삭제는 모든 worktree에서 보임
- 스태시는 공유됨

### 정리 작업

```bash
# 정리 필요한 worktree 확인
git worktree prune --dry-run

# 실제 정리
git worktree prune

# 잠긴 worktree 확인
git worktree list --porcelain | grep -A1 locked
```

## 디렉토리 구조 권장사항

```
~/projects/
├── my-project/           # 메인 저장소 (main 브랜치)
│   ├── .git/
│   ├── .team-claude/     # Team Claude 데이터
│   └── (프로젝트 파일)
│
└── worktrees/            # worktree 저장소
    ├── feature-auth/     # Worker A
    ├── feature-payment/  # Worker B
    └── feature-ui/       # Worker C
```

## 트러블슈팅

### Worktree 디렉토리가 이미 존재함

```bash
# 빈 디렉토리면 삭제 후 재시도
rm -rf ../worktrees/feature-auth
git worktree add ../worktrees/feature-auth -b feature/auth origin/main
```

### 브랜치가 이미 존재함

```bash
# 기존 브랜치 사용
git worktree add ../worktrees/feature-auth feature/auth

# 또는 브랜치 삭제 후 재생성
git branch -D feature/auth
git worktree add ../worktrees/feature-auth -b feature/auth origin/main
```

### Worktree 경로를 찾을 수 없음

```bash
# worktree 정리
git worktree prune

# 목록 재확인
git worktree list
```

## Best Practices

1. **명명 규칙**: `feature-<name>` 형식 일관성 유지
2. **정리 습관**: PR 머지 후 바로 worktree 제거
3. **경로 규칙**: 프로젝트 상위에 `worktrees/` 디렉토리 사용
4. **백업**: 중요 변경사항은 자주 push
5. **동기화**: 정기적으로 `git fetch` 실행
