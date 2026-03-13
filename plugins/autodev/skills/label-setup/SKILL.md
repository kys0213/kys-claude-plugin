---
name: label-setup
description: GitHub 레포에 autodev가 사용하는 라벨을 자동으로 등록합니다
version: 1.0.0
---

# autodev 라벨 자동 등록

GitHub 레포에 autodev 워크플로우에서 사용하는 라벨을 생성/업데이트합니다.
`--force` 플래그를 사용하므로 이미 존재하는 라벨은 색상/설명이 업데이트됩니다.

## 라벨 정의

| 라벨 | 색상 | 설명 |
|------|------|------|
| `autodev:analyze` | `0366D6` (blue) | Trigger autodev analysis |
| `autodev:wip` | `FBCA04` (yellow) | Work in progress |
| `autodev:done` | `0E8A16` (green) | Completed |
| `autodev:skip` | `E4E669` (light yellow) | Skipped |
| `autodev:analyzed` | `1D76DB` (blue) | Analysis complete, awaiting review |
| `autodev:approved-analysis` | `28A745` (light green) | Analysis approved, awaiting implementation |
| `autodev:implementing` | `FBCA04` (yellow) | Implementation in progress |
| `autodev:changes-requested` | `E99695` (red) | Changes requested on PR |
| `autodev:extracted` | `D4C5F9` (purple) | Knowledge extracted |
| `autodev:extract-failed` | `B60205` (dark red) | Extraction failed |
| `autodev:impl-failed` | `B60205` (dark red) | Implementation failed |
| `autodev:analyze-failed` | `B60205` (dark red) | Analysis failed |
| `autodev:review-failed` | `B60205` (dark red) | Review failed |
| `autodev:improve-failed` | `B60205` (dark red) | Improve feedback failed |

## 입력 변수

- `REPO`: `<owner>/<repo>` 형식의 레포 식별자 (필수)
- `gh_host`: GitHub Enterprise 호스트명 (선택, 일반 GitHub일 경우 빈 값)

## 실행 스크립트

```bash
REPO="<owner>/<repo>"  # 호출 시 주입

declare -A LABEL_COLORS=(
  ["autodev:analyze"]="0366D6"
  ["autodev:wip"]="FBCA04"
  ["autodev:done"]="0E8A16"
  ["autodev:skip"]="E4E669"
  ["autodev:analyzed"]="1D76DB"
  ["autodev:approved-analysis"]="28A745"
  ["autodev:implementing"]="FBCA04"
  ["autodev:changes-requested"]="E99695"
  ["autodev:extracted"]="D4C5F9"
  ["autodev:extract-failed"]="B60205"
  ["autodev:impl-failed"]="B60205"
  ["autodev:analyze-failed"]="B60205"
  ["autodev:review-failed"]="B60205"
  ["autodev:improve-failed"]="B60205"
)

declare -A LABEL_DESCS=(
  ["autodev:analyze"]="Trigger autodev analysis"
  ["autodev:wip"]="Work in progress"
  ["autodev:done"]="Completed"
  ["autodev:skip"]="Skipped"
  ["autodev:analyzed"]="Analysis complete, awaiting review"
  ["autodev:approved-analysis"]="Analysis approved, awaiting implementation"
  ["autodev:implementing"]="Implementation in progress"
  ["autodev:changes-requested"]="Changes requested on PR"
  ["autodev:extracted"]="Knowledge extracted"
  ["autodev:extract-failed"]="Extraction failed"
  ["autodev:impl-failed"]="Implementation failed"
  ["autodev:analyze-failed"]="Analysis failed"
  ["autodev:review-failed"]="Review failed"
  ["autodev:improve-failed"]="Improve feedback failed"
)

created=0
skipped=0

for label in "${!LABEL_COLORS[@]}"; do
  color="${LABEL_COLORS[$label]}"
  desc="${LABEL_DESCS[$label]}"
  if [ -n "${gh_host}" ]; then
    output=$(gh label create "$label" --color "$color" --description "$desc" --repo "$REPO" --hostname "${gh_host}" --force 2>&1)
  else
    output=$(gh label create "$label" --color "$color" --description "$desc" --repo "$REPO" --force 2>&1)
  fi
  if [ $? -eq 0 ]; then
    echo "  ✓ $label"
    ((created++))
  else
    echo "  ⚠ $label: $output"
    ((skipped++))
  fi
done

echo ""
echo "라벨 등록 완료: ${created}개 등록, ${skipped}개 스킵"
```

등록 실패 시에도 전체 설정 흐름은 계속 진행합니다.
