.PHONY: help install validate detect-changes lint clean

# 기본 타겟
help:
	@echo "사용 가능한 명령어:"
	@echo ""
	@echo "  make install         의존성 설치"
	@echo "  make validate        플러그인 검증 (전체)"
	@echo "  make validate-specs  스펙 검증만"
	@echo "  make validate-paths  경로 검증만"
	@echo "  make detect          변경된 패키지 감지"
	@echo "  make detect-from REF=<ref>  특정 ref 기준 변경 감지"
	@echo ""
	@echo "예시:"
	@echo "  make detect              # main 브랜치 기준"
	@echo "  make detect-from REF=develop"

# 의존성 설치
install:
	npm install

# 검증
validate:
	npm run validate

validate-specs:
	npm run validate:specs

validate-paths:
	npm run validate:paths

validate-versions:
	npm run validate:versions

# 변경 감지
detect:
	@./scripts/detect-changes.sh main

detect-from:
	@./scripts/detect-changes.sh $(REF)

# 변경된 패키지만 검증
validate-changed:
	@for pkg in $$(./scripts/detect-changes.sh main); do \
		echo "Validating $$pkg..."; \
		npm run validate -- --path $$pkg || exit 1; \
	done

# 정리
clean:
	rm -rf node_modules
