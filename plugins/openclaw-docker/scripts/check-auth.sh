#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<EOF
Usage: $(basename "$0") [project-dir]

OpenClaw 인증 상태를 확인합니다.
auth-profiles.json에서 JWT exp 클레임을 추출하여 토큰 만료 여부를 표시합니다.

Arguments:
  project-dir  OpenClaw 프로젝트 루트 (기본: ~/Documents/openclaw)

Output format:
  PROFILE=<name> STATUS=VALID|EXPIRED EXPIRES=<ISO8601> REMAINING=<duration>

Security:
  토큰 값(access, refresh)은 절대 출력하지 않습니다.

Exit codes:
  0  모든 프로필 유효
  1  만료된 프로필 있음
  2  파일 없음 또는 파싱 오류
EOF
  exit 0
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
fi

PROJECT_DIR="${1:-$HOME/Documents/openclaw}"
ENV_FILE="$PROJECT_DIR/.env.local"

# 1. Extract OPENCLAW_CONFIG_DIR from .env.local
if [[ ! -f "$ENV_FILE" ]]; then
  echo "ERROR=ENV_NOT_FOUND $ENV_FILE" >&2
  exit 2
fi

CONFIG_DIR="$(grep '^OPENCLAW_CONFIG_DIR=' "$ENV_FILE" | cut -d'=' -f2- | sed 's/^["'\'']//' | sed 's/["'\'']*$//')"
# Expand ~ if present
CONFIG_DIR="${CONFIG_DIR/#\~/$HOME}"

if [[ -z "$CONFIG_DIR" ]]; then
  echo "ERROR=CONFIG_DIR_NOT_SET OPENCLAW_CONFIG_DIR not found in .env.local" >&2
  exit 2
fi

# 2. Check auth-profiles.json
AUTH_FILE="$CONFIG_DIR/agents/main/agent/auth-profiles.json"

if [[ ! -f "$AUTH_FILE" ]]; then
  echo "ERROR=AUTH_FILE_NOT_FOUND $AUTH_FILE" >&2
  exit 2
fi

# 3. Parse with node (same pattern as run.sh sync_codex_auth)
node -e '
const fs = require("fs");
const path = process.argv[1];

let data;
try {
  data = JSON.parse(fs.readFileSync(path, "utf8"));
} catch (e) {
  process.stderr.write("ERROR=PARSE_FAILED " + e.message + "\n");
  process.exit(2);
}

const profiles = data.profiles || {};
const names = Object.keys(profiles);

if (names.length === 0) {
  process.stderr.write("ERROR=NO_PROFILES auth-profiles.json has no profiles\n");
  process.exit(2);
}

const now = Date.now();
let hasExpired = false;

for (const name of names) {
  const p = profiles[name];
  let expiresMs;

  // Primary: JWT access_token exp claim (run.sh sync_codex_auth pattern)
  if (p.access) {
    try {
      const parts = p.access.split(".");
      if (parts.length === 3) {
        const payload = JSON.parse(Buffer.from(parts[1], "base64url").toString());
        if (payload.exp) {
          expiresMs = payload.exp * 1000; // sec -> ms
        }
      }
    } catch (_) {
      // fallback below
    }
  }

  // Fallback: expires field directly (already ms)
  if (!expiresMs && p.expires) {
    expiresMs = p.expires;
  }

  if (!expiresMs) {
    console.log("PROFILE=" + name + " STATUS=UNKNOWN EXPIRES=N/A REMAINING=N/A");
    continue;
  }

  const expiresDate = new Date(expiresMs);
  const diffMs = expiresMs - now;
  const status = diffMs > 0 ? "VALID" : "EXPIRED";
  if (diffMs <= 0) hasExpired = true;

  // Format remaining
  const absDiff = Math.abs(diffMs);
  const days = Math.floor(absDiff / 86400000);
  const hours = Math.floor((absDiff % 86400000) / 3600000);
  const sign = diffMs < 0 ? "-" : "";
  const remaining = sign + days + "d" + hours + "h";

  console.log(
    "PROFILE=" + name +
    " STATUS=" + status +
    " EXPIRES=" + expiresDate.toISOString().replace(/\.\d{3}Z$/, "Z") +
    " REMAINING=" + remaining
  );
}

process.exit(hasExpired ? 1 : 0);
' "$AUTH_FILE"
