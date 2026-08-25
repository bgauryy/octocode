import { describe, expect, it } from "vitest";

import {
  AgentAttachmentSchema,
  AgentCancellationAckSchema,
  AgentCapabilityAckSchema,
  AgentCapabilityDeclarationSchema,
  AgentDeliveryAckSchema,
  AgentExtensionsSchema,
  AgentGoalAckSchema,
  AgentGoalRestatementSchema,
  AgentLaneIdentifierSchema,
  AgentLifecycleEventSchema,
  AgentMessageInputSchema,
  AgentMessageSchema,
  AgentResultAckSchema,
  AgentResultNextLanesSchema,
  AgentResultSchema,
  AgentTaskBudgetSchema,
  AgentTaskSchema,
  AgentTerminalOutcomeSchema,
  RESEARCH_PROTOCOL_VERSION,
  RemoteResearchResultSchema,
  RemoteResearchTraceSchema,
  ResearchAuthoritySchema,
  ResearchBranchSchema,
  ResearchClaimSchema,
  ResearchContinuationSchema,
  TransportResearchAuthoritySchema,
  isAgentResultEvidenceLocator,
  toAgentMessage,
} from "./index.js";

const NOW = "2026-08-25T10:00:00.000Z";

describe("protocol hardening: fixes from the multi-agent audit", () => {
  it("lets a lane be a structured capability name, distinct from a bare agent identity", () => {
    expect(AgentLaneIdentifierSchema.safeParse("octocode.search").success).toBe(true);
    expect(AgentLaneIdentifierSchema.safeParse("repo:search").success).toBe(true);
    expect(AgentLaneIdentifierSchema.safeParse("../../unsafe").success).toBe(false);
    expect(AgentResultNextLanesSchema.safeParse(["octocode.search", "repo:search"]).success).toBe(true);
  });

  it("types objective as always-present for an available task and always-absent for a recovered one", () => {
    const available = AgentTaskSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-1",
      contextId: "ctx-1",
      agent: "researcher",
      assignedBy: "orchestrator",
      objective: "Trace the retry owner.",
      semanticAvailability: "available",
      successCriteria: [],
      status: "running",
      createdAt: NOW,
    });
    if (available.semanticAvailability !== "unavailable_after_recovery") {
      expect(available.objective).toBe("Trace the retry owner.");
    }

    const recovered = AgentTaskSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-2",
      contextId: "ctx-1",
      agent: "researcher",
      semanticAvailability: "unavailable_after_recovery",
      successCriteria: [],
      status: "running",
      createdAt: NOW,
    });
    if (recovered.semanticAvailability === "unavailable_after_recovery") {
      expect(recovered.objective).toBeUndefined();
    }

    expect(AgentTaskSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-3",
      contextId: "ctx-1",
      agent: "researcher",
      semanticAvailability: "unavailable_after_recovery",
      objective: "leaked plaintext",
      successCriteria: [],
      status: "running",
      createdAt: NOW,
    }).success).toBe(false);

    // A real discriminatedUnion, not a plain union: the discriminant is required,
    // so a task that omits semanticAvailability entirely is rejected outright
    // rather than silently defaulting to one branch.
    expect(AgentTaskSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-4",
      contextId: "ctx-1",
      agent: "researcher",
      objective: "Trace the retry owner.",
      successCriteria: [],
      status: "running",
      createdAt: NOW,
    }).success).toBe(false);
  });

  it("adapts a bounded AgentMessageInput into an addressed, sequenced AgentMessage in one call", () => {
    const message = toAgentMessage(
      AgentMessageInputSchema.parse({
        kind: "question",
        content: "Which lane owns retries?",
        blocking: true,
      }),
      {
        taskId: "task-1",
        contextId: "ctx-1",
        messageId: "msg-1",
        sequence: 1,
        from: "researcher",
        to: "remote_research",
        sentAt: NOW,
      },
    );
    expect(AgentMessageSchema.safeParse(message).success).toBe(true);
    expect(message.blocking).toBe(true);
  });

  it("uses an open, deployment-neutral search-mode vocabulary while keeping the fallback invariants", () => {
    const base = {
      executionMode: "research" as const,
      checksPerformed: ["exact read"],
      searchModeRequested: "indexed_local",
      searchModeUsed: "indexed_local",
      toolFamiliesUsed: ["indexed_local"],
    };
    expect(RemoteResearchTraceSchema.safeParse(base).success).toBe(true);
    expect(RemoteResearchTraceSchema.safeParse({
      ...base,
      executionMode: "evidence_probe",
      searchModeUsed: "filesystem_walk",
      fallbackReason: "index unavailable",
      toolFamiliesUsed: ["filesystem_walk"],
    }).success).toBe(false);
    expect(RemoteResearchTraceSchema.safeParse({
      ...base,
      searchModeUsed: "filesystem_walk",
      toolFamiliesUsed: ["filesystem_walk"],
    }).success).toBe(false);
  });

  it("gives the assigner a way to accept, request revision on, or reject a terminal result", () => {
    expect(AgentResultAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-1",
      contextId: "ctx-1",
      verdict: "accepted",
      acknowledgedAt: NOW,
    }).success).toBe(true);
    expect(AgentResultAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-1",
      contextId: "ctx-1",
      verdict: "needs_revision",
      acknowledgedAt: NOW,
    }).success).toBe(false);
    expect(AgentResultAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-1",
      contextId: "ctx-1",
      acknowledgedBy: "researcher",
      verdict: "needs_revision",
      reason: "Evidence doesn't cover the fallback path.",
      acknowledgedAt: NOW,
    }).success).toBe(true);
  });

  it("lets a still-running task hand back evidence found so far without ending it", () => {
    expect(AgentLifecycleEventSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      eventId: "event-1",
      taskId: "task-1",
      contextId: "ctx-1",
      sequence: 1,
      agent: "remote_research",
      kind: "lifecycle",
      status: "running",
      interimEvidence: [{ ref: "src/retry.ts:10", claim: "scheduleRetry owns retries." }],
      occurredAt: NOW,
    }).success).toBe(true);
  });

  it("communicates a soft budget on a task and reports actual consumption on a result", () => {
    expect(AgentTaskBudgetSchema.safeParse({ maxSteps: 20, maxCostUsd: 2.5 }).success).toBe(true);
    expect(AgentResultSchema.safeParse({
      finding: {
        question: "Who owns retries?",
        summary: "retry.ts owns retries.",
        status: "answered",
        terminal: true,
        evidence: [{ ref: "src/retry.ts:10", claim: "scheduleRetry owns retries." }],
      },
      consumption: { steps: 6, costUsd: 0.12 },
    }).success).toBe(true);
  });

  it("rejects a locator-shaped sentence even when a word inside it looks like a file path", () => {
    expect(isAgentResultEvidenceLocator("the parse call in result.ts")).toBe(false);
    expect(isAgentResultEvidenceLocator("It is enforced by result.ts line 137.")).toBe(false);
    expect(isAgentResultEvidenceLocator("src/result.ts:137")).toBe(true);
  });

  it("requires a research continuation to identify the session it resumes", () => {
    expect(ResearchContinuationSchema.safeParse({
      authority: { kind: "unspecified", reason: "no authority yet" },
      claims: [],
      branches: [],
      durableAnchors: [],
      contradictions: [],
    }).success).toBe(false);
    expect(ResearchContinuationSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "ctx-1",
      savedAt: NOW,
      authority: { kind: "unspecified", reason: "no authority yet" },
      claims: [],
      branches: [],
      durableAnchors: [],
      contradictions: [],
    }).success).toBe(true);
  });

  it("gives a needs_revision verdict a real path back: a distinct task status and a link between results", () => {
    const reopened = AgentTaskSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-1",
      contextId: "ctx-1",
      agent: "remote_research",
      objective: "Re-anchor the ownership evidence.",
      semanticAvailability: "available",
      successCriteria: [],
      status: "revising",
      createdAt: NOW,
    });
    expect(reopened.status).toBe("revising");

    const revisedResult = AgentResultSchema.parse({
      finding: {
        question: "Who owns retry scheduling?",
        summary: "retry.ts owns retry scheduling; re-anchored to the module definition.",
        status: "answered",
        terminal: true,
        evidence: [{ ref: "src/retry.ts:1", claim: "The owning module is defined here." }],
      },
      supersedesResultId: "result-1",
    });
    expect(revisedResult.supersedesResultId).toBe("result-1");
  });

  it("gives a delivery ack an author and a timestamp, matching every sibling ack's mesh-attribution rationale", () => {
    expect(AgentDeliveryAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      accepted: true,
      delivery: "restart",
      taskId: "task-1",
      messageId: "msg-1",
      acknowledgedBy: "researcher",
    }).success).toBe(false);
    expect(AgentDeliveryAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      accepted: true,
      delivery: "restart",
      taskId: "task-1",
      messageId: "msg-1",
      acknowledgedBy: "researcher",
      acknowledgedAt: NOW,
    }).success).toBe(true);
  });

  it("lets a capability declaration or goal restatement target one specific peer, or broadcast by omitting it", () => {
    const broadcast = AgentCapabilityDeclarationSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-1",
      contextId: "ctx-1",
      declaredBy: "researcher_a",
      capabilities: [{ kind: "sqlite_read" }],
      declaredAt: NOW,
    });
    expect(broadcast.declaredTo).toBeUndefined();
    const targeted = AgentCapabilityDeclarationSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-2",
      contextId: "ctx-1",
      declaredBy: "researcher_a",
      declaredTo: "researcher_b",
      capabilities: [{ kind: "sqlite_read" }],
      declaredAt: NOW,
    });
    expect(targeted.declaredTo).toBe("researcher_b");

    expect(AgentGoalRestatementSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      restatementId: "restate-1",
      contextId: "ctx-1",
      restatedBy: "researcher_b",
      restatedTo: "researcher_a",
      restatedObjective: "Confirm whether checkout has an unresolved incident.",
      restatedAt: NOW,
    }).success).toBe(true);
  });

  it("lets a non-repo capability (e.g. a database query) cross the remote-research boundary without repo-shaped field names", () => {
    // The trace's field names (searchModeRequested/searchModeUsed) and vocabulary
    // (RemoteSearchModeSchema, an open string) are both capability-neutral now —
    // a DB-query trace no longer has to overload repo-search-flavored fields.
    expect(RemoteResearchResultSchema.safeParse({
      finding: {
        question: "Is there an unresolved checkout incident?",
        summary: "Yes — two unresolved RETRY_EXHAUSTED incidents.",
        status: "answered",
        terminal: true,
        evidence: [{ ref: "research.db:incidents.id=4", claim: "Unresolved checkout incident." }],
      },
      researchTrace: {
        executionMode: "evidence_probe",
        immutablePin: "abc1234",
        checksPerformed: ["sqlite query"],
        searchModeRequested: "sqlite_read",
        searchModeUsed: "sqlite_read",
        toolFamiliesUsed: ["sqlite_read"],
      },
    }).success).toBe(true);
  });

  it("lets a research claim be about any domain, not just code", () => {
    const base = {
      id: "claim-1",
      text: "The incident-response SLA was breached for this ticket.",
      status: "open" as const,
      evidenceRefs: [],
      authorityRequired: "on-call incident timeline",
    };
    for (const kind of ["legal_review", "incident_response", "financial_reconciliation", "code_behavior"]) {
      expect(ResearchClaimSchema.safeParse({ ...base, kind }).success).toBe(true);
    }
    expect(ResearchClaimSchema.safeParse({ ...base, kind: "Not An Identifier!" }).success).toBe(false);
  });

  it("pins an answered remote result to any immutable identifier, not just a git commit", () => {
    // No fixed shape: a SQLite content hash, not a hex SHA — this is the exact
    // fabrication risk closed: previously a non-git result had to invent a fake
    // git-shaped hash ("abc1234") to pass; now it can state what it actually has.
    expect(RemoteResearchResultSchema.safeParse({
      finding: {
        question: "Is there an unresolved checkout incident?",
        summary: "Yes — two unresolved RETRY_EXHAUSTED incidents.",
        status: "answered",
        terminal: true,
        evidence: [{ ref: "research.db:incidents.id=4", claim: "Unresolved checkout incident." }],
      },
      researchTrace: {
        executionMode: "evidence_probe",
        immutablePin: "sqlite:research.db#sha256=9f86d081884c7d659a2feaa0c55ad015",
        checksPerformed: ["sqlite query"],
        searchModeRequested: "sqlite_read",
        searchModeUsed: "sqlite_read",
        toolFamiliesUsed: ["sqlite_read"],
      },
    }).success).toBe(true);
    // Still required for "answered" — only the shape was generalized, not the invariant.
    expect(RemoteResearchResultSchema.safeParse({
      finding: {
        question: "Is there an unresolved checkout incident?",
        summary: "Yes.",
        status: "answered",
        terminal: true,
        evidence: [{ ref: "research.db:incidents.id=4", claim: "Unresolved checkout incident." }],
      },
      researchTrace: {
        executionMode: "evidence_probe",
        checksPerformed: ["sqlite query"],
        searchModeRequested: "sqlite_read",
        searchModeUsed: "sqlite_read",
        toolFamiliesUsed: ["sqlite_read"],
      },
    }).success).toBe(false);
  });

  it("lets research authority pin a local non-git artifact (a file/database/dataset), not just a git checkout", () => {
    expect(ResearchAuthoritySchema.safeParse({
      kind: "local_artifact",
      path: "/private/tmp/live-protocol-session/research.db",
      fingerprint: "sha256:9f86d081884c7d659a2feaa0c55ad015",
    }).success).toBe(true);
    expect(ResearchAuthoritySchema.safeParse({
      kind: "local_artifact",
      path: "/private/tmp/live-protocol-session/research.db",
    }).success).toBe(true);
    // Same trust boundary as local_checkout: a network surface can't verify a local path.
    expect(TransportResearchAuthoritySchema.safeParse({
      kind: "local_artifact",
      path: "/private/tmp/live-protocol-session/research.db",
    }).success).toBe(false);
  });

  it("lets a research branch be deliberately abandoned, distinct from stuck, and requires why", () => {
    const base = {
      id: "branch-1",
      claimIds: ["claim-1"],
      objective: "Check whether the failure-recovery path also regressed.",
      dependencyIds: [],
      lane: "code",
      expectedEvidence: "A call site in the failure-recovery branch.",
    };
    expect(ResearchBranchSchema.safeParse({ ...base, status: "blocked" }).success).toBe(true);
    expect(ResearchBranchSchema.safeParse({ ...base, status: "abandoned" }).success).toBe(false);
    expect(ResearchBranchSchema.safeParse({
      ...base,
      status: "abandoned",
      reason: "Live metrics already proved the failure-recovery path is unaffected; no code-level check needed.",
    }).success).toBe(true);
  });

  it("lets a peer accept, partially accept, or decline a capability declaration, without granting authorization", () => {
    expect(AgentCapabilityAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-1",
      contextId: "ctx-1",
      verdict: "accepted",
      acknowledgedAt: NOW,
    }).success).toBe(true);
    expect(AgentCapabilityAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-1",
      contextId: "ctx-1",
      verdict: "partially_accepted",
      relyingOn: ["github_search"],
      reason: "Already have a warm npm mirror; won't need npm_registry_lookup.",
      acknowledgedAt: NOW,
    }).success).toBe(true);
    // partially_accepted requires a non-empty relyingOn
    expect(AgentCapabilityAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-1",
      contextId: "ctx-1",
      verdict: "partially_accepted",
      reason: "subset unspecified",
      acknowledgedAt: NOW,
    }).success).toBe(false);
    // declined requires a reason
    expect(AgentCapabilityAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-1",
      contextId: "ctx-1",
      verdict: "declined",
      acknowledgedAt: NOW,
    }).success).toBe(false);
    // relyingOn is only meaningful for partially_accepted
    expect(AgentCapabilityAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      declarationId: "decl-1",
      contextId: "ctx-1",
      verdict: "accepted",
      relyingOn: ["github_search"],
      acknowledgedAt: NOW,
    }).success).toBe(false);
  });

  it("lets a message point at a produced artifact via a digest-committed pointer, never raw bytes", () => {
    const attachment = AgentAttachmentSchema.parse({
      kind: "image",
      ref: "s3://octocode-artifacts/run-8812/screenshot.png",
      digest: "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
      mimeType: "image/png",
      description: "Screenshot of the failing checkout page.",
    });
    expect(AgentMessageInputSchema.safeParse({
      kind: "evidence",
      content: "Here's the failing screenshot.",
      attachments: [attachment],
    }).success).toBe(true);
    const message = toAgentMessage(
      AgentMessageInputSchema.parse({ kind: "evidence", content: "See attached.", attachments: [attachment] }),
      { taskId: "t1", contextId: "c1", messageId: "m1", sequence: 1, from: "researcher_a", to: "researcher_b", sentAt: NOW },
    );
    expect(AgentMessageSchema.safeParse(message).success).toBe(true);
    expect(message.attachments).toEqual([attachment]);
    // digest is required — this is what distinguishes an attachment from a bare evidenceRef
    expect(AgentAttachmentSchema.safeParse({
      kind: "image",
      ref: "s3://octocode-artifacts/run-8812/screenshot.png",
      mimeType: "image/png",
    }).success).toBe(false);
    // no raw bytes: an oversized base64-shaped ref/description is not a special case, it's just
    // rejected by the same bounds as everything else — no separate "bytes" field exists to carry it
    expect(AgentAttachmentSchema.safeParse({
      kind: "image",
      ref: "x".repeat(3_000),
      digest: "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    }).success).toBe(false);
  });

  it("lets a consumer attach deployment-specific data via extensions without weakening core strictness", () => {
    const base = {
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "task-1",
      contextId: "ctx-1",
      agent: "researcher",
      objective: "Trace the retry owner.",
      semanticAvailability: "available" as const,
      successCriteria: [],
      status: "running" as const,
      createdAt: NOW,
    };
    // A bare custom field at the top level is still rejected — typo protection on known
    // fields is exactly what .strict() is for and extensions doesn't weaken that.
    expect(AgentTaskSchema.safeParse({ ...base, internalTraceId: "trace-abc123" }).success).toBe(false);
    // The same data, moved into the sanctioned bag, is accepted.
    expect(AgentTaskSchema.safeParse({
      ...base,
      extensions: { internal_trace_id: "trace-abc123", billing_tag: "team-quasar" },
    }).success).toBe(true);
    // Still bounded — not an unlimited escape hatch.
    expect(AgentExtensionsSchema.safeParse(
      Object.fromEntries(Array.from({ length: 20 }, (_, i) => [`k${i}`, "v"])),
    ).success).toBe(false);
    expect(AgentExtensionsSchema.safeParse({ "Not An Identifier!": "v" }).success).toBe(false);
  });

  it("extends the extensions escape hatch to every top-level ack/event envelope, not just the four core ones", () => {
    expect(AgentGoalAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "ctx-1",
      verdict: "confirmed",
      acknowledgedAt: NOW,
      extensions: { trace_id: "abc" },
    }).success).toBe(true);
    expect(AgentCancellationAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      requestId: "cancel-1",
      taskId: "task-1",
      accepted: true,
      status: "cancelled",
      acknowledgedAt: NOW,
      extensions: { trace_id: "abc" },
    }).success).toBe(true);
    expect(AgentTerminalOutcomeSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      outcomeId: "outcome-1",
      taskId: "task-1",
      contextId: "ctx-1",
      from: "researcher_a",
      to: "researcher_b",
      status: "cancelled",
      reason: "Superseded by a faster lead.",
      occurredAt: NOW,
      extensions: { trace_id: "abc" },
    }).success).toBe(true);
  });

  it("never lets a critical_path claim be concluded from one evidence grade alone, across heterogeneous subagent tools", () => {
    // Scenario: a main researcher orchestrates subagents with different tools
    // (octocode/code search, Grafana metrics, web search) — operationalizes the
    // routing/evidence position paper's one non-negotiable rule as an enforced
    // invariant, not a documented best practice.
    const base = {
      id: "claim-retry-storm",
      text: "The retry storm was caused by the payment-gateway timeout regression.",
      kind: "incident_response",
      status: "supported" as const,
      authorityRequired: "local_checkout of the merged PR",
    };
    // One subagent, one grade, high-stakes claim: rejected.
    expect(ResearchClaimSchema.safeParse({
      ...base,
      verificationLevel: "critical_path",
      evidenceRefs: ["src/payments/gateway-client.ts:142"],
      evidenceGrades: ["semantic"],
    }).success).toBe(false);
    // Two subagents, two distinct grades: accepted.
    expect(ResearchClaimSchema.safeParse({
      ...base,
      verificationLevel: "critical_path",
      evidenceRefs: ["src/payments/gateway-client.ts:142", "grafana:dash-42/panel-7"],
      evidenceGrades: ["semantic", "grafana_metric"],
    }).success).toBe(true);
    // An ordinary (non-critical_path) claim needs no grade declaration at all — the
    // guardrail against over-strictness: this rule only bites on the high-stakes case.
    expect(ResearchClaimSchema.safeParse({
      ...base,
      evidenceRefs: ["src/payments/gateway-client.ts:142"],
    }).success).toBe(true);
    // The grade vocabulary is genuinely open — not the position paper's 4-value
    // taxonomy re-hardcoded; a tool that taxonomy never anticipated still works.
    expect(ResearchClaimSchema.safeParse({
      ...base,
      verificationLevel: "critical_path",
      evidenceRefs: ["src/payments/gateway-client.ts:142", "https://status.example.com/incidents/882"],
      evidenceGrades: ["semantic", "web_search"],
    }).success).toBe(true);
  });
});
