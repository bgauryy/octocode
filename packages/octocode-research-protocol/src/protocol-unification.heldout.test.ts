import { describe, expect, it } from "vitest";

import * as Protocol from "./index.js";

type SchemaLike = {
  safeParse(value: unknown): { success: boolean };
};

const NOW = "2026-08-23T10:00:00.000Z";

function schemas(): SchemaLike[] {
  return (Object.values(Protocol) as unknown[]).filter((value): value is SchemaLike =>
    typeof value === "object"
      && value !== null
      && "safeParse" in value
      && typeof (value as { safeParse?: unknown }).safeParse === "function"
  );
}

function accepted(value: unknown): boolean {
  return schemas().some((schema) => schema.safeParse(value).success);
}

function oneSchemaAcceptsAll(values: readonly unknown[]): boolean {
  return schemas().some((schema) => values.every((value) => schema.safeParse(value).success));
}

function oneSchemaAcceptsAndRejects(valid: unknown, invalid: readonly unknown[]): boolean {
  return schemas().some((schema) =>
    schema.safeParse(valid).success
      && invalid.every((value) => !schema.safeParse(value).success)
  );
}

describe("research protocol unification: held-out cases", () => {
  it("covers a remote participant task without binding it to Agent Sync transport", () => {
    expect(accepted({
      protocolVersion: "1",
      taskId: "remote-7",
      contextId: "research-9",
      agent: "remote_research",
      objective: "Establish the owning symbol at an immutable revision.",
      semanticAvailability: "available",
      successCriteria: ["Return a source locator and resolved commit."],
      status: "spawning",
      createdAt: NOW,
    })).toBe(true);
  });

  it("covers a bounded read-only dev-machine handoff", () => {
    expect(accepted({
      protocolVersion: "1",
      handoffId: "dev-handoff-2",
      taskId: "dev-2",
      contextId: "research-9",
      from: "researcher",
      to: "dev_machine",
      objective: "Inspect the known checkout path without executing repository code.",
      successCriteria: ["Return exact file and symbol anchors."],
      authority: {
        kind: "remote_ref",
        repository: "acme/example-repo",
        requestedRef: "main",
        resolvedCommit: "abc1234",
      },
      createdAt: NOW,
    })).toBe(true);
  });

  it("uses the same lifecycle vocabulary for a configured custom capability", () => {
    expect(oneSchemaAcceptsAll(["spawning", "running", "done", "failed", "cancelled"].map(
      (status, index) => ({
        protocolVersion: "1",
        eventId: `custom-event-${index}`,
        taskId: "custom-3",
        contextId: "research-9",
        sequence: index + 1,
        agent: "billing_service",
        kind: "lifecycle",
        status,
        occurredAt: NOW,
      }),
    ))).toBe(true);
  });

  it("rejects cancellation scope escalation and unbounded reasons", () => {
    const cancellation = {
      protocolVersion: "1",
      requestId: "cancel-8",
      taskId: "custom-3",
      contextId: "research-9",
      requestedBy: "researcher",
      reason: "The evidence target is no longer material.",
      requestedAt: NOW,
    } as const;
    expect(oneSchemaAcceptsAndRejects(cancellation, [
      { ...cancellation, reason: "x".repeat(501) },
      { ...cancellation, allTasks: true },
      { ...cancellation, requestedBy: "../../operator" },
    ])).toBe(true);
  });

  it("rejects malformed lifecycle ordering fields and unknown terminal states", () => {
    const event = {
      protocolVersion: "1",
      eventId: "event-8",
      taskId: "task-8",
      contextId: "research-9",
      sequence: 8,
      agent: "dealer",
      kind: "lifecycle",
      status: "done",
      occurredAt: NOW,
    } as const;
    expect(oneSchemaAcceptsAndRejects(event, [
      { ...event, sequence: 0 },
      { ...event, status: "zombie" },
      { ...event, occurredAt: "yesterday" },
      { ...event, transportMetadata: { queue: "kafka" } },
    ])).toBe(true);
  });

  it("keeps capability traces strict while generic results remain trace-free", () => {
    const finding = {
      question: "Where is the retry policy owned?",
      summary: "The retry policy is owned by retry.ts.",
      status: "answered",
      terminal: true,
      evidence: [{ ref: "src/retry.ts:10", claim: "retry.ts defines the policy." }],
    } as const;
    expect(Protocol.AgentResultSchema.safeParse({ finding }).success).toBe(true);
    expect(Protocol.RemoteResearchResultSchema.safeParse({
      finding,
      researchTrace: {
        executionMode: "evidence_probe",
        immutablePin: "abc1234",
        checksPerformed: ["exact source read"],
        inspectedSurface: ["src/retry.ts"],
        exclusions: [],
        searchModeRequested: "octocode_local",
        searchModeUsed: "native",
        fallbackReason: "local search unavailable",
        toolFamiliesUsed: ["native_checkout"],
      },
    }).success).toBe(false);
  });
});
