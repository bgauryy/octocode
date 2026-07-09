# Pi Agent Flow with Memory Tools

Guide to the Pi-extension live awareness flow, tool usage, schemas, and output quality.

See also: [REFLECT.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-pi-extension/docs/REFLECT.md) — full documentation of the Awareness learning, memory hygiene, and harness improvement loop.

## Validated surfaces

- The extension wires memory-awareness hooks for session start, write/edit calls, tool results, agent end, and session shutdown.
- The default agent tool surface includes 10 canonical memory and coordination tools; redundant compatibility aliases are not registered.
- Memory maintenance is Awareness-owned and user-approved via `/octocode-memory-digest` and `/octocode-memory-forget` commands.
- Each memory tool has a typed schema and a dedicated runtime operation.
- Tests validate required schema fields, lean outputs, duplicate handling, scoping, audit/verify behavior, reflection output, and workspace/refinement summaries.

## Runtime flow

```mermaid
flowchart TD
  A[Pi session start] --> B[before_agent_start]
  B --> C[Memory briefing: reminders, refinements, notifications]
  C --> D[Agent plans work]
  D --> E{Write/edit tool call?}
  E -- no --> F[Normal tool execution]
  E -- yes --> G[Extract target files]
  G --> H[Declare advisory FilesUnderWork on task or WORK run]
  H --> I{Exclusive conflict?}
  I -- yes --> J[Block edit with lock owner]
  I -- no --> K[Show peer overlap and run edit/write]
  K --> L[Keep task/WORK active; automatic HOOK run becomes PENDING]
  L --> M[Agent runs verification]
  M --> N[memory_verify marks SUCCESS or FAILED]
  F --> O[agent_end]
  N --> O
  O --> P{Pending tasks?}
  P -- yes --> Q[Follow-up verify gate: run checks then memory_verify]
  P -- no --> R[Conclusion allowed]
  R --> S[Session shutdown captures handoff unless disabled]
```

## Agent tool usage matrix

| Tool | Use when | Key inputs | Output shape / caveat |
|---|---|---|---|
| `memory_recall` | Before risky, unfamiliar, long, or related work. | `query`; optional `limit`, `min_importance`, `smart`, `label`, `references`, `regex`, scope. | Lean `{count, memories}`; raw observations appear because recall needs them. Carries `judgment_required` + `judgment_reason` when confidence is low (zero results or a weak top match) — treat those results as leads and verify before relying on them. |
| `memory_record` | Awareness reflection: store a verified durable root cause, decision, workaround, or gotcha after evidence exists. | `task_context`, `observation`; optional `label`, `importance`, `tags`, `references`, scope, `supersedes`, `allow_similar`, `failure_signature`. | Returns id/label/importance/novelty; duplicate skip avoids echoing long prose. |
| `memory_reflect` | Awareness reflection: after non-trivial work with a reusable lesson, repo fix, harness fix, or failure signature. | `task`; at least one lesson/failure/fix/signature field. | Returns lean learning id; includes next-action hints only when action exists. |
| `workspace_status` | Before long edits or when checking advisory work, exclusive locks, or pending work. | Optional workspace/repo scope. | Returns bounded FilesUnderWork/agent counts and optional exclusive locks. |
| `memory_refine_get` | At task start when previous reflections may have left actionable repo fixes. | Optional `state`, `include_handoffs`, `limit`, scope. | Returns lean refinement id/state/fix/files/repo summary. |
| `memory_audit_unverified` | Mid-turn when unsure; final audit also runs automatically. | No params. | Returns pending runs with test plans; non-zero exit when pending. |
| `memory_verify` | Only after running the stated verification. | One of `run_id`, `run_ids[]`, or `allPending:true`; optional `status`. | Single result or batch result; exit fails on per-id errors. |
| `agent_signal` | Real multi-agent coordination: publish/list/reply/resolve/ack blockers, handoffs, questions, decisions, or FYIs. | `action`; publish/reply fields include `kind`, `subject`, `body`, `to_agents`, `files`, and scope. | Returns compact signal/thread/workspace identifiers. |
| `file_lock` | A sensitive path needs exclusive protection. | `type`; optional `target_files`, task/WORK `run_id`, `ttl_ms`, `reasoning`. | Exclusive-only. Ordinary edits use advisory presence and can overlap. |
| `memory_export_harness` | Before proposing AGENTS.md changes — human review required. | `harness_only`, `limit`, `min_importance`, scope. | Two-tier markdown block; never writes files. |

