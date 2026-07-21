---
name: octocode-documentation
description: Use when writing or updating docs, README, API docs, runbooks, AGENTS.md, CLAUDE.md, ADRs, Diátaxis restructuring, or codebase docs for humans or coding agents. Do not use for pure code research without a docs deliverable, or for authoring Agent Skills (SKILL.md).
---

# Octocode Documentation

Evidence-backed docs for humans and agents. Classify first. Gate writes. Prefer durable cross-refs over code dumps.

## Flow

`UNDERSTAND → RESEARCH → CLASSIFY → OUTLINE GATE → WRITE → VERIFY`

Compress when targets and type are named. Expand when claims need verification.

## Hard rules

- MUST verify claims in the repo before asserting them.
- MUST pick one mode and load its routes before writing.
- MUST gate creates/overwrites unless the user already approved targets this turn.
- MUST treat `AGENTS.md` as a docs index (links + non-obvious rules), not a content dump.
- Prefer durable pointers (module path, contract name, doc link) over line numbers and pasted code.
- FORBIDDEN: inventing commands, paths, APIs, or env vars.
- Keep one Diátaxis type per page; link siblings instead of mixing.
- Do not copy README/CONTRIBUTING into `AGENTS.md`, or paste large code blocks — link the source.

Stop when: outline gate awaits answer; write+verify finishes; fact missing (report gap); user cancels.

## Routes

Load only what the step needs:

- Read `references/modes.md` when choosing mode or audience.
- Read `references/evidence-research.md` when gathering or verifying repo facts.
- Read `references/diataxis.md` when writing or reviewing human-docs.
- Read `references/agents-md.md` when writing or updating agent instruction files.
- Read `references/adr.md` when recording a decision.
- Read `references/agent-readable.md` before WRITE (cross-refs, density, durability).
- Read `references/write-verify.md` for outline gate, write steps, and verify checklist.
- Skip `references/references.md` during normal tasks (creation audit only).

## Related

- Full multi-file pack → `octocode-documentation-writer` if installed.
- Authoring a `SKILL.md` → `octocode-skills`.
- Unclear mode → ask once: agent-docs / human-docs / adr / codebase-pack.
- Conflicting conventions → surface; do not add a second scheme.
- Missing command → omit or mark unverified. No Octocode → host search tools.
