> ❄️ **Snapshot freeze** — 이 플러그인은 v2.4.2 에서 동결되었습니다.
> 후속 개발은 [atelier](../atelier/) 에서 진행됩니다.
> 마이그레이션: `plugins/atelier/README.md` 참조.

# git-utils

Git workflow automation — branch, commit, PR, merge, sync, conflict resolution, issue management.

TypeScript CLI(`git-utils`)와 Default Branch Guard hook 을 제공하던 플러그인입니다.
CLI 는 atelier 의 단일 Rust 바이너리(`atelier git`)로 포팅되었고, hook 은
`/atelier:setup` 이 `atelier git guard` CLI 커맨드로 등록합니다.
