# CI/CD 및 배포 - 완료

## 항목

- [x] **17. CI/CD 통합 (rust-binary.yml)**
  - autodev 바이너리 빌드 job 추가 (matrix strategy)
  - 크로스 플랫폼 빌드 (linux-x86_64, darwin-aarch64)
  - 릴리즈 아티팩트 업로드 (`autodev-{platform}.tar.gz`)
  - validate.yml에 autodev 전용 Rust 검증 추가 (fmt, clippy, test)

- [x] **18. marketplace.json 등록**
  - 플러그인 메타데이터 등록 (v0.2.8, category: automation)
  - plugin.json 포함 (agents 3개, commands 4개)
  - 버전 관리 연동 (bumpversion 자동 감지)

- [x] **19. README 문서화**
  - 설치 가이드 (Pre-built binary + Build from source + Requirements)
  - Quick Start (4단계)
  - CLI Commands (전체 서브커맨드 목록)
  - TUI Dashboard (레이아웃 다이어그램 + 키바인딩 테이블)
  - Configuration (YAML 예시 + File locations)
  - Architecture (코드 구조 + 라벨 상태 전이)
  - Slash Commands (4개 커맨드 테이블)

## 파일 변경

| 파일 | 변경 내용 |
|------|----------|
| `.github/workflows/rust-binary.yml` | `build-autodev` job 추가 (matrix: linux-x86_64, darwin-aarch64) |
| `.github/workflows/validate.yml` | autodev 변경 감지 + fmt/clippy/test 검증 추가 |
| `plugins/autodev/README.md` | Installation, Quick Start, CLI, TUI, Config 섹션 추가 |
