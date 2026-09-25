# LexGuard — Known Limitations

**Last updated:** 2026-09-25
**Audience:** reviewers, judges, and future maintainers.

This document is deliberately blunt. It lists what LexGuard **does
not** do, what it **cannot** do, and where its AI analysis is
**unreliable**. Read it before trusting any output.

---

## 1. AI output is non-deterministic

**What this means:** The same contract, analyzed twice with the same
prompt and the same model, can produce different results.

**Observed example (during development):**

| Run | Contract | Risk Score | Risks |
|-----|----------|-----------|-------|
| 1   | MUTUAL SERVICE AGREEMENT | 45 / 100 (Medium) | 1 |
| 2   | MUTUAL SERVICE AGREEMENT | 0 / 100 (Low)    | 0 |

**Why:** Large language models sample from a probability distribution.
Even at `temperature=0`, small differences in tokenisation or
provider-side batching can change the output.

**Impact:** Do not treat a single analysis as authoritative. Re-run
when a decision matters.

**Mitigation in place:** `analysis_prompt_version` is stored with every
result, so we can at least tell which prompt produced which output.

**Not mitigated:** There is no second-pass critic, no majority voting,
and no confidence calibration. All three are listed under *Future Work*.

---

## 2. No jurisdiction awareness

**What this means:** LexGuard does not know which country, state, or
legal system governs the contract.

**Why it matters:** Many clauses — non-competes, mandatory arbitration,
employment termination — are treated very differently across
jurisdictions. A clause that is routine in one country may be
unenforceable in another.

**Current behaviour:** The prompt instructs the model **not** to make
jurisdiction-specific claims ("illegal", "unenforceable") without a
stated jurisdiction. When none is given, the model says that
jurisdiction-specific review is required.

**What is missing:** There is no field on the contract to record a
jurisdiction, and no jurisdiction-aware rules. A planned migration
(`0008_ai_trust_hardening.sql`) adds the field but is not applied in
this build.

---

## 3. No contract-type awareness

**What this means:** LexGuard does not know whether the contract is an
employment agreement, NDA, SaaS agreement, lease, or something else.

**Why it matters:** The set of clauses that matter depends heavily on
the type. An employment agreement without a non-compete is normal; an
NDA without a confidentiality clause is not.

**Current behaviour:** The prompt includes a completeness checklist
(termination, payment, liability, IP, confidentiality, ...) but applies
it generically. It cannot tell, for example, that a missing *service
level agreement* is a real gap in a SaaS contract but irrelevant in a
one-off consulting agreement.

**What is missing:** No `contract_type` field is captured or used at
analysis time.

---

## 4. Evidence verification is fuzzy, not exact

**What this means:** When the model returns a quoted excerpt from the
contract, LexGuard checks that the excerpt appears in the source text.
This check is deliberately tolerant.

**Current algorithm (`src/ai/validation.rs::evidence_matches_contract`):**

1. **Exact match:** The evidence, after lowercasing and whitespace
   collapse, appears as a substring of the contract. → accept.
2. **Fuzzy match:** At least **70%** of the evidence's *significant*
   tokens (length ≥ 4) appear somewhere in the contract. → accept.

**Why fuzzy:** Models often paraphrase slightly — `may` → `can`,
`Agreement` → `agreement`, curly quotes, etc. An exact match rejected
too many valid analyses during development.

**What fuzzy matching does NOT prevent:** A model that invents a
plausible-sounding clause using words that all appear elsewhere in the
contract can slip through. This is a real, non-zero risk.

**Impact:** Evidence verification raises the bar against hallucination
but does not eliminate it.

---

## 5. No second-pass critic

**What this means:** The analysis is produced by a single model call.
There is no independent verification pass that tries to *disprove* each
risk or find missed risks.

**Why it matters:** A second pass catches roughly the same class of
errors a human reviewer would. Without it, the model's own biases are
never challenged.

**What is missing:** A `review_analysis()` method exists on the
`AiProvider` trait and defaults to returning the draft unchanged. The
OpenAI-compatible provider does not override it. Implementing a real
review pass is future work.

---

## 6. No clause-level segmentation

**What this means:** The model receives the **entire contract** as one
block of text and returns risks and obligations in one shot.

**Why it matters:** For contracts above ~20–30 pages, model attention
degrades. Section-level summarisation would scale better.

**Current behaviour:** The whole text is sent. If it exceeds
`AI_MAX_INPUT_CHARS` (default 100,000 bytes), the analysis is rejected
before it starts.

**What is missing:** Document structure parsing, section detection,
and per-clause analysis.

---

## 7. No cross-clause conflict detection

**What this means:** A risk that emerges from the *interaction* of two
clauses (e.g., a payment clause and a separate withholding clause)
will often be missed.

**Example:** Clause 4 says *"payment due within 30 days"*. Clause 11
says *"Company may withhold payment indefinitely."* The conflict is
real, but the model sees each clause independently.

**Current behaviour:** The prompt mentions "when two clauses conflict"
as a category, but there is no systematic pass for it.

---

## 8. Missing-protection detection is best-effort

**What this means:** The model can flag a *missing* clause (e.g.,
"this contract has no limitation-of-liability provision"). This is a
useful signal but it is not exhaustive.

**Why it matters:** Absence is harder to verify than presence. The
model may miss a missing protection, or invent one that is actually
present elsewhere in the document.

