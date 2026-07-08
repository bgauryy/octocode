# Reflection And Self-Improvement

**Audience**: agents and maintainers designing the feedback loop from task outcomes to better future behavior.

Reflection turns session outcomes into structured memory. It supports durable lessons, recurring failure mining, optional handoff/refinement proposals, and human-reviewed harness guidance.

## What Reflection Stores

| Data | Table | Writer |
|---|---|---|
| Lesson narrative | `memories` | `reflect record` |
| References | `memory_refs` | CLI: `reflect record --fix-file`; library API: `references`, `file`, `files`, `folders` |
| Failure clustering key | `memories.failure_signature` | `reflect record --failure-signature` |
| Reflection lifecycle event | `harness_log` | `reflect record` |
| Repo-fix or harness-fix hint | `refinements` and/or `harness_log.payload_json` | `reflect record --fix-repo`, `--fix-harness` |
| Structured eval failures | extra `memories` rows | `reflect record --eval-failure-json` |

Reflection outputs are advisory. They never apply skill, docs, or `AGENTS.md` changes automatically.

## Basic Flow

```text
reflect record
  -> memory inserted
  -> harness_log reflect event inserted
  -> optional refinement inserted
  -> mine-weakness clusters repeated signatures
  -> export-harness previews guidance candidates
  -> human reviews and applies accepted edits
```

## Recording A Reflection

```bash
octocode-awareness reflect record \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" \
  --task "Split awareness docs" \
  --outcome worked \
  --lesson "Focused docs by subsystem are easier to maintain than one giant harness reference" \
  --importance 7 \
  --failure-signature "mechanism:docs-harness|cause:monolithic-reference"
```

Outcomes:

| Outcome | Default intent |
|---|---|
| `worked` | Preserve a good pattern. |
| `partial` | Capture what worked and what still needs correction. |
| `failed` | Capture a failure mode that should be mined and avoided. |

Reflection memories are stored with the `reflection` tag and an outcome tag. The implementation currently labels main reflection memories as `EXPERIENCE` so they remain filterable and do not overwhelm normal briefings.

## Failure Signatures

Use `failure_signature` for repeated failure classes. Keep it short, stable, and structured:

```text
mechanism:<system>|cause:<root-cause>|surface:<where-it-showed>
```

Examples:

| Signature | Meaning |
|---|---|
| `mechanism:verify-gate|cause:lock-release-erased-obligation` | Verification state got lost after lock release. |
| `mechanism:skill-drift|cause:docs-updated-without-skill-reference` | User-facing docs and skill guidance diverged. |
| `mechanism:wiki-memory|cause:projection-not-regenerated` | `.octocode/` projection stale relative to DB. |

`reflect mine-weakness` groups active memories by this field.

## Mining Weaknesses

```bash
octocode-awareness reflect mine-weakness \
  --workspace "$PWD" \
  --limit 10 \
  --compact
```

Use this when failures repeat. It is a detection step, not a patching step. The output should lead to a proposal, a test, or a doc/skill change that a human can review.

## Exporting Harness Candidates

```bash
octocode-awareness reflect export-harness \
  --workspace "$PWD" \
  --min-importance 7 \
  --limit 10 \
  --compact
```

`export-harness` previews guidance candidates for `AGENTS.md`, `SKILL.md`, or package docs. The command reads memories and refinements; it does not write those target files.

Recommended review checklist:

| Check | Why it matters |
|---|---|
| Evidence | The memory should cite current files, commands, or a reproducible failure. |
| Generality | The proposed guidance should prevent a class of failures, not one incidental mistake. |
| Scope | Repo-specific rules go in repo docs; skill-wide rules go in skill docs. |
| Testability | A future agent should be able to tell whether it followed the guidance. |
| Freshness | Verify against current source before applying old memories. |

## Refinements vs Memories

| Store | Use for |
|---|---|
| `memories` | Durable lessons, decisions, gotchas, architecture facts. |
| `refinements` | Work state, handoffs, repo-fix proposals, and review queues. |
| `harness_log` | Lifecycle audit of reflect/mine/propose/validate/apply/capture events. |

A reflection can write all three: a memory for the lesson, a refinement for next action, and a harness log event for audit.

## Doc Staleness

`docs staleness` compares source edit activity with documentation edit activity using `edit_log`.

```bash
octocode-awareness docs staleness \
  --targets-json '[{"docFile":"packages/foo/ARCHITECTURE.md","sourceDirs":["packages/foo/src"]}]' \
  --workspace "$PWD" \
  --min-edits 5 \
  --min-lines 50 \
  --propose \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --compact
```

With `--propose`, stale docs can create `harness_log` proposal events. They still do not edit files automatically.

Bundled shell hooks and the Pi bridge populate basic `edit_log` update rows when they can extract file paths. Host integrations should call `insertEditLog()` directly when they need richer diff stats, create/delete/rename events, or host-specific session metadata.

## Human Approval Boundary

The self-improvement loop is intentionally not autonomous:

```text
memory evidence -> candidate guidance -> human/maintainer review -> explicit file edit -> tests
```
Do not let a reflection directly modify the rules that generated it. That separation keeps the harness useful without making it self-reinforcing in the worst way.
