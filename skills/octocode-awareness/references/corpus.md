# Engineer Corpus

Read this before creating, reorganizing, or substantially updating `~/.octocode/awareness/corpus/**/*.md`. The corpus is the human-readable companion to the SQLite awareness store: memories keep searchable lessons, refinements keep repo handoffs, and corpus docs keep curated engineering context a future agent can browse.

## Canonical paths

- `~/.octocode/awareness/corpus/README.md` — the map. It should explain the corpus purpose, list the important docs/folders, and stay updated when docs are added, moved, or renamed.
- `~/.octocode/awareness/corpus/learn/ideas.md` — the learning backlog. Use it for subjects, experiments, and questions an agent wants to learn to perform better.
- `~/.octocode/awareness/corpus/**/*.md` — optional focused notes. Prefer lowercase folders such as `projects/`, `tools/`, `workflows/`, `patterns/`, `gotchas/`, `decisions/`, and `learn/`.

## When to write

Write only after a real flow teaches something useful. Good corpus candidates are repo maps, repeated gotchas, tool workflows, mental models, decision summaries, and "I should learn X because it would improve Y" ideas. Do not use the corpus for routine status, task handoff, raw transcripts, long logs, or anything secret. Use `refine-set` for unfinished work and `tell-memory` for searchable reusable lessons; mirror a distilled version into the corpus only when browsing it later would help.

Think like a junior engineer who wants to return tomorrow sharper: what would save confusion, reveal the next good question, or make the first thirty minutes of a similar task better?

## How to write

- Claim corpus files with `pre-flight-intent` before editing them, just like repo files.
- Keep notes short, specific, and source-backed. Include what was learned, why it matters, where it was observed, and how to verify or reuse it.
- Add to an existing focused doc before creating a new one. If a new doc is warranted, update `README.md` in the same pass.
- Separate known facts from learning wishes. `learn/ideas.md` is allowed to contain hypotheses and questions; other corpus docs should be verified.
- Never store secrets, credentials, raw `.env` values, private personal data, or giant copied outputs.

## Suggested note shapes

For a knowledge note:

```markdown
## YYYY-MM-DD - Short Topic

- Learned: One concise engineering fact or pattern.
- Why it helps: The future decision or task it improves.
- Evidence: File, command, test, PR, doc, or observed flow.
- Use when: The trigger for recalling this note.
```

For `learn/ideas.md`:

```markdown
## Open

- Topic: The subject or skill to learn.
  Why it helps: The concrete performance gain.
  Trigger: What flow exposed the gap.
  First step: The smallest useful experiment or source to inspect.
  Status: open | trying | learned | dropped.
```
