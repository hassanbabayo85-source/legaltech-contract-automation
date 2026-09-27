# Development Process & AI Tool Disclosure

This project was built for LexHack 2026 under a tight timeline, and we
believe transparency about how it was built is part of submitting honest,
judgeable work. This document explains the timeline, the effort involved,
and exactly which AI tools were used and how.

## Timeline

- **Day 0:** Learned about LexHack 2026 with 8 days remaining before the
  deadline.
- **Day 1:** Full-day planning — scoped the problem, selected the feature
  set (auth, contract ingestion, AI risk analysis, deadline reminders,
  notification channels), and designed the architecture and database
  schema before writing any application code.
- **Days 2–3:** Initial implementation attempt; lost time re-working the
  early setup before settling on the final architecture.
- **Days 4–7:** Focused, intensive build phase — approximately 12 hours a
  day — implementing the backend (Rust/Axum/SQLx), the frontend
  (Leptos/WASM), the AI analysis pipeline, security hardening, and tests.
- **Day 8:** Final testing, documentation, and submission.

The project was written from scratch for this hackathon; no pre-existing
personal codebase was reused as a starting point.

## AI tools used to build this project

It's important to separate two different uses of AI in this repository:

1. **AI used *by* the application at runtime** — this is a core feature
   of LexGuard itself, documented in the main README's "Tech Stack" and
   "Provider compatibility notes" sections (OpenAI-compatible providers:
   Groq, OpenAI, Gemini, used for contract risk analysis and image OCR).

2. **AI used *by the developer* to help build the application** — this
   is disclosed here for transparency:
   - **Gemini** — used for coding assistance during implementation.
   - **DeepSeek** — used as the primary coding assistant for generating
     and iterating on large sections of the Rust and Leptos code, chosen
     for its availability without tight usage/credit limits during a
     time-constrained build.
   - **Claude** — used for code review: identifying bugs, security gaps,
     and architectural issues, and suggesting concrete improvements
     (e.g. the SSRF hardening, evidence-verification approach, and error
     handling patterns were refined through this review process).

All architectural decisions, feature scope, integration work, testing,
and final code review/acceptance were done by the author. AI tools were
used as accelerators for a solo developer working under a strict
deadline — not as a substitute for understanding or verifying the code
that ships in this repository.

## Why we're disclosing this

A hackathon judge reviewing a solo submission with this much functionality
in 8 days might reasonably ask "how?" We would rather answer that
question upfront than have it raised as a doubt. We built this honestly,
under real time pressure, and we're proud of the result — including how
we got here.
