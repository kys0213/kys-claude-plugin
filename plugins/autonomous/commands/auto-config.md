---
description: 등록된 레포의 자율 모니터링 설정을 변경합니다
argument-hint: "[repo-name]"
allowed-tools: ["AskUserQuestion", "Bash"]
---

# 모니터링 설정 변경 (/auto-config)

등록된 레포의 설정을 대화형으로 변경합니다.

## 실행 흐름

### Step 1: 레포 선택

인자가 있으면 해당 레포를, 없으면 등록된 레포 목록을 보여주고 선택:

```bash
autonomous repo list
```

### Step 2: 현재 설정 표시

```bash
autonomous repo config <name>
```

### Step 3: 변경할 항목 선택

AskUserQuestion으로 변경할 항목을 선택:
- 스캔 주기
- Consumer 처리량 (Issue/PR/Merge)
- 워크플로우 선택
- 필터 설정
- 활성화/비활성화

### Step 4: 설정 적용

변경된 설정을 CLI에 전달:

```bash
autonomous repo config <name> --update '<json>'
```

변경 결과를 사용자에게 요약 출력합니다.
