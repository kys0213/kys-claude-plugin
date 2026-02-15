// ============================================================
// JiraService — Jira 티켓 감지
// ============================================================
// detect-jira-ticket.sh 의 3단계 regex 매칭 로직을 통합합니다.
//
// 지원 패턴:
//   - feat/WAD-0212         → WAD-0212
//   - feat/wad-0212         → WAD-0212 (대문자 정규화)
//   - WAD-0212              → WAD-0212
//   - fix/wad-2223/desc     → WAD-2223
//
// 기존 대비 개선: 프로젝트 키 최소 2글자 제한으로 오매칭 방지
// ============================================================

import type { JiraTicket } from '../types';

export interface JiraService {
  detectTicket(branchName: string): JiraTicket | null;
}

// Pattern 1: prefix/TICKET-123 또는 prefix-TICKET-123
const PREFIXED_TICKET = /^[a-z]+[-/]([A-Za-z]{2,}-\d+)/;

// Pattern 2: 대문자 TICKET-123 (브랜치명 어디서든)
const UPPERCASE_TICKET = /([A-Z]{2,}-\d+)/;

// Pattern 3: 소문자 ticket-123 (브랜치명 어디서든, 최소 2글자 프로젝트 키)
const LOWERCASE_TICKET = /([a-z]{2,}-\d+)/;

export function createJiraService(): JiraService {
  return {
    detectTicket(branchName: string): JiraTicket | null {
      if (!branchName) return null;

      // Pattern 1: prefix/TICKET-123
      let match = branchName.match(PREFIXED_TICKET);
      if (match) {
        const raw = match[1];
        return { raw, normalized: raw.toUpperCase() };
      }

      // Pattern 2: UPPERCASE TICKET-123
      match = branchName.match(UPPERCASE_TICKET);
      if (match) {
        return { raw: match[1], normalized: match[1] };
      }

      // Pattern 3: lowercase ticket-123
      match = branchName.match(LOWERCASE_TICKET);
      if (match) {
        const raw = match[1];
        return { raw, normalized: raw.toUpperCase() };
      }

      return null;
    },
  };
}
