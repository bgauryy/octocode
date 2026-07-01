# Precedence & Instruction Patterns

Load when resolving conflicting instructions, applying precedence, or choosing high-value patterns during FIX.

## Instruction precedence
When rules conflict, the highest priority wins.

| Priority | Category | Examples | Notes |
|----------|----------|----------|-------|
| 1 (highest) | Safety / tool restrictions | Forbidden tools, NEVER actions | Always wins |
| 2 | User explicit request | "I want X", "Do Y" | Overrides defaults |
| 3 | Forbidden/MUST rules | "FORBIDDEN: changing logic" | Overrides preferences |
| 4 | Skill defaults | Default behaviors, templates | Baseline |
| 5 (lowest) | Soft guidance | "prefer", "consider" | Yields to all above |

## Conflict resolution
1. **Detect** — name the two conflicting instructions explicitly.
2. **Resolve** — apply the precedence table (highest priority wins).
3. **Document** — add a one-line note: "Conflict: [A] vs [B] → resolved by priority [N]".
4. **Continue** — proceed using the resolved instruction only.

Do not proceed while both conflicting instructions remain active.

## State summaries
Use a concise summary only when it preserves context: goal, progress, next step, blockers.
- Full Path: produce one at each phase transition or context shift.
- Fast Path: produce one only when context shifts materially.

## High-value vs low-value content
| Keep (high value) | Remove or reduce (low value) |
|-------------------|------------------------------|
| Tables with explicit actions | Explanatory prose with no constraint |
| Imperative verbs (STOP, VERIFY, EXECUTE) | Repeated examples (keep 1-2) |
| Forbidden/Allowed lists | Long paragraphs a table would carry |
| IF/THEN decision rules | Hedging language in critical rules |
| Exact commands, versions, flags, triggers (signal) | Nominalizations, passive voice, double negatives |
| Markdown default, XML only for attention control | Emoji used as instructions |

## Quick reference
A mnemonic only; gate files are the source of truth.

| Need | Pattern |
|------|---------|
| Stop / checkpoint | `**STOP — DO NOT proceed**` plus a Gate Check |
| Mandatory action | `**REQUIRED:** You MUST [action]` |
| Prohibited action | `**FORBIDDEN:** [action]` |
| Decision logic | `**IF** [condition] → **THEN** [action]` |
| Critical rule hardening | Triple Lock: STATE + FORBID + REQUIRE |

## Common mistakes
- `over-strengthening` — turning "prefer" into MUST breaks optional flexibility; keep "should/prefer" for truly optional items.
- `orphan-referent` — "it/this/that" makes the agent apply a fix to the wrong element; name every entity explicitly.
- `changing-working-logic` — the user trusted the original behavior; if the logic works, leave it.
- `xml-overuse` — paired tags add noise and style drift with no reliability gain; keep Markdown default, XML only for attention control (see `attention.md` for when a tag earns its place).
- `over-compressing` — dropping signal or structural words creates ambiguity and garden-paths; cut only no-signal tokens, keep exact commands, versions, and structure.