**Impact:** Treat "missing protection" findings as hints, not
conclusions.

---

## 9. No confidence score

**What this means:** The model returns a risk level and a numeric
score, but no calibrated confidence.

**Why it matters:** A score of 72 backed by a direct quote is very
different from a score of 72 inferred from ambiguous wording. The UI
does not distinguish these.

**Current behaviour:** `evidence_strength` (strong / moderate / weak)
is planned but not stored in this build.

---

## 10. Multi-instance rate limiting is in-process

**What this means:** Rate limits are stored in an in-memory map on each
backend process.

**Impact:**

- **Single instance:** Works as documented.
- **Multiple instances:** Each instance has its own counters, so an
  attacker hitting N instances effectively gets N× the limit.
- **Restart:** All counters are lost. A restart temporarily resets
  every user's quota.

**Mitigation for production:** Use a shared store (Redis, PostgreSQL
advisory locks) or put a rate limiter in front of the service.

---

## 11. Encryption key rotation is not supported

**What this means:** The ChaCha20-Poly1305 key used to encrypt
notification-channel credentials is read from
`NOTIFICATION_SECRET_KEY` at startup and used for every row.

**Impact:** Rotating the key requires re-encrypting every existing
row. There is no migration path. If the key is lost, every encrypted
channel credential is unrecoverable.

**Mitigation for production:** Back up the key like any other secret.
Plan a manual migration for rotation.

---

## 12. Image OCR depends on the network

**What this means:** Images and scanned PDFs are sent to a vision model
(Groq's `qwen/qwen3.8-27b`) for OCR.

**Impact:**

- **No internet:** Image upload and scanned-PDF upload fail.
- **Provider outage:** Same.
- **PII concern:** The image is sent to a third party. For sensitive
  documents, this may be unacceptable.

**Not mitigated:** There is no local OCR fallback in this build.
`ocrs` (a pure-Rust OCR engine) was considered but not integrated.

---

## 13. PDF text extraction fails on scanned PDFs

**What this means:** `pdf-extract` reads the *embedded text layer* of a
PDF. A PDF that is a photograph of paper has no text layer and cannot
be read.

**Current behaviour:** Returns a clear error ("PDF has no text layer;
it may be a scanned document"). The user is told to try the image
upload path instead.

---

## 14. Frontend tests are minimal

**What this means:** The frontend has a handful of `wasm-bindgen`
tests (`frontend/src/api/tests.rs`) that require `wasm-pack test` to
run. They cover request/response serialisation, not UI behaviour.

**Impact:** No automated test covers the login flow, the create-contract
form, the detail page tabs, or the theme toggle.

**Why:** `wasm-pack` is not installed in the current environment, and
installing it requires additional downloads.

---

## 15. AI analysis was verified against Groq only

**What this means:** During development, the analysis pipeline was
tested end-to-end against Groq's OpenAI-compatible endpoint with
`openai/gpt-oss-120b`.

**What was not tested:** The same pipeline against OpenAI itself,
Gemini, Ollama, or vLLM. The URL builder is provider-agnostic
(`src/ai/openai.rs::build_chat_url`), but only one provider was
exercised.

**Known provider differences:**

- Groq rejected `response_format: {"type": "json_object"}` on some
  models, so the prompt relies on explicit JSON instructions instead.
- Gemini's free tier returned `403 PERMISSION_DENIED` for the developer's
  project, so Gemini was abandoned for this build.

---

## 16. What is *not* a limitation

For balance, here is what is solid:

- Ownership is enforced in SQL, not in application code.
- Passwords use Argon2id with OWASP-recommended parameters.
- Session tokens are stored as SHA-256 hashes; raw tokens never touch
  the database.
- SSRF protection is enforced on every webhook request, and DNS
  rebinding is closed by pinning the HTTP client to validated
  addresses.
- Notification-channel credentials are encrypted with ChaCha20-Poly1305.
- Evidence verification (fuzzy) rejects a meaningful class of
  hallucinations and logs a verification score.
- Crash recovery resets both stale reminder claims and stale pending
  analyses.
- Prompt injection is explicitly guarded: the contract is labelled
  untrusted data in the system prompt.

---

## 17. Roadmap

If LexGuard were to move toward production, the following changes
would come first:

1. **Migration `0008`** — add jurisdiction, contract type, analyzed
   party, and verification metadata columns.
2. **Second-pass critic** — an independent review call that tries to
   falsify each risk.
3. **Local OCR fallback** (`ocrs`) for offline deployments.
4. **Clause-level analysis** for contracts above ~30 pages.
5. **Confidence calibration** and `evidence_strength` in the schema.
6. **Shared rate limiter** for multi-instance deployments.
7. **Key rotation** for `NOTIFICATION_SECRET_KEY`.
8. **Integration tests against multiple AI providers**.

None of these are in scope for the current build.

---

## 18. Reporting problems

If you find a case where LexGuard produces a wrong or misleading
result, the fastest way to understand *why* is:

1. Look at `contracts.analysis_prompt_version` — tells you which
   prompt produced the result.
2. Look at `contract_risks.evidence` — the quoted text the model used.
3. Compare (2) against `contracts.raw_text` — does the evidence appear?
4. Check `contracts.analysis_error` — did verification reject something?

A finding is actionable if it can be reproduced with the same
`PROMPT_VERSION` and the same raw text.
