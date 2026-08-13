# skilleval

스킬 description 이 실제로 발동을 만들어내는지, 여러 스킬이 겹칠 때 어느 쪽이 앞서는지를 측정한다.

`claude -p` 를 실제로 호출하므로 **수동 실행 전용**이다. CI 에 넣지 않는다 — 호출 비용이 들고
같은 입력에도 결과가 흔들린다.

## 쓰는 법

```bash
make build

./bin/skilleval \
  --eval-set tools/skilleval/examples/communicate.json \
  --skill plugins/atelier/skills/communicate/SKILL.md \
  --runs 5
```

경쟁 조건은 `--skill` 을 여러 번 준다. **첫 번째가 측정 대상**이고 나머지는 경쟁자다.

```bash
./bin/skilleval \
  --eval-set tools/skilleval/examples/communicate.json \
  --skill plugins/atelier/skills/communicate/SKILL.md \
  --skill plugins/atelier/skills/git/SKILL.md \
  --skill plugins/atelier/skills/orchestrator/SKILL.md \
  --runs 5 --workers 5
```

출력:

```
target=communicate  vs git, orchestrator  runs=5

  +  5/5   notes/work-log.md 내용을 팀 슬랙에 공유할 수 있게 정리해줘
  +  4/5   CHANGES.md 보고 PR 설명 써줘                    led git:3  with-others 4  no-fire 1
  -  0/5   auth.py 의 버그 고쳐줘                          no-fire 5

  should fire      27/30  (90%)
  should not fire  10/10  (100%)
```

| 표시 | 뜻 |
|---|---|
| `+` / `-` | 발동해야 하는 케이스 / 발동하면 안 되는 케이스 |
| `led X:n` | X 가 먼저 발동한 횟수 |
| `with-others n` | 대상과 다른 스킬이 **함께** 쓰인 횟수 |
| `no-fire n` | 아무 스킬도 발동하지 않은 횟수 |
| `timeout n` | 시간 초과로 판정 불가한 횟수 |

`led` 가 있다고 뺏긴 것이 아니다. `with-others` 와 함께 보라 — 다른 스킬이 앞서고 대상이 뒤이어
발동했다면 둘 다 쓰인 것이다. 이 둘을 구분하지 않으면 정반대 결론이 나온다.

## eval 세트

```json
[
  {
    "query": "notes/work-log.md 내용을 팀 슬랙에 공유할 수 있게 정리해줘",
    "expect": true,
    "files": { "notes/work-log.md": "- 결제 재시도 큐 도입\n- 실패율 4.1% -> 0.3%\n" }
  },
  { "query": "auth.py 의 버그 고쳐줘", "expect": false, "files": { "auth.py": "..." } }
]
```

- `expect: false` 케이스를 **반드시** 넣는다. 없으면 아무 데나 다 걸리는 description 이 만점을 받는다.
- `files` 는 프롬프트 전에 프로젝트 루트에 깔린다.

`files` 가 이 도구의 핵심이다. 이미 세션에 있는 재료를 다루는 스킬(남에게 옮겨 적기, 쉽게
풀어주기)은 **맥락 없는 프롬프트로는 절대 발동하지 않는다** — 에이전트가 "무엇을 정리할까요"
로 되묻고 끝나기 때문에 스킬을 부를 자리에 도달조차 못 한다. 맥락을 심어야 측정이 성립한다.

## 한계

- 사용자 레벨 스킬(`~/.claude/skills/`)은 격리되지 않고 함께 로드된다. 절대 발동률에는 영향이
  있으나, 같은 조건에서 두 description 을 비교하는 용도로는 유효하다.
- 실행마다 결과가 흔들린다. `--runs 5` 에서 보이는 쿼리별 2~3회 차이는 노이즈로 봐야 한다.
  결론을 내려면 갈린 쿼리만 `--runs 15` 이상으로 다시 재라.
