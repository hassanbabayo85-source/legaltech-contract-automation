# AI Contract Analysis

## Overview

An authenticated contract owner can request AI analysis:


Synchronous endpoint: calls the configured provider, validates the
response, persists risks/obligations, returns the result. Idempotent at
contract level — repeated analysis replaces prior results.

## Provider abstraction

AiProvider (src/ai/provider.rs) is the only interface the business
layer depends on. OpenAiProvider speaks the OpenAI chat-completions
protocol (POST {AI_BASE_URL}/v1/chat/completions), which works with
OpenAI directly, and with Gemini's OpenAI-compatible routing,
OpenRouter, Ollama, vLLM, etc.

Tests use MockProvider and never call a real service.

## Configuration

| Variable | Required | Default | Meaning |
|---|---|---|---|
| AI_BASE_URL | No | https://api.openai.com | Provider base URL |
| AI_API_KEY | No | — | Bearer token; unset -> 502 on analyze |
| AI_MODEL | No | gpt-4o-mini | Model identifier |
| AI_TIMEOUT_SECONDS | No | 60 | HTTP timeout |
| AI_MAX_INPUT_CHARS | No | 100000 | Max contract text bytes |
| AI_MAX_RETRIES | No | 2 | Retries (max 10) |

## Analysis state

Each contract tracks: content_version, analysis_status
(not_analyzed|pending|completed|failed), analyzed_content_version,
analyzed_at, analysis_provider, analysis_model,
analysis_prompt_version, analysis_error. Stale iff
analyzed_content_version != content_version.

## Concurrency

Two simultaneous analyze requests result in exactly one AI call. The
first transitions to pending atomically; the second observes 0 rows
affected and returns 409 analysis_in_progress.

## Stale-result protection

content_version captured before the AI call. Final UPDATE includes
WHERE content_version = $N. If the user edits during the call, the
UPDATE affects 0 rows, the transaction is rolled back, and the
analysis is discarded.

## Server-side validation

Enforced: risk_level enum, risk_score 0-100, obligation status enum,
ISO dates, end>=start, non-empty required strings, bounded lengths,
evidence presence. Any failure aborts the whole result.

## Transaction


## Failure handling

| Failure | HTTP | analysis_error |
|---|---|---|
| Not configured | 502 | not_configured |
| Timeout (after retries) | 502 | timeout |
| Rate limited | 502 | rate_limited |
| Transient | 502 | transient |
| Invalid AI JSON | 502 | invalid_ai_output |
| Contract too large | 400 | unchanged |
| Modified during analysis | 409 | unchanged for new version |
| Two concurrent | 409 | first wins |

## Retries

Only Timeout/RateLimited/Transient retried with 500ms, 1s, 2s backoff
(capped 5s). Permanent/MalformedResponse/NotConfigured/Other are not
retried.

## Security

Never logged: raw_text, AI prompts/responses, API keys, Authorization
headers.

Logged: contract_id, user_id, content_version, provider, model,
failure category, counts.

## Legal disclaimer

AI analysis is machine-generated. It is NOT legal advice and does not
replace review by a qualified lawyer.

## Testing

    export DATABASE_URL="postgres://lexhack:lexhack_dev_pw@127.0.0.1:5432/lexhack_test"
    cargo test --test ai_analysis

## Manual smoke test

Do not commit real secrets. Export them in your shell only.

    export DATABASE_URL="postgres://lexhack:lexhack_dev_pw@127.0.0.1:5432/lexhack_test"
    export AI_BASE_URL="https://api.openai.com"
    export AI_API_KEY="sk-..."
    export AI_MODEL="gpt-4o-mini"
    cargo run

Then, in another terminal:

    curl -s -X POST http://localhost:3000/api/auth/register \
      -H 'content-type: application/json' \
      -d '{"email":"smoke@example.com","password":"correct horse battery staple","full_name":"Smoke"}' \
      | tee /tmp/register.json
    TOKEN=$(jq -r .token /tmp/register.json)

    curl -s -X POST http://localhost:3000/api/contracts \
      -H "authorization: Bearer $TOKEN" \
      -H 'content-type: application/json' \
      -d '{"title":"Smoke","raw_text":"This Agreement commences on 2026-01-01 and terminates on 2026-12-31."}' \
      | tee /tmp/contract.json
    CID=$(jq -r .id /tmp/contract.json)

    curl -s -X POST "http://localhost:3000/api/contracts/$CID/analyze" \
      -H "authorization: Bearer $TOKEN" | jq .

Verify with psql:

    SELECT id, start_date, end_date, risk_level, risk_score,
           analysis_status, analyzed_content_version
      FROM contracts WHERE id = '<CID>';
    SELECT title, risk_level, risk_score
      FROM contract_risks WHERE contract_id = '<CID>';
    SELECT title, due_date, status
      FROM contract_obligations WHERE contract_id = '<CID>';
    SELECT action, metadata
      FROM audit_logs WHERE entity_id = '<CID>' ORDER BY created_at;
