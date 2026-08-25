# Octocode Research Protocol

Octocode Research Protocol is a set of TypeScript/[Zod](https://zod.dev) contracts for how research agents talk to each other. Maybe you're building a system where one agent hands work to another. Maybe several agents each hold different tools and need to compare notes. Either way, this package gives you validated message shapes for that conversation, instead of everyone inventing their own ad hoc JSON.

It's deliberately narrow. It doesn't run your agents, deliver your messages, or store anything. It defines what a task, a message, a finding, and an acknowledgement look like, and it validates that every one of them says what it claims to say.

## Why this exists

Two agents talking to each other tend to fail in the same few ways:

- One side assumes the other understood the goal the same way it did.
- A "finding" turns out to be a guess with nothing backing it.
- A message carries a custom field, and the receiver's strict validator rejects the whole thing outright.
- A claim that really mattered got accepted on the strength of a single, unverified source.

None of these are exotic failure modes — they're the ordinary cost of letting message shapes drift.

This package addresses that by making the shapes explicit and machine-checked, instead of relying on a convention everyone has to remember on their own. You can't mark a task answered without citing evidence. A restated goal is a real object either side can check, not a hope that the other agent "got it." A high-stakes claim can require more than one kind of evidence before anyone trusts it. And every message still has room for whatever your own deployment needs to bolt on, without breaking that validation.

## Install

```sh
npm install octocode-research-protocol zod
```

Requires Node.js `^20.19.0` or `>=22.12.0`. `zod` is a peer dependency the package validates against directly.

## A quick example

Here's the smallest real exchange: one agent hands off a task, and the other reports back a result with evidence attached.

```ts
import { AgentHandoffSchema, AgentResultSchema, RESEARCH_PROTOCOL_VERSION } from "octocode-research-protocol";

const handoff = AgentHandoffSchema.parse({
  protocolVersion: RESEARCH_PROTOCOL_VERSION,
  handoffId: "handoff-1",
  taskId: "task-1",
  contextId: "ctx-1",
  from: "researcher",
  to: "remote_research",
  objective: "Determine whether retry scheduling covers the failure-recovery path.",
  successCriteria: ["Return a source locator and resolved commit."],
  createdAt: new Date().toISOString(),
});

const result = AgentResultSchema.parse({
  finding: {
    question: handoff.objective,
    summary: "retry.ts owns retry scheduling; the failure-recovery branch is covered.",
    status: "answered",
    terminal: true,
    evidence: [{ ref: "src/retry.ts:10", claim: "scheduleRetry owns retry scheduling." }],
  },
});
```

Try to submit that result with `status: "answered"` and no `evidence` array, and `.parse()` throws — that's the whole point. For the full walk-through, including the message and acknowledgement steps this snippet leaves out, see [`GETTING_STARTED.md`](./GETTING_STARTED.md).

## How the pieces fit together

A conversation between agents in this protocol usually moves through a few kinds of object, in roughly this order:

1. **Capability declarations** (`AgentCapabilityDeclarationSchema`) — optional, and most useful when the two agents don't already know what tools each other has. One agent tells another what it can do, in an open vocabulary you define (`"github_search"`, `"grafana_metric"`, whatever fits your deployment).
2. **A task or handoff** (`AgentTaskSchema`, `AgentHandoffSchema`) — the actual assignment: an objective, and the observable conditions that make it done.
3. **Goal grounding** (`AgentGoalRestatementSchema`, `AgentGoalAckSchema`) — optional, and worth using whenever it matters that both sides understood the goal the same way before work starts. The receiver restates the goal in its own words; the sender confirms it, asks for a correction, or flags a real mismatch.
4. **Messages while the work is in progress** (`AgentMessageSchema`, built with `toAgentMessage()`) — questions, evidence, challenges, or blocking issues, each tagged with what kind of message it is.
5. **A result** (`AgentResultSchema`) — the terminal answer: a status, a citation-backed summary, and optional next steps if a different capability needs to pick this up.
6. **An acknowledgement** (`AgentResultAckSchema`) — the assigning side accepts the result, asks for a revision with a reason, or rejects it outright.

None of steps 1, 3, or 4 are mandatory — a task with a clear objective can skip straight from a handoff to a result. Nothing here requires one side to be "in charge," either. Two agents can skip the handoff entirely: they declare capabilities to each other, jointly ground a goal, and each pick up its own half of the work as equals.

Here's that same flow as a diagram, for the common case where one side assigns the work:

```text
Researcher                          Assignee

  →  AgentCapabilityDeclaration            optional: "here's what I can do"
  ←  AgentCapabilityAck
  →  AgentHandoff                          objective + successCriteria
  ←  AgentGoalRestatement                  optional: "here's my understanding"
  →  AgentGoalAck

     (the assignee accepts the handoff as its own AgentTask)

  ↔  AgentMessage                          either direction, zero or more, via toAgentMessage()
  ←  AgentResult                           evidence-backed finding
  →  AgentResultAck                        accepted / needs_revision / rejected
```

`→` reads as sent by the researcher, `←` as sent by the assignee, `↔` as either side.

For the symmetric case — neither side assigns the other — drop the `AgentHandoff` row entirely; both sides send `AgentCapabilityDeclaration` and `AgentGoalRestatement`/`AgentGoalAck` to each other, then each independently starts its own `AgentTask`.

## What this package handles, and what it deliberately leaves to you

**It owns:**

- Agent identity, task and handoff envelopes, progress and lifecycle events.
- Message frames and attachments.
- Capability declarations and acknowledgements.
- Goal restatement and grounding.
- Research findings with evidence-grading rules, and terminal outcomes.
- An extension point for your own data.
- The wire format for one specific remote transport (Agent Sync).

**It deliberately doesn't own:**

- How you register agents.
- How your runtime schedules and executes tasks.
- Persistence, authentication, or service discovery.
- Anything about your UI.

Those are all real problems, but they're yours to solve however fits your system — this package stays out of the way instead of dictating them. The `from`/`to` fields on a message are routing labels, not proof of who someone is. If you need authentication, layer it underneath this protocol, not inside it.

## How this compares to A2A, MCP, and similar protocols

If you've looked at Google's A2A, Anthropic's MCP, or similar agent-communication standards, here's where this package sits relative to them, honestly:

- This is not a competitor to any of them, and it isn't trying to be. MCP handles an agent talking to a *tool* (a database, a filesystem, an API); this package handles one *agent* talking to another agent. A2A and similar standards focus on discovery and cross-vendor transport — how a client finds a completely unrelated agent's endpoint and talks to it over HTTP. This package assumes you already have that wiring and asks a narrower question: once you connect two of your own agents, what do the actual messages say?
- Where another protocol had a genuinely good idea, this package borrows it rather than reinventing something worse. The capability-acknowledgement flow shapes itself after MCP's `initialize` handshake. The `extensions` field on every envelope follows A2A's own `extensions` array — a sanctioned place for data this package doesn't standardize, so you're never blocked from adding what your deployment needs.
- This package adds a rule other agent protocols leave undocumented: evidence-grading as an enforced check rather than a guideline. A claim you flag as high-stakes (`verificationLevel: "critical_path"`) cannot validate on a single source of evidence. That's a real constraint your code has to satisfy, not a comment reminding you to be careful.

## Package layout

This package organizes the schemas by what they're about, one concern per file:

| File | What's in it |
|---|---|
| `identity.ts` | Agent identities, lane identifiers, and the shared identifier/timestamp types everything else builds on |
| `task.ts` | Tasks, handoffs, lifecycle events, cancellation, errors, and terminal outcomes |
| `message.ts` | Messages, attachments, delivery acknowledgements, and the wire frame for Agent Sync |
| `capability.ts` | Capability declarations and their acknowledgements |
| `goal.ts` | Goal restatement, acknowledgement, and revision (for when a goal turns out to be already solved, impossible, or no longer worth pursuing) |
| `result.ts` | Results, evidence and anchor schemas, result acknowledgements, and the remote-research execution trace |
| `research.ts` | Research authority, claims (with evidence-grading), branches, and durable research continuations |
| `extensions.ts` | The extension bag shared by every top-level envelope |
| `task-fields.ts`, `limits.ts`, `cleanup.ts` | Shared field schemas and bounds used across the other files |

Say you need a specific bound (a max length, an array size) or a shared field schema (an objective, a timestamp). Import it directly from this package rather than copying the value into your own code. That way, you stay in sync when it changes.

## Core schema shapes

These are the fields you construct by hand. Every schema also carries `protocolVersion` and a `contextId`, and every one accepts an optional `extensions` bag. This section is an orientation, not the source of truth: for the full field list, including every optional field and its `.describe()` text, read the exported type or the source file named in the preceding package layout table.

**`AgentHandoffSchema`** — one agent delegating to another:

```ts
handoffId, taskId: string
from, to: AgentIdentity
objective: string
successCriteria: string[]        // 1-4 entries
createdAt: string                // ISO 8601 with an offset
```

**`AgentTaskSchema`** (the `"available"` branch) — the assignee's accepted projection of that handoff:

```ts
taskId: string
agent: AgentIdentity
assignedBy?: AgentIdentity        // usually the handoff's `from`
objective: string
successCriteria: string[]
status: "spawning" | "running" | "revising" | "done" | "failed" | "cancelled" | "expired"
createdAt: string
```

**`AgentMessageSchema`** — one message inside a running task, built with `toAgentMessage()`:

```ts
taskId, messageId: string
sequence: number                  // 1-based, strictly increasing per taskId
from, to: AgentIdentity
kind: "instruction" | "question" | "challenge" | "evidence" | "gap" | "control"
content: string
evidenceRefs?: string[]
replyTo?: string                  // the messageId this one answers
blocking?: boolean                // true only if the sender is stalled on this reply
```

**`AgentResultSchema`** — the terminal answer:

```ts
finding: {
  question?: string               // echoes the objective, when there's a plaintext one
  summary: string
  status: "answered" | "partial" | "unresolved" | "needs_lane" | "error"
  terminal: true
  evidence: { ref: string; claim: string }[]   // required when status is "answered" or "partial"
  nextSuggestedLanes?: string[]                // required when status is "needs_lane"
}
supersedesResultId?: string        // links a revision back to the result it replaces
```

**`AgentResultAckSchema`** — the assigner's response to a result:

```ts
taskId: string
verdict: "accepted" | "needs_revision" | "rejected"
reason?: string                   // required unless verdict is "accepted"
acknowledgedAt: string
```

## Where to go next

- **New to the package?** Start with [`GETTING_STARTED.md`](./GETTING_STARTED.md) — a full tutorial that builds and validates one complete exchange, end to end.
- **Building the prompt for an agent that speaks this protocol?** [`PROMPTS.md`](./PROMPTS.md) has ready-to-use system-prompt fragments for the goal-holder, tool-holder, and symmetric-peer roles.
- **Looking for a specific schema or field?** The package layout table names the file; the exported types and `.describe()` text on each field are the source of truth.
