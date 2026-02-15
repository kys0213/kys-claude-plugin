// ============================================================
// JiraService — Jira 티켓 감지 인터페이스
// ============================================================
// detect-jira-ticket.sh 의 3단계 regex 매칭 로직을 통합합니다.
//
// 지원 패턴:
//   - feat/WAD-0212         → WAD-0212
//   - feat/wad-0212         → WAD-0212 (대문자 정규화)
//   - WAD-0212              → WAD-0212
//   - fix/wad-2223/desc     → WAD-2223
// ============================================================

import type { JiraTicket } from '../types';

export interface JiraService {
  /**
   * 브랜치 이름에서 Jira 티켓을 감지합니다.
   * 순수 함수 — git 호출 없이 문자열만 분석합니다.
   *
   * @param branchName - 분석할 브랜치 이름
   * @returns 감지된 티켓 또는 null
   */
  detectTicket(branchName: string): JiraTicket | null;
}
