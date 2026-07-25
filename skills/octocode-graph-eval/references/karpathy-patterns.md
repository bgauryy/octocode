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

## Bilevel Autoresearch (meta-loop extension)
Source: "Bilevel Autoresearch: Meta-Autoresearching Itself" (arxiv, March 2026).

The outer loop watches the **inner loop's search behavior**, reads its code and execution traces, identifies where the search process is stuck in model priors, and generates new code that changes *how* the inner loop searches — then injects it and reruns.

- Inner loop: propose → train → evaluate → keep/discard (Karpathy original)
- Outer loop: observe inner loop patterns → rewrite the search strategy → inject → repeat

Result on Karpathy's GPT pretraining benchmark: **5× improvement** (-0.045 vs -0.009 val_bpb) using the same LLM at both levels. The gain comes from architecture (outer loop forcing exploration outside the model's priors), not from a smarter model.

**Key difference from a meta loop that just tunes `program.md`:** the outer loop generates executable code that structurally changes the search, not just instructions. It breaks LLM priors by forcing exploration in directions the model's instincts avoided.

**Eval signal for escalation:** inner loop flat across N trials with no new hypothesis → suspect stuck priors → consider bilevel: outer observer that audits the inner loop's exploration trace and rewrites the strategy.

## Agentic engineering (talk summaries)
Eval design, diff review, and taste become scarce as code generation gets cheap. Prefer primary Year-in-Review + autoresearch over secondary blog paraphrases when citing.

Next: write the KPI → `kpi-contract.md`; run the loop → `agent-loop.md`; bilevel escalation → `nested-loops.md`.
