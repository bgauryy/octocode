# Karpathy Patterns
Load when grounding eval/loop advice in Karpathy primary sources. Why: verifiability is the scarce resource.

## Software 2.0
Specify desirable behavior via an **evaluation criterion** (dataset / reward / tests).
Then search for a program that satisfies it.
Iterate by growing the eval set — not by hand-writing every edge case.
Source: Software 2.0 (2017).

## RLVR + jagged intelligence
Reinforcement Learning from **Verifiable Rewards** spikes capability where checks are objective (math, code, tests). Public benches are gameable (“benchmaxxing”). Expect jagged performance: genius in verifiable pockets, brittle elsewhere. Source: 2025 LLM Year in Review.

## Autoresearch loop (canonical agent loop)
From `karpathy/autoresearch`:
- Human edits `program.md` (the skill); agent edits **one** subject file
- Eval harness (`prepare.py`) is **read-only**
- Fixed time budget; **one** metric (`val_bpb`); keep if better else discard
- Log experiments; NEVER STOP until interrupted

Strip domain specifics → universal recipe: one mutable subject · one metric · fixed budget · keep/discard · human owns the program.

## LLM Council
Multi-model first opinions → anonymized peer rank → chairman synthesis. Use for contested open-ended judgments, not as a substitute for deterministic outcome checks. Source: `karpathy/llm-council`.

## Agentic engineering (talk summaries)
Eval design, diff review, and taste become scarce as code generation gets cheap. Prefer primary Year-in-Review + autoresearch over secondary blog paraphrases when citing.

Next: write the KPI → `kpi-contract.md`; run the loop → `agent-loop.md`.
