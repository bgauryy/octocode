# Octocode Research

Investigate local code, external repositories, packages, docs, history, failures, and reviews with exact evidence before asserting a claim or making a change.

**Use when** you need callers, imports, affected scope, safe-delete proof, a root cause, upstream/package/PR evidence, or evidence before and after an edit; or when asked to "research this" / "use octocode".

**Not for** a trivial edit with known impact, documentation work (`octocode-documentation`), skill structure (`octocode-skills`), or open ideation (`octocode-brainstorming`).

```text
FRAME → CLASSIFY → MODEL → SEMANTIC? → SEARCH/READ → PROVE → DECIDE/PATCH → VERIFY
```

Search hits are leads until exact bytes confirm them; empty results describe only the searched lane.

```bash
npx -y octocode skill install octocode-research
node scripts/check-description.mjs && node scripts/check-guidance.mjs --self-test --examples
```
