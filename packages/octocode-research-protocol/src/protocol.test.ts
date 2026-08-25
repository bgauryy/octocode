import { describe, expect, it } from "vitest";

import {
  AGENT_SYNC_CLEANUP_STAGES,
  AGENT_MESSAGE_KINDS,
  AGENT_RESULT_STATUSES,
  AgentSyncCleanupStageSchema,
  AgentDeliveryAckSchema,
  AgentCancellationAckSchema,
  AgentTaskSchema,
  AgentTaskObjectiveSchema,
  AgentTaskContextSchema,
  AgentTaskSuccessCriteriaSchema,
  AgentResultEvidenceSchema,
  AgentResultAnchorSchema,
  AgentResultNextLanesSchema,
  AgentMessageInputSchema,
  AgentMessageSchema,
  AgentResultSchema,
  RemoteResearchResultSchema,
  AgentSyncMessageFrameSchema,
  AgentSyncResultFrameSchema,
  ResearchAuthoritySchema,
  ResearchClaimSchema,
  ResearchContinuationSchema,
  RESEARCH_CONTINUATION_MAX_SERIALIZED_CHARS,
  RESEARCH_PROTOCOL_VERSION,
  findingRequiresEvidence,
  findingRequiresNextLanes,
  boundProtocolText,
} from "./index.js";

