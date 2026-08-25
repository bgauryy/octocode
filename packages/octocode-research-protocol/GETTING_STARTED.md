# Get started with Octocode Research Protocol

This tutorial walks you through one complete exchange between two agents: a researcher hands off a task, the assignee reports progress and submits a final result, and the researcher acknowledges it. By the end, you validate every message with the real schemas and see what each one catches.

## Goal

Build and validate a minimal task, message, and result exchange using `octocode-research-protocol`.

## Prerequisites

- Node.js `^20.19.0` or `>=22.12.0`.
- TypeScript, or a runner that supports ES modules (this tutorial uses `tsx`).
- `octocode-research-protocol` and `zod` installed as dependencies.

## Step 1: import the schemas you need

Create a file named `exchange.ts` and import the pieces this exchange uses:

```ts
import {
  AgentHandoffSchema,
  AgentTaskSchema,
  AgentMessageInputSchema,
  toAgentMessage,
  AgentResultSchema,
  AgentResultAckSchema,
  RESEARCH_PROTOCOL_VERSION,
} from "octocode-research-protocol";
```

Every schema is a [Zod](https://zod.dev) object. Call `.parse()` when invalid input is a bug in your own code, or `.safeParse()` when the input comes from another agent and you want to handle rejection yourself.

## Step 2: hand off a task

A handoff is one agent (`from`) delegating an objective to another (`to`). Validate one with `AgentHandoffSchema`:

```ts
const NOW = new Date().toISOString();

const handoff = AgentHandoffSchema.parse({
  protocolVersion: RESEARCH_PROTOCOL_VERSION,
  handoffId: "handoff-1",
  taskId: "task-1",
  contextId: "ctx-1",
  from: "researcher",
  to: "remote_research",
  objective: "Determine whether retry scheduling covers the failure-recovery path.",
  successCriteria: ["Return a source locator and resolved commit."],
  createdAt: NOW,
});
```

`objective` is the single question this task must answer, not the full briefing. `successCriteria` takes at most four entries — the schema forces you to pick the ones that matter.

## Step 3: accept the handoff as a task

The assignee accepts the handoff by emitting an `AgentTask`. Reuse the handoff's `taskId` and `contextId` so both sides agree on which exchange this is, and record `assignedBy` so the task remembers who delegated it:

```ts
const task = AgentTaskSchema.parse({
  protocolVersion: RESEARCH_PROTOCOL_VERSION,
  taskId: handoff.taskId,
  contextId: handoff.contextId,
  agent: handoff.to,
  assignedBy: handoff.from,
  objective: handoff.objective,
  semanticAvailability: "available",
  successCriteria: handoff.successCriteria,
  status: "running",
  createdAt: NOW,
});
```

`AgentTaskSchema` is a discriminated union on `semanticAvailability`. Set it to `"available"` for a normal task. The other branch, `"unavailable_after_recovery"`, is for a runtime that recovered only an objective digest and must not invent replacement plaintext — see `README.md` for that case.

## Step 4: send a message mid-task

Build the message content with `AgentMessageInputSchema`, then call `toAgentMessage()` to add the routing and sequencing envelope:

```ts
const message = toAgentMessage(
  AgentMessageInputSchema.parse({
    kind: "evidence",
    content: "Found the retry scheduler; checking the failure-recovery branch next.",
    evidenceRefs: ["src/retry.ts:10"],
  }),
  {
    taskId: task.taskId,
    contextId: task.contextId,
    messageId: "msg-1",
    sequence: 1,
    from: task.agent,
    to: handoff.from,
    sentAt: NOW,
  },
);
```

`toAgentMessage()` exists because every runtime otherwise re-implements this same join. Increase `sequence` by one for each message you send in this task; the receiver uses it to detect gaps and reordering.

## Step 5: submit an evidence-backed result

A result claiming `"answered"` or `"partial"` must cite at least one piece of evidence. `evidence[].ref` must be a compact, checkable locator — a path with a line number, a request ID, a URL — not a sentence:

```ts
const result = AgentResultSchema.parse({
  finding: {
    question: task.objective,
    summary: "retry.ts owns retry scheduling; the failure-recovery branch is covered.",
    status: "answered",
    terminal: true,
    evidence: [
      { ref: "src/retry.ts:10", claim: "scheduleRetry owns retry scheduling." },
    ],
  },
});
```

`terminal` must be `true` on every result — it's the field that marks this task as done from the assignee's side.

## Step 6: acknowledge the result

The researcher reviews the result and responds with a verdict. `"accepted"` needs no reason; `"needs_revision"` and `"rejected"` both require one:

```ts
const ack = AgentResultAckSchema.parse({
  protocolVersion: RESEARCH_PROTOCOL_VERSION,
  taskId: task.taskId,
  contextId: task.contextId,
  verdict: "accepted",
  acknowledgedAt: NOW,
});
```

## Outcome

You built and validated one full exchange: a handoff, an accepted task, a mid-task message, an evidence-backed result, and an acknowledgement. Five schemas worked together, each one rejecting the specific mistake it exists to catch — a missing citation, an unbounded success-criteria list, a message with no sequence number.

## Next steps

- **Two peers, no handoff.** If neither side assigns the other, skip `AgentHandoffSchema` entirely: each peer self-assigns its own `AgentTask`, after exchanging `AgentCapabilityDeclarationSchema` and grounding the goal with `AgentGoalRestatementSchema`/`AgentGoalAckSchema`. See `README.md` under "Symmetric or asymmetric, by choice, not by construction."
- **Evidence grading for high-stakes claims.** `ResearchClaimSchema` and `EvidenceSourceGradeSchema` let you require corroboration from more than one tool before trusting an "impact" or "incident" conclusion. See `README.md` under "Evidence first."
- **Deployment-specific fields.** Every top-level envelope carries an optional `extensions` bag for data this protocol doesn't standardize — see `AgentExtensionsSchema` in `README.md`'s schema catalog.
- **Writing an agent's own instructions.** `PROMPTS.md` has ready-to-use system-prompt fragments for the goal-holder, tool-holder, and symmetric-peer roles.
- **Every schema and design decision.** `README.md` is the full reference.
