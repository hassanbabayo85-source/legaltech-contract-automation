#!/usr/bin/env bash
# =============================================================================
# LexHack — demo seed script.
#
# Creates a demo user, a fictional contract, and a demo webhook channel
# via the running backend's REST API. Uses only curl + jq. Does NOT
# touch the database directly.
#
# Safe to run repeatedly: the demo user's email is fixed, and if it
# already exists the register call is skipped and login is used instead.
#
# Prerequisites:
#   * The backend is running and reachable (default http://localhost:3000).
#   * `curl` and `jq` are installed.
#
# Usage:
#   ./scripts/seed_demo.sh
#   API_BASE=http://localhost:8080 ./scripts/seed_demo.sh
# =============================================================================

set -euo pipefail

API_BASE="${API_BASE:-http://localhost:3000}"
DEMO_EMAIL="demo@lexhack.local"
DEMO_PASSWORD="correct horse battery staple"
DEMO_NAME="Demo User"

require() {
    command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1" >&2; exit 1; }
}
require curl
require jq

echo "→ Registering demo user ($DEMO_EMAIL)…"
REGISTER_RESPONSE=$(curl -sS -X POST "$API_BASE/api/auth/register" \
    -H 'content-type: application/json' \
    -d "$(jq -nc \
        --arg e "$DEMO_EMAIL" \
        --arg p "$DEMO_PASSWORD" \
        --arg n "$DEMO_NAME" \
        '{email:$e, password:$p, full_name:$n}')" \
    -w '\n%{http_code}')

REGISTER_BODY=$(echo "$REGISTER_RESPONSE" | head -n -1)
REGISTER_CODE=$(echo "$REGISTER_RESPONSE" | tail -n 1)

if [ "$REGISTER_CODE" = "201" ]; then
    TOKEN=$(echo "$REGISTER_BODY" | jq -r .token)
    echo "  user created"
elif [ "$REGISTER_CODE" = "409" ]; then
    echo "  user exists; logging in instead"
    LOGIN_BODY=$(curl -sS -X POST "$API_BASE/api/auth/login" \
        -H 'content-type: application/json' \
        -d "$(jq -nc \
            --arg e "$DEMO_EMAIL" \
            --arg p "$DEMO_PASSWORD" \
            '{email:$e, password:$p}')")
    TOKEN=$(echo "$LOGIN_BODY" | jq -r .token)
else
    echo "unexpected register response ($REGISTER_CODE):" >&2
    echo "$REGISTER_BODY" >&2
    exit 1
fi

if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
    echo "failed to obtain a session token" >&2
    exit 1
fi

echo "→ Creating demo contract…"
CONTRACT_BODY=$(curl -sS -X POST "$API_BASE/api/contracts" \
    -H "authorization: Bearer $TOKEN" \
    -H 'content-type: application/json' \
    -d "$(jq -nc --arg t "Demo Employment Agreement (fictional)" --arg r "$(cat <<'CONTRACT'
DEMO EMPLOYMENT AGREEMENT (FICTIONAL)

This document is a fictional sample used only to demonstrate LexHack.
It contains no real personal or legal information.

1. Parties
   This Agreement is between Example Corp ("Employer") and
   Jane Demo ("Employee").

2. Term
   This Agreement commences on 2026-01-01 and terminates on
   2026-12-31, unless earlier terminated in accordance with
   Section 4.

3. Compensation
   Employee shall be paid a monthly salary. Payment is due within
   30 days of the end of each month.

4. Termination
   Either party may terminate this Agreement with 30 days written
   notice. The Employer may terminate immediately for cause.

5. Automatic Renewal
   If neither party provides notice of non-renewal at least 90 days
   before the end of the term, this Agreement shall renew
   automatically for an additional 12 months on the same terms.

6. Confidentiality
   Employee shall keep confidential all non-public information
   received during employment.

7. Governing Law
   This Agreement shall be governed by the laws of the fictional
   jurisdiction of Exampleland.
CONTRACT
)" '{title:$t, raw_text:$r}')")

CONTRACT_ID=$(echo "$CONTRACT_BODY" | jq -r .id)
if [ -z "$CONTRACT_ID" ] || [ "$CONTRACT_ID" = "null" ]; then
    echo "failed to create contract:" >&2
    echo "$CONTRACT_BODY" >&2
    exit 1
fi
echo "  contract_id = $CONTRACT_ID"

echo "→ Creating demo webhook channel…"
# This URL will fail SSRF validation on purpose — the channel is created
# so the demo can show the "Configured (encrypted)" indicator, but the
# worker will never succeed in delivering to it. That is fine for a
# demo; replace with a real URL for a real deployment.
CHANNEL_BODY=$(curl -sS -X POST "$API_BASE/api/notification-channels" \
    -H "authorization: Bearer $TOKEN" \
    -H 'content-type: application/json' \
    -d "$(jq -nc '{channel_type:"webhook", name:"Demo webhook", url:"https://example.com/lexhack/demo-hook"}')" \
    -w '\n%{http_code}')
CHANNEL_STATUS=$(echo "$CHANNEL_BODY" | tail -n 1)
if [ "$CHANNEL_STATUS" = "201" ] || [ "$CHANNEL_STATUS" = "409" ]; then
    echo "  channel ready"
else
    echo "  channel not created (status $CHANNEL_STATUS):"
    echo "$CHANNEL_BODY" | head -n -1 | jq . 2>/dev/null || echo "$CHANNEL_BODY"
fi

echo ""
echo "==========================================="
echo " Demo seed complete."
echo "==========================================="
echo "  URL:      $API_BASE"
echo "  Email:    $DEMO_EMAIL"
echo "  Password: $DEMO_PASSWORD"
echo ""
echo "Next steps:"
echo "  1. Sign in at $API_BASE/login"
echo "  2. Open the demo contract, click Analyze."
echo "  3. If AI_API_KEY is unset, the button returns 502 — set it and retry."
echo "  4. After a successful analysis, generate reminders."