describe("octocode research protocol", () => {
  const message = {
    protocolVersion: RESEARCH_PROTOCOL_VERSION,
    taskId: "a1",
    contextId: "session-1",
    messageId: "m1",
    sequence: 1,
    from: "researcher",
    to: "billing_service",
    kind: "evidence",
    content: "The domain is externally managed.",
    evidenceRefs: ["domains_get:example.com"],
    sentAt: "2026-08-12T10:00:00.000Z",
  } as const;

  it("owns the provider-neutral Agent Sync cleanup stages", () => {
    expect(AgentSyncCleanupStageSchema.options).toEqual([
      AGENT_SYNC_CLEANUP_STAGES.CANCEL_REMOTE_TASK,
      AGENT_SYNC_CLEANUP_STAGES.CLOSE_SESSION,
      AGENT_SYNC_CLEANUP_STAGES.CLEANUP_REMOTE_TASK,
    ]);
    expect(AgentSyncCleanupStageSchema.safeParse("delete_factory_secrets").success).toBe(false);
  });

  it("owns one strict, versioned semantic message frame", () => {
    expect(AgentMessageSchema.parse(message)).toEqual(message);
    expect(AgentMessageSchema.safeParse({ ...message, extra: true }).success).toBe(false);
    expect(AgentMessageSchema.safeParse({ ...message, sequence: 0 }).success).toBe(false);
  });

  it("uses the same bounded semantic input for every transport", () => {
    expect(AgentMessageInputSchema.parse({ content: "Need the exact request id" })).toEqual({
      kind: "instruction",
      content: "Need the exact request id",
      evidenceRefs: [],
    });
    expect(AgentMessageInputSchema.safeParse({
      kind: "question",
      content: "Which account owns it?",
      evidenceRefs: ["x".repeat(2_048)],
    }).success).toBe(true);
    expect(AgentMessageInputSchema.safeParse({
      kind: "question",
      content: "Which account owns it?",
      evidenceRefs: ["x".repeat(2_049)],
    }).success).toBe(false);
    expect(AgentMessageInputSchema.safeParse({ content: "   " }).success).toBe(false);
    expect(AgentMessageInputSchema.safeParse({ content: "ok", evidenceRefs: ["  "] }).success).toBe(false);
    expect(AgentMessageInputSchema.safeParse({ content: "ok", replyTo: "  " }).success).toBe(false);
    expect(AGENT_MESSAGE_KINDS).toEqual([
      "instruction", "question", "challenge", "evidence", "gap", "control",
    ]);
  });

  it("represents every runtime delivery acknowledgement", () => {
    expect(AgentDeliveryAckSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      accepted: true,
      delivery: "turn_boundary",
      taskId: "a1",
      messageId: "m1",
      acknowledgedAt: "2026-08-12T10:00:00.000Z",
    })).toMatchObject({ accepted: true, delivery: "turn_boundary" });
    expect(["restart", "remote"].map((delivery) => AgentDeliveryAckSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION, accepted: true, delivery, taskId: "a1", messageId: "m1",
      acknowledgedAt: "2026-08-12T10:00:00.000Z",
    }).delivery)).toEqual(["restart", "remote"]);
    expect(AgentDeliveryAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      accepted: false,
      delivery: "turn_boundary",
      taskId: "a1",
      acknowledgedAt: "2026-08-12T10:00:00.000Z",
    }).success).toBe(false);
    expect(AgentDeliveryAckSchema.safeParse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      accepted: false,
      delivery: "turn_boundary",
      taskId: "a1",
      reason: "inbox at capacity",
      acknowledgedAt: "2026-08-12T10:00:00.000Z",
    }).success).toBe(true);
  });

  it("requires a reason when a cancellation request is rejected", () => {
    const acknowledgement = {
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      requestId: "cancel-1",
      taskId: "task-1",
      accepted: false,
      status: "running",
      acknowledgedAt: "2026-08-12T10:00:00.000Z",
    } as const;
    expect(AgentCancellationAckSchema.safeParse(acknowledgement).success).toBe(false);
    expect(AgentCancellationAckSchema.safeParse({
      ...acknowledgement,
      reason: "provider cancellation is still reconciling",
    }).success).toBe(true);
  });

  it("requires task semantics unless durable recovery explicitly marks them unavailable", () => {
    const recoveredTask = {
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      taskId: "remote-1",
      contextId: "research-1",
      agent: "remote_research",
      semanticAvailability: "unavailable_after_recovery",
      successCriteria: [],
      status: "running",
      createdAt: "2026-08-12T10:00:00.000Z",
    } as const;
    expect(AgentTaskSchema.parse(recoveredTask)).toEqual(recoveredTask);
    const { semanticAvailability: _marker, ...unmarked } = recoveredTask;
    expect(AgentTaskSchema.safeParse(unmarked).success).toBe(false);
  });

  it("exports one set of reusable task and result field owners", () => {
    expect(AgentTaskObjectiveSchema.parse("Trace the retry owner.")).toBe("Trace the retry owner.");
    expect(AgentTaskContextSchema.safeParse("x".repeat(16_001)).success).toBe(false);
    expect(AgentTaskSuccessCriteriaSchema.safeParse(["x".repeat(601)]).success).toBe(false);
    expect(AgentResultEvidenceSchema.parse({
      ref: "src/retry.ts:10",
      claim: "scheduleRetry owns retry scheduling.",
    })).toBeDefined();
    expect(AgentResultAnchorSchema.parse({
      kind: "path",
      value: "src/retry.ts",
      ref: "src/retry.ts:10",
    })).toBeDefined();
    expect(AgentResultNextLanesSchema.parse(["code"])).toEqual(["code"]);
  });

  it("owns the strict Agent Sync transport frame", () => {
    const frame = {
      schemaVersion: 1,
      sessionId: "session-1",
      messageId: "message-1",
      sequence: 1,
      sender: "REMOTE_RESEARCH",
      kind: "EVIDENCE",
      content: "Verified at src/index.ts:10.",
      evidenceRefs: ["src/index.ts:10"],
      sentAt: "2026-08-12T10:00:00.000Z",
    } as const;
    expect(AgentSyncMessageFrameSchema.parse(frame)).toEqual(frame);
    expect(AgentSyncMessageFrameSchema.safeParse({ ...frame, sender: "CALLER" }).success).toBe(false);
    expect(AgentSyncMessageFrameSchema.safeParse({ ...frame, extra: true }).success).toBe(false);
  });

  it("owns the strict remote terminal result and durable frame", () => {
    const result = {
      finding: {
        question: "Who owns retries?",
        summary: "REF: main @ abc1234\nVERDICT: retry.ts owns retries",
        status: "answered",
        terminal: true,
        evidence: [{ ref: "src/retry.ts:10", claim: "scheduleRetry owns retries." }],
      },
      researchTrace: {
        executionMode: "evidence_probe",
        immutablePin: "abc1234",
        checksPerformed: ["exact read"],
        inspectedSurface: ["src/retry.ts"],
        exclusions: [],
        searchModeRequested: "octocode_local",
        searchModeUsed: "octocode_local",
        toolFamiliesUsed: ["octocode_local"],
      },
    } as const;
    expect(AgentResultSchema.parse(result)).toEqual(result);
    expect(AgentSyncResultFrameSchema.parse({
      schemaVersion: 1,
      sessionId: "session-1",
      resultId: "result-1",
      sender: "REMOTE_RESEARCH",
      result,
      submittedAt: "2026-08-12T10:00:00.000Z",
    }).result).toEqual(result);
    expect(AgentResultSchema.safeParse({
      finding: { ...result.finding, status: "answered", evidence: [] },
    }).success).toBe(false);
    expect(AgentResultSchema.safeParse({
      finding: { ...result.finding, terminal: false },
    }).success).toBe(false);
    for (const ref of ["unknown", "none", "n/a", "unavailable", "unverified", "unresolved", "missing", "tbd"]) {
      expect(AgentResultSchema.safeParse({
        finding: { ...result.finding, evidence: [{ ref, claim: "placeholder" }] },
      }).success, ref).toBe(false);
    }
  });

  it("records the remote BCA repository-search trace", () => {
    const finding = {
      question: "Who owns retries?",
      summary: "retry.ts owns retries",
      status: "answered" as const,
      terminal: true as const,
      evidence: [{ ref: "src/retry.ts:10", claim: "scheduleRetry owns retries." }],
    };
    expect(RemoteResearchResultSchema.safeParse({
      finding,
      researchTrace: {
        executionMode: "evidence_probe",
        immutablePin: "abc1234",
        checksPerformed: ["semantic references"],
        inspectedSurface: ["src/retry.ts and its semantic references"],
        exclusions: ["runtime state"],
        searchModeRequested: "octocode_local",
        searchModeUsed: "octocode_local",
        toolFamiliesUsed: ["octocode_local"],
      },
    }).success).toBe(true);
    expect(RemoteResearchResultSchema.safeParse({
      finding,
      researchTrace: {
        executionMode: "evidence_probe",
        immutablePin: "abc1234",
        checksPerformed: ["exact read"],
        inspectedSurface: ["src/retry.ts"],
        exclusions: [],
        searchModeRequested: "octocode_local",
        searchModeUsed: "native",
        fallbackReason: "Octocode local failed to initialize",
        toolFamiliesUsed: ["native_checkout"],
      },
    }).success).toBe(false);
    expect(RemoteResearchResultSchema.safeParse({
      finding,
      researchTrace: {
        executionMode: "research",
        immutablePin: "abc1234",
        checksPerformed: ["exact read"],
        inspectedSurface: ["src/retry.ts"],
        exclusions: [],
        searchModeRequested: "octocode_local",
        searchModeUsed: "native",
        toolFamiliesUsed: ["native_checkout"],
      },
    }).success).toBe(false);
    expect(RemoteResearchResultSchema.safeParse({
      finding,
      researchTrace: {
        executionMode: "research",
        immutablePin: "abc1234",
        checksPerformed: ["exact read"],
        inspectedSurface: ["src/retry.ts"],
        exclusions: [],
        searchModeRequested: "native",
        searchModeUsed: "octocode_local",
        toolFamiliesUsed: ["octocode_local"],
      },
    }).success).toBe(false);
    expect(RemoteResearchResultSchema.safeParse({
      finding,
      researchTrace: {
        executionMode: "evidence_probe",
        checksPerformed: ["semantic references"],
        inspectedSurface: ["src/retry.ts"],
        exclusions: [],
        searchModeRequested: "octocode_local",
        searchModeUsed: "octocode_local",
        toolFamiliesUsed: ["octocode_local"],
      },
    }).success).toBe(false);
  });

  it("allows configured custom-role routing hints without accepting unsafe identifiers", () => {
    const finding = {
      question: "Which lane owns premium domains?",
      summary: "The configured billing_service role owns the next check.",
      status: "needs_lane",
      terminal: true,
      evidence: [],
      nextSuggestedLanes: ["billing_service"],
    } as const;
    expect(AgentResultSchema.safeParse({ finding }).success).toBe(true);
    expect(AgentResultSchema.safeParse({
      finding: { ...finding, nextSuggestedLanes: ["../../unsafe"] },
    }).success).toBe(false);
  });

  it("owns strict research authority and evidence-state contracts", () => {
    const authority = ResearchAuthoritySchema.parse({
      kind: "local_checkout",
      repository: "acme/octocode-research-agent",
      checkoutPath: "/workspace/octocode-research-agent",
      headCommit: "0123456789abcdef",
      dirty: true,
    });
    const claim = ResearchClaimSchema.parse({
      id: "claim-1",
      text: "The active checkout owns the state key.",
      kind: "code_behavior",
      status: "supported",
      evidenceRefs: ["src/state/protocol-constants.ts:10"],
      authorityRequired: "active checkout",
    });
    expect(ResearchContinuationSchema.parse({
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "research-9",
      savedAt: "2026-08-12T10:00:00.000Z",
      authority,
      claims: [claim],
      branches: [],
      durableAnchors: claim.evidenceRefs,
      contradictions: [],
    }).claims).toEqual([claim]);
    expect(ResearchClaimSchema.safeParse({ ...claim, evidenceRefs: [] }).success).toBe(false);
    expect(claim.verificationLevel).toBeUndefined();
    // critical_path alone, with only one evidence grade represented, is not enough —
    // never conclude a high-stakes claim from one grade alone.
    expect(ResearchClaimSchema.safeParse({ ...claim, verificationLevel: "critical_path" }).success).toBe(false);
    expect(ResearchClaimSchema.parse({
      ...claim,
      verificationLevel: "critical_path",
      evidenceGrades: ["semantic", "lexical"],
    }).verificationLevel).toBe("critical_path");
    expect(ResearchClaimSchema.safeParse({ ...claim, verificationLevel: "critical" }).success).toBe(false);
  });

  it("accepts any deployment-defined anchor kind, not just a closed vendor-specific enum", () => {
    expect(AgentResultAnchorSchema.safeParse({
      kind: "kafka_topic",
      value: "domain-events-orders",
      ref: "kafka:domain-events-orders",
    }).success).toBe(true);
    expect(AgentResultAnchorSchema.safeParse({
      kind: "Not An Identifier!",
      value: "x",
      ref: "x",
    }).success).toBe(false);
  });

  it("bounds arbitrary projection text to one of the protocol's own limits, with a fallback", () => {
    expect(boundProtocolText("  hello  ", "fallback", 100)).toBe("hello");
    expect(boundProtocolText(undefined, "fallback", 100)).toBe("fallback");
    expect(boundProtocolText("   ", "fallback", 100)).toBe("fallback");
    expect(boundProtocolText("x".repeat(10), "fallback", 5)).toBe("xxxxx");
  });

  it("carries an optional cross-agent confidence signal without requiring it", () => {
    const answered = {
      question: "Who owns retries?",
      summary: "retry.ts owns retries.",
      status: "answered" as const,
      terminal: true as const,
      evidence: [{ ref: "src/retry.ts:10", claim: "scheduleRetry owns retries." }],
    };
    expect(AgentResultSchema.safeParse({ finding: answered }).success).toBe(true);
    expect(AgentResultSchema.safeParse({
      finding: { ...answered, confidence: "high" },
    }).success).toBe(true);
    expect(AgentResultSchema.safeParse({
      finding: { ...answered, confidence: "certain" },
    }).success).toBe(false);
  });

  it("distinguishes partial (evidenced) from unresolved (no evidence found)", () => {
    // Hardcoded literal, not just toContain("partial") — toContain would stay green
    // even if a status were silently renamed, dropped, or added elsewhere in the set.
    expect([...AGENT_RESULT_STATUSES].sort()).toEqual(
      ["answered", "error", "needs_lane", "partial", "unresolved"].sort(),
    );
    expect(findingRequiresEvidence("partial")).toBe(true);
    expect(findingRequiresEvidence("unresolved")).toBe(false);
    expect(findingRequiresNextLanes("needs_lane")).toBe(true);
    const base = {
      question: "Who owns retries?",
      summary: "retry.ts owns scheduling; the failure-recovery path is still unconfirmed.",
      terminal: true,
    } as const;
    expect(AgentResultSchema.safeParse({
      finding: { ...base, status: "partial", evidence: [{ ref: "src/retry.ts:10", claim: "scheduleRetry owns retries." }] },
    }).success).toBe(true);
    expect(AgentResultSchema.safeParse({
      finding: { ...base, status: "partial", evidence: [] },
    }).success).toBe(false);
    expect(AgentResultSchema.safeParse({
      finding: { ...base, status: "unresolved", evidence: [] },
    }).success).toBe(true);
  });

  it("rejects a research continuation past the aggregate serialized-size ceiling", () => {
    const authority = ResearchAuthoritySchema.parse({ kind: "unspecified", reason: "size probe" });
    const oversizedClaim = ResearchClaimSchema.parse({
      id: "claim-oversized",
      text: "x".repeat(1_000),
      kind: "code_behavior",
      status: "open",
      evidenceRefs: [],
      authorityRequired: "y".repeat(500),
      decidingCheck: "z".repeat(1_000),
    });
    const claims = Array.from({ length: 48 }, (_, index) => ({
      ...oversizedClaim,
      id: `claim-${index}`,
    }));
    const oversized = {
      protocolVersion: RESEARCH_PROTOCOL_VERSION,
      contextId: "research-9",
      savedAt: "2026-08-12T10:00:00.000Z",
      authority,
      claims,
      branches: [],
      durableAnchors: [],
      contradictions: [],
    };
    expect(JSON.stringify(oversized).length).toBeGreaterThan(RESEARCH_CONTINUATION_MAX_SERIALIZED_CHARS);
    expect(ResearchContinuationSchema.safeParse(oversized).success).toBe(false);
    expect(ResearchContinuationSchema.safeParse({ ...oversized, claims: [oversizedClaim] }).success).toBe(true);
  });

});
