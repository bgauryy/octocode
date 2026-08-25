import { describe, expect, it } from "vitest";

import * as Protocol from "./index.js";

type SchemaLike = {
  safeParse(value: unknown): { success: boolean };
};

const ISO_TIME = "2026-08-23T10:00:00.000Z";

function isSchema(value: unknown): value is SchemaLike {
  return typeof value === "object"
    && value !== null
    && "safeParse" in value
    && typeof (value as { safeParse?: unknown }).safeParse === "function";
}

function exportedSchemas(): Array<[string, SchemaLike]> {
  return (Object.entries(Protocol) as Array<[string, unknown]>).filter(
    (entry): entry is [string, SchemaLike] => isSchema(entry[1]),
  );
}

function schemasAccepting(value: unknown): string[] {
  return exportedSchemas()
    .filter(([, schema]) => schema.safeParse(value).success)
    .map(([name]) => name);
}

function expectProtocolAccepts(capability: string, value: unknown): void {
  expect(
    schemasAccepting(value),
    `No exported protocol schema accepts the canonical ${capability}`,
  ).not.toEqual([]);
}

function expectOneStrictContract(
  capability: string,
  valid: unknown,
  invalid: readonly unknown[],
): void {
  const matches = exportedSchemas()
    .filter(([, schema]) => schema.safeParse(valid).success)
    .filter(([, schema]) => invalid.every((value) => !schema.safeParse(value).success))
    .map(([name]) => name);

  expect(
    matches,
    `No single exported protocol schema both accepts and strictly bounds ${capability}`,
  ).not.toEqual([]);
}

const canonicalTask = {
  protocolVersion: "1",
  taskId: "task-1",
  contextId: "research-1",
  agent: "prod_data",
  objective: "Determine whether request req-123 failed in the production lane.",
  semanticAvailability: "available",
  successCriteria: ["Return an exact request-id evidence locator."],
  status: "spawning",
  createdAt: ISO_TIME,
} as const;

const canonicalHandoff = {
  protocolVersion: "1",
  handoffId: "handoff-1",
  taskId: "task-1",
  contextId: "research-1",
  from: "researcher",
  to: "prod_data",
  objective: "Determine whether request req-123 failed in the production lane.",
  successCriteria: ["Return an exact request-id evidence locator."],
  context: "Request id: req-123; environment: production.",
  createdAt: ISO_TIME,
} as const;

const genericResult = {
  finding: {
    question: "Did request req-123 fail?",
    summary: "The request failed with a bounded timeout.",
    status: "answered",
    terminal: true,
    evidence: [{ ref: "request:req-123", claim: "The request ended with TIMEOUT." }],
    anchors: [{ kind: "request_id", value: "req-123", ref: "request:req-123" }],
  },
} as const;

describe("research protocol unification: visible regression and train cases", () => {
  it("accepts a canonical transport-neutral agent task", () => {
    expectProtocolAccepts("agent task", canonicalTask);
  });

  it("accepts a canonical transport-neutral handoff", () => {
    expectProtocolAccepts("agent handoff", canonicalHandoff);
  });

  it("normalizes the current task lifecycle in one event contract", () => {
    const lifecycle = ["spawning", "running", "done", "failed", "cancelled"].map(
      (status, index) => ({
        protocolVersion: "1",
        eventId: `event-${index + 1}`,
        taskId: "task-1",
        contextId: "research-1",
        sequence: index + 1,
        agent: "prod_data",
        kind: "lifecycle",
        status,
        occurredAt: ISO_TIME,
      }),
    );
    const matches = exportedSchemas()
      .filter(([, schema]) => lifecycle.every((event) => schema.safeParse(event).success))
      .map(([name]) => name);

    expect(matches, "No exported event schema owns every current lifecycle state").not.toEqual([]);
  });

  it("accepts generic built-in, custom, remote, and dev-machine identities", () => {
    const identities = [
      "researcher", "prod_data", "petri", "dealer", "code",
      "billing_service", "remote_research", "dev_machine",
    ];
    expect(identities.every((identity) => Protocol.AgentIdentitySchema.safeParse(identity).success))
      .toBe(true);
  });

  it("retains one generic result contract for capability-neutral findings", () => {
    expect(Protocol.AgentResultSchema.safeParse(genericResult).success).toBe(true);
  });

  it("retains a capability-specific remote repository trace", () => {
    expect(Protocol.AgentResultSchema.safeParse({
      ...genericResult,
      researchTrace: {
        executionMode: "evidence_probe",
        immutablePin: "abc1234",
        checksPerformed: ["semantic reference lookup"],
        inspectedSurface: ["src/request.ts"],
        exclusions: ["live production state"],
        searchModeRequested: "octocode_local",
        searchModeUsed: "octocode_local",
        toolFamiliesUsed: ["octocode_local"],
      },
    }).success).toBe(true);
  });

  it("accepts a task-scoped cancellation command", () => {
    expectProtocolAccepts("cancellation command", {
      protocolVersion: "1",
      requestId: "cancel-1",
      taskId: "task-1",
      contextId: "research-1",
      requestedBy: "researcher",
      reason: "The branch was superseded by immutable evidence.",
      requestedAt: ISO_TIME,
    });
  });

  it("accepts a structured recoverable agent error", () => {
    expectProtocolAccepts("agent error", {
      protocolVersion: "1",
      errorId: "error-1",
      taskId: "task-1",
      contextId: "research-1",
      agent: "prod_data",
      code: "UPSTREAM_TIMEOUT",
      message: "The production-data query exceeded its deadline.",
      recoverable: true,
      occurredAt: ISO_TIME,
    });
  });

  it("retains a bounded delivery acknowledgement", () => {
    expect(Protocol.AgentDeliveryAckSchema.safeParse({
      protocolVersion: Protocol.RESEARCH_PROTOCOL_VERSION,
      accepted: false,
      delivery: "turn_boundary",
      taskId: "task-1",
      acknowledgedAt: ISO_TIME,
      reason: "The agent inbox is full.",
    }).success).toBe(true);
    expect(Protocol.AgentDeliveryAckSchema.safeParse({
      protocolVersion: Protocol.RESEARCH_PROTOCOL_VERSION,
      accepted: false,
      delivery: "turn_boundary",
      taskId: "task-1",
      acknowledgedAt: ISO_TIME,
      reason: "x".repeat(1_001),
    }).success).toBe(false);
  });

  it("strictly bounds the canonical handoff at its protocol boundary", () => {
    expectOneStrictContract("agent handoff", canonicalHandoff, [
      { ...canonicalHandoff, objective: "x".repeat(4_001) },
      { ...canonicalHandoff, successCriteria: Array.from({ length: 5 }, () => "observable") },
      { ...canonicalHandoff, transport: "http" },
    ]);
  });
});
