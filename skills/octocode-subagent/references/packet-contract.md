# Packet Contract

Load before invoking the local worker. Workers inherit no chat history — the packet is the entire world.

## Required fields

```text
GOAL:           <one sentence>
JOB:            summarize | extract | classify | draft | map | check | vision | translate
MODEL:          <exact name from ollama list>
INPUT:          <paths and/or inline slices; prefer paths + line ranges>
IMAGE:          <optional path for vision jobs>
OUTPUT_SCHEMA:  <JSON schema or bullet fields; be strict>
CONSTRAINTS:
  - no tools, no shell, no web
  - do not invent files, APIs, or line numbers
  - do not generate or invent images (vision = describe provided image only)
  - if unsure, emit null / "unknown" and say why
ACCEPTANCE:
  - <machine-checkable rules>
  - <1–2 spot-check rules for orchestrator>
RETURN:         stdout | file:.octocode/worker/<id>.json
```

## Prompt template (paste into worker)

```text
You are a local worker. Complete only the JOB. Obey CONSTRAINTS.
Return ONLY valid output matching OUTPUT_SCHEMA. No markdown fences unless asked.

GOAL: {{GOAL}}
JOB: {{JOB}}
MODEL: {{MODEL}}

INPUT:
{{INPUT}}

OUTPUT_SCHEMA:
{{OUTPUT_SCHEMA}}

ACCEPTANCE:
{{ACCEPTANCE}}
```

## Schema examples

See `references/packet-schemas.md` for JSON schema examples (Summarize, Extract, Classify, Translate, Article).

## Anti-patterns

- Vague goals (“look at the codebase”)
- Asking the worker to “fix” or “run tests”
- Asking the worker to open/fetch URLs
- Accepting free-form prose when JSON was required
- Packets larger than the local context window — shard instead (or raise `num_ctx`)
- Skipping local for every small task when a warm model is ready and the user wants local
- Integrating article summaries without quote substring checks
- Structured jobs without `--format-json` + schema text (+ prefer `--temperature 0.2`)
