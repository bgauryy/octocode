import { describe, expect, it } from "vitest";

import {
  AGENT_GOAL_REVISION_KINDS,
  AgentCapabilityDeclarationSchema,
  AgentGoalAckSchema,
  AgentGoalRestatementSchema,
  AgentGoalRevisionSchema,
  RESEARCH_PROTOCOL_VERSION,
} from "./index.js";

const NOW = "2026-08-25T10:00:00.000Z";

describe("symmetric two-peer primitives: capability declaration and goal grounding", () => {
  it("lets a peer declare an open-vocabulary capability surface, and rejects a malformed kind", () => {
    expect(AgentCapabilityDeclarationSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-1",
      contextId: "ctx-1",
      declaredBy: "researcher_a",
      capabilities: [
        { kind: "github_search", description: "Full-text and semantic code search over GitHub." },
        { kind: "kafka_read" },
      ],
      declaredAt: NOW,
    }).success).toBe(true);

    expect(AgentCapabilityDeclarationSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-2",
      contextId: "ctx-1",
      declaredBy: "researcher_b",
      capabilities: [{ kind: "GitHub Search" }],
      declaredAt: NOW,
    }).success).toBe(false);
  });

  it("lets a goal restatement happen with or without a committed taskId", () => {
    const preCommitment = AgentGoalRestatementSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      restatementId: "restate-1",
      contextId: "ctx-1",
      restatedBy: "researcher_b",
      restatedObjective: "Determine whether retry scheduling covers the failure-recovery path.",
      plannedDecomposition: ["Locate the retry scheduler.", "Check the failure-recovery branch."],
      assumptions: ["The failure-recovery path is reachable from the same entrypoint."],
      openQuestions: ["Is a separate DLQ path in scope?"],
      restatedAt: NOW,
    });
    expect(preCommitment.success).toBe(true);

    const postCommitment = AgentGoalRestatementSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      restatementId: "restate-2",
      taskId: "task-1",
      contextId: "ctx-1",
      restatedBy: "researcher_b",
      restatedObjective: "Determine whether retry scheduling covers the failure-recovery path.",
      restatedAt: NOW,
    });
    expect(postCommitment.success).toBe(true);
  });

  it("requires a reason for every goal-ack verdict except a plain confirmed", () => {
    expect(AgentGoalAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "ctx-1",
      restatementId: "restate-1",
      acknowledgedBy: "researcher_a",
      verdict: "confirmed",
      acknowledgedAt: NOW,
    }).success).toBe(true);

    expect(AgentGoalAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "ctx-1",
      restatementId: "restate-1",
      verdict: "confirmed_with_amendment",
      acknowledgedAt: NOW,
    }).success).toBe(false);
    expect(AgentGoalAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "ctx-1",
      restatementId: "restate-1",
      verdict: "confirmed_with_amendment",
      reason: "Drop the DLQ path, it's out of scope.",
      acknowledgedAt: NOW,
    }).success).toBe(true);

    expect(AgentGoalAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "ctx-1",
      restatementId: "restate-1",
      verdict: "misaligned",
      acknowledgedAt: NOW,
    }).success).toBe(false);
    expect(AgentGoalAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "ctx-1",
      restatementId: "restate-1",
      verdict: "misaligned",
      reason: "The failure-recovery path is a separate service, not the same entrypoint.",
      acknowledgedAt: NOW,
    }).success).toBe(true);
  });

  it("raisedBy has no assigner/peer distinction to enforce — any valid identity is accepted the same way", () => {
    // AgentGoalRevisionSchema deliberately carries no role concept (see role-pattern
    // research: every real precedent derives "who guides vs. who holds tools" from
    // capability declaration + message kind, never a typed role field). So this does
    // NOT prove "symmetry" the way a schema with a real assigner/peer split could —
    // there is no such split here to violate. It only proves raisedBy doesn't
    // special-case one identity over another, which is the intended absence, not a
    // tested guarantee.
    for (const raisedBy of ["researcher_a", "researcher_b"] as const) {
      expect(AgentGoalRevisionSchema.safeParse({
        protocolVersion: RESEARCH_PROTOCOL_VERSION,
        contextId: "ctx-1",
        raisedBy,
        revision: "achieved",
        reason: "The other branch already answered this.",
        raisedAt: NOW,
      }).success).toBe(true);
    }
  });

  it("pins the exact revision vocabulary, independent of what accepts/rejects it", () => {
    // Deliberately a hardcoded literal, NOT derived from AGENT_GOAL_REVISION_KINDS:
    // a fixture built from the same constant it's checking can't detect that
    // constant silently narrowing (e.g. losing "impossible") — every safeParse
    // assertion below would stay green under a narrowed enum too, since toBe(false)
    // is satisfied by a rejection for the wrong reason just as well as the right one.
    expect([...AGENT_GOAL_REVISION_KINDS].sort()).toEqual(["achieved", "impossible", "irrelevant"]);
  });

  it("always requires a reason for a goal revision, regardless of who raises it", () => {
    for (const revision of ["achieved", "impossible", "irrelevant"] as const) {
      for (const raisedBy of ["researcher_a", "researcher_b"] as const) {
        expect(AgentGoalRevisionSchema.safeParse({
          protocolVersion: RESEARCH_PROTOCOL_VERSION,
          contextId: "ctx-1",
          raisedBy,
          revision,
          raisedAt: NOW,
        }).success).toBe(false);
      }
    }
  });
});
