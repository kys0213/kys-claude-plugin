import { describe, test, expect } from 'bun:test';
import { createJiraService } from '../../src/core/jira';

// ============================================================
// JiraService.detectTicket — Black-box Test
// ============================================================

const jira = createJiraService();

describe('JiraService.detectTicket', () => {
  describe('Jira 티켓 감지 성공', () => {
    test('대문자 직접 티켓: WAD-0212 → { raw: "WAD-0212", normalized: "WAD-0212" }', () => {
      expect(jira.detectTicket('WAD-0212')).toEqual({ raw: 'WAD-0212', normalized: 'WAD-0212' });
    });

    test('prefix/대문자: feat/WAD-0212 → WAD-0212', () => {
      const result = jira.detectTicket('feat/WAD-0212');
      expect(result?.normalized).toBe('WAD-0212');
    });

    test('prefix/소문자: feat/wad-0212 → WAD-0212 (대문자 정규화)', () => {
      const result = jira.detectTicket('feat/wad-0212');
      expect(result?.normalized).toBe('WAD-0212');
    });

    test('fix prefix: fix/wad-2223 → WAD-2223', () => {
      const result = jira.detectTicket('fix/wad-2223');
      expect(result?.normalized).toBe('WAD-2223');
    });

    test('숫자가 긴 티켓: PROJ-12345 → PROJ-12345', () => {
      const result = jira.detectTicket('PROJ-12345');
      expect(result?.normalized).toBe('PROJ-12345');
    });

    test('prefix/ticket/description: feat/WAD-0212/add-login → WAD-0212', () => {
      const result = jira.detectTicket('feat/WAD-0212/add-login');
      expect(result?.normalized).toBe('WAD-0212');
    });

    test('하이픈 prefix: feat-WAD-0212 → WAD-0212', () => {
      const result = jira.detectTicket('feat-WAD-0212');
      expect(result?.normalized).toBe('WAD-0212');
    });
  });

  describe('Jira 티켓 미감지 (null 반환)', () => {
    test('일반 feature 브랜치: feature/user-auth → null', () => {
      // "feature/user-auth" → Pattern 1 매칭 시도: prefix=feature, ticket=user-auth
      // 하지만 "user"는 숫자가 아닌 "auth"가 뒤따르므로 \d+ 불일치 → null
      expect(jira.detectTicket('feature/user-auth')).toBeNull();
    });

    test('main 브랜치: main → null', () => {
      expect(jira.detectTicket('main')).toBeNull();
    });

    test('숫자만 있는 브랜치: 12345 → null', () => {
      expect(jira.detectTicket('12345')).toBeNull();
    });

    test('빈 문자열 → null', () => {
      expect(jira.detectTicket('')).toBeNull();
    });
  });

  describe('엣지 케이스', () => {
    test('프로젝트 키가 1글자: A-123 → null (최소 2글자 필요)', () => {
      expect(jira.detectTicket('A-123')).toBeNull();
    });

    test('숫자 뒤 추가 문자: feat/WAD-0212abc → WAD-0212 만 추출', () => {
      const result = jira.detectTicket('feat/WAD-0212abc');
      expect(result?.normalized).toBe('WAD-0212');
    });

    test('여러 티켓 패턴 존재: WAD-001-FIX-002 → 첫 번째 매칭', () => {
      const result = jira.detectTicket('WAD-001-FIX-002');
      expect(result?.normalized).toBe('WAD-001');
    });
  });
});