> Consolidation/deletion (`memory_digest`, `memory_forget`) are **user commands**, not agent tools — see "User maintenance commands" below. Weakness clustering is **not** an agent tool; recurring failure-signature clusters are surfaced automatically in the session-start briefing.

## User maintenance commands

| Command | Default | Mutation |
|---|---|---|
| `/octocode-memory-digest` | Dry-run preview of archive/prune work. | `--apply` mutates after UI confirmation; non-UI requires `--apply --yes`. |
| `/octocode-memory-forget` | Dry-run preview by `--id`, `--tag`, `--before`, or `--max-importance`. | `--apply` deletes after UI confirmation; non-UI requires `--apply --yes`. Broad selectors (tag/age without ids) never sweep importance ≥ 8 unless `--max-importance` explicitly raises the ceiling (`salience_floor` reported). |

## Schema and output quality checks

- Required fields are schema-level where simple: `memory_recall.query`, `memory_record.task_context`, and `memory_record.observation`.
- Closed domains use enums where useful: labels, recall state/sort, refinement state, notification kind, reflect outcome, and verify status.
- Some cross-field constraints stay runtime-level because the schema cannot express them cleanly: `memory_verify` selector choice, non-empty reflection content, and notify kind/subject requirement.
- Outputs are intentionally compact: ids, counts, labels, scores, files, and next-action hints.
- Tools avoid echoing long prose except `memory_recall`, where returning observations is the purpose.

## Recommended agent protocol

1. **Attend:** call `workspace_status`; load refinements or targeted memory only when they can change the plan.
2. **Choose and act:** inspect Ready/Claimed/Verify through Awareness. Claim matching plan work or open standalone WORK. Every edit declares advisory file presence on that run; peer overlap is visible and allowed.
3. **Protect and coordinate:** use `file_lock` only when sensitive work needs exclusivity. Use `agent_signal` for real blockers, handoffs, questions, decisions, or FYIs.
4. **Verify:** submit claimed tasks, run the stated checks, then clear runs with `memory_verify({run_ids:[...]})` or `memory_verify({allPending:true})`.
5. **Reflect:** use the Awareness reflection loop for post-task lessons/fix queues/failure signatures; use `memory_record` for one specific verified finding.
6. **Maintain:** agents do not clean or delete memories; users run the maintenance commands above after preview.

## Copy-paste examples

```ts
workspace_status({ workspace_path: "/repo" })
memory_refine_get({ workspace_path: "/repo", limit: 5 })
memory_recall({ query: "editing Pi memory tools", workspace_path: "/repo", min_importance: 6 })
memory_record({ task_context: "Future generated-file edits", observation: "Generated files are rebuilt from sources; edit the source and rebuild", label: "GOTCHA", importance: 7, files: ["source-file"] })
memory_reflect({ task: "fixed verify gate", outcome: "worked", lesson: "Run verification before memory_verify", fix_repo: "Add a regression test for pending runs" })
memory_audit_unverified({})
memory_verify({ run_ids: ["run_..."], status: "SUCCESS" })
agent_signal({ action: "publish", kind: "blocker", subject: "Exclusive conflict", files: ["source-file"], importance: 8, workspace_path: "/repo" })
```

```bash
/octocode-memory-digest --export-doc
/octocode-memory-digest --apply
/octocode-memory-forget --tag EXPERIENCE --max-importance 5
/octocode-memory-forget --tag EXPERIENCE --max-importance 5 --apply
```
