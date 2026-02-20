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
- **레포 삭제**

### Step 4: 설정 적용

#### 설정 변경인 경우

변경된 설정을 CLI에 전달:

```bash
autonomous repo config <name> --update '<json>'
```

변경 결과를 사용자에게 요약 출력합니다.

#### 레포 삭제인 경우

1. 선택된 레포의 큐 현황을 표시합니다:

```bash
autonomous queue list <name>
```

2. 활성 큐 아이템(pending/processing 상태)이 있으면 **경고**를 표시합니다.

3. AskUserQuestion으로 최종 삭제 확인:
   - "정말 삭제하시겠습니까? 관련된 모든 데이터(큐, 로그, 스캔 기록)가 함께 삭제됩니다."
   - 선택지: 삭제 진행 / 취소

4. 확인 시 삭제 실행:

```bash
autonomous repo remove <name>
```

5. 삭제 결과를 출력합니다.
