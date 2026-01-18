.PHONY: help build test validate validate-ci detect clean

BINARY := bin/validate

# 기본 타겟
help:
	@echo "사용 가능한 명령어:"
	@echo ""
	@echo "  make build           Go 바이너리 빌드"
	@echo "  make test            Go 테스트 실행"
	@echo "  make validate        플러그인 검증 (전체)"
	@echo "  make validate-ci     CI용 검증 (버전 제외)"
	@echo "  make validate-specs  스펙 검증만"
	@echo "  make validate-paths  경로 검증만"
	@echo "  make detect          변경된 패키지 감지"
	@echo "  make detect-from REF=<ref>  특정 ref 기준 변경 감지"
	@echo "  make clean           빌드 결과물 정리"
	@echo ""
	@echo "예시:"
	@echo "  make build && make validate"
	@echo "  make detect"
	@echo "  make detect-from REF=develop"

# Go 빌드
build:
	@echo "Building validate tool..."
	@mkdir -p bin
	@cd tools/validate && go build -o ../../$(BINARY) .
	@echo "Built: $(BINARY)"

# Go 테스트
test:
	@echo "Running tests..."
	@cd tools/validate && go test -v ./...

# 검증 (바이너리가 없으면 빌드)
validate: $(BINARY)
	@./$(BINARY) .

validate-specs: $(BINARY)
	@./$(BINARY) --specs-only .

validate-paths: $(BINARY)
	@./$(BINARY) --paths-only .

validate-versions: $(BINARY)
	@./$(BINARY) --versions-only .

# CI용 검증 (버전 검증 제외 - 머지 시점에 자동 범프)
validate-ci: $(BINARY)
	@./$(BINARY) --skip-versions .

$(BINARY):
	@$(MAKE) build

# 변경 감지
detect:
	@./scripts/detect-changes.sh main

detect-from:
	@./scripts/detect-changes.sh $(REF)

# 변경된 패키지만 검증
validate-changed: $(BINARY)
	@for pkg in $$(./scripts/detect-changes.sh main); do \
		echo "Validating $$pkg..."; \
		./$(BINARY) $$pkg || exit 1; \
	done

# 정리
clean:
	rm -rf bin/
	rm -rf node_modules/
