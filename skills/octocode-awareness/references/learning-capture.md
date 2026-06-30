# Capturing learnings from research & brainstorm

Read this when a research, brainstorm, or investigation produced a durable insight worth
recalling the next time the same issue or topic comes up. The goal: **don't re-research what
you already learned.** Store the lesson once, with the sources that back it, so a later
`get-memory` surfaces both the conclusion *and* where it came from.

Learning capture uses the same memory store as every other lesson (`tell-memory` /
`get-memory`, see `memory-recall.md`); the additions are `--reference` for provenance and optional
`--workspace`/`--repo`/`--ref` for applicability. References answer "where did we learn this?";
scope answers "where should this lesson apply?"

## When to capture

Capture after a meaningful convergence, not during exploration:
- Prior art resolved — "does X already exist?", "which library wins?", a chosen approach.
- A root cause, behavior, or constraint you had to dig to find.
- A decision and the evidence that drove it.

Do **not** capture routine status, raw search dumps, secrets/tokens, or anything the repo and
git history already record. One distilled lesson beats a transcript. Same discipline as
`tell-memory` (see `memory-recall.md`).

## How to capture

One `tell-memory` call. Put the causal lesson in `--observation`, and add one `--reference`
per source that backs it:

```bash
python3 <skill_root>/scripts/awareness.py tell-memory \
  --agent-id "$AGENT" \
  --task-context "Evaluating glob libraries for the bundler; reason: future agents need a default choice without re-benchmarking" \
  --observation "fast-glob is ~2x faster than globby on large trees; globby only adds gitignore handling on top of fast-glob. Prefer fast-glob unless gitignore semantics are needed." \
  --importance-score 7 --label API --tag globbing \
  --reference "https://github.com/mrmlnc/fast-glob" \
  --reference "npm:fast-glob" \
  --reference "pr:acme/bundler#482"
```

### Capture packet for other skills

`octocode-research` and `octocode-brainstorming` should finish with at most one compact packet:

```json
{
  "memoryObservation": "durable verdict or decision-changing constraint",
  "memoryReason": "why future agents need this / what failure or decision it changes",
  "memoryReferences": ["ev1 locator or fetched URL", "pr:owner/repo#123"],
  "supportingClaimIds": ["cl1"],
  "supportingEvidenceIds": ["ev1", "ev4"],
  "confidence": "confirmed|likely|uncertain",
  "supersedes": ["mem_old"],
  "doNotCaptureReason": null
}
```

Write one distilled memory from the packet, not one memory per ledger row. When calling
`tell-memory`, fold `memoryReason` into `--task-context` or `--observation`; it is a
capture-packet aid, not a separate stored field. If nothing durable survived, or if
`memoryReason` cannot be stated, set `doNotCaptureReason` and do not call `tell-memory`.

### Reference format

Free-form strings, but keep them conventional and exact so they read well and recall cleanly.
Use repo/file references only when that repo or file was a source, not merely because the memory
applies there; use `--workspace`/`--repo`/`--ref`/`--file` for applicability.

| Form | Example |
|------|---------|
| URL (article, doc, issue) | `https://docs.acme.dev/tenancy#limits` |
| Pull request | `pr:owner/repo#123` |
| Commit | `commit:owner/repo@abc1234` |
| Repository source | `repo:owner/repo@main` |
| npm package | `npm:fast-glob@3.3.2` |
| Named doc / paper | `doc:RFC-9110 Caching` |
| Local file source | `file:/abs/path/to/file.ts:42` |

Up to 20 references per memory, 512 chars each; duplicates are dropped, order preserved.

## How it comes back

References are stored structured **and** folded into the recall index, so any of these resurface the lesson:

```bash
# Plain recall — a reference token (pkg, PR, repo) hits via FTS:
get-memory --query "fast-glob"

# Everything learned from an exact source:
get-memory --query "" --reference "repo:owner/repo@main"
get-memory --query "" --reference "npm:fast-glob@3.3.2"

# Partial/source-family matching:
get-memory --query "" --regex "repo:owner/"

# Paraphrase-tolerant, after embed-index:
get-memory --query "which glob library is faster" --semantic
```

Recalled memories carry a `references` array — show the user *where* a claim came from instead
of re-deriving it. As always, **validate code-related memories against current code** before
relying on them (`memory-recall.md`); references are leads to re-open, not proof on their own.

## Re-research → supersede, don't duplicate

When you research the same topic again and learn a better answer, replace the old memory in one
step instead of stacking near-duplicates:

```bash
tell-memory --observation "<updated lesson>" --reference "<new source>" \
  --supersedes mem_abc123
```

The old memory is marked `SUPERSEDED` (hidden from default recall, its valid window closed) and
points at the new one. Then regenerate the read-first index: `awareness.py memory-index`.
