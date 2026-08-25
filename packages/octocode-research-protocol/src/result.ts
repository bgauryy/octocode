import { z } from "zod/v3";

import {
  AgentAckAuthorSchema,
  AgentLaneIdentifierSchema,
  AgentProtocolIdentifierSchema,
  AgentProtocolTimestampSchema,
  RESEARCH_PROTOCOL_VERSION,
} from "./identity.js";
import { AgentExtensionsSchema } from "./extensions.js";
import { AgentTaskObjectiveSchema } from "./task-fields.js";

export const AGENT_RESULT_LIMITS = Object.freeze({
  summaryChars: 8_000,
  evidenceEntries: 48,
  evidenceRefChars: 2_048,
  evidenceClaimChars: 4_000,
  anchors: 48,
  nextLanes: 16,
});

/**
 * partial = evidence-backed but incomplete; keep it out of "unresolved" so a
 * synthesizer never averages "found half" with "found nothing" into one bucket.
 */
export const AGENT_RESULT_STATUSES = ["answered", "partial", "unresolved", "needs_lane", "error"] as const;
export const AGENT_CONFIDENCE_LEVELS = ["low", "medium", "high"] as const;
export const AgentConfidenceSchema = z.enum(AGENT_CONFIDENCE_LEVELS);
export type AgentConfidence = z.infer<typeof AgentConfidenceSchema>;
/**
 * Anchor kind is an open, deployment-defined vocabulary, not a protocol enum:
 * available tools/surfaces are runtime configuration (see README), so the
 * shared protocol must not hardcode any one deployment's tool names here.
 * No consumer branches on a specific kind value other than the "other"
 * fallback, so a closed list bought nothing but had to be edited every time
 * a new surface (e.g. a Kafka topic anchor) was added.
 */
export const AGENT_RESULT_ANCHOR_KIND_MAX_CHARS = 64;
export const AgentResultAnchorKindSchema = z.string().trim().min(1).max(
  AGENT_RESULT_ANCHOR_KIND_MAX_CHARS,
).regex(/^[a-z][a-z0-9_]*$/, "anchor kind must be a lowercase snake_case identifier");

/**
 * Open, deployment-defined vocabulary — same rationale as AgentResultAnchorKindSchema
 * above. A closed enum here previously hardcoded one deployment's own tool names
 * ("octocode_local", "native_checkout") into a package the README requires stay
 * capability-registry-neutral; only the ABSTRACT invariants below (evidence_probe
 * cannot silently fall back; a mode switch needs a reason; the family actually used
 * must be declared) are this protocol's business, never the mode names themselves.
 */
export const REMOTE_SEARCH_MODE_MAX_CHARS = 32;
export const RemoteSearchModeSchema = z.string().trim().min(1).max(
  REMOTE_SEARCH_MODE_MAX_CHARS,
).regex(/^[a-z][a-z0-9_]*$/, "search mode must be a lowercase snake_case identifier").describe(
  "Deployment-defined search mode (e.g. \"native\", \"indexed_local\", \"sqlite_read\") — not fixed by this package.",
);
export type RemoteSearchMode = z.infer<typeof RemoteSearchModeSchema>;
export const REMOTE_RESEARCH_EXECUTION_MODES = ["research", "evidence_probe"] as const;
export const RemoteFactoryIdSchema = z.string().trim().regex(
  /^[a-z][a-z0-9_-]{0,31}$/,
  "factory must be a lowercase stable identifier",
);
export type RemoteFactoryId = z.infer<typeof RemoteFactoryIdSchema>;
export const RemoteResearchExecutionModeSchema = z.enum(REMOTE_RESEARCH_EXECUTION_MODES).describe(
  "research = may fall back between search modes with a reason; evidence_probe = must use exactly the requested mode.",
);
export type RemoteResearchExecutionMode = z.infer<typeof RemoteResearchExecutionModeSchema>;

/**
 * Scoped to the terminal result boundary only — evidence.ref/anchor.ref are
 * the load-bearing citation a downstream agent acts on without re-checking.
 * Do not reuse this on lower-stakes channels (e.g. AgentMessage evidenceRefs,
 * which pass model input straight through with no such requirement).
 */
export function isAgentResultEvidenceLocator(ref: string): boolean {
  const trimmed = ref.trim();
  const normalized = trimmed.toLowerCase().replace(/[._-]+/g, " ").replace(/\s+/g, " ");
  if (new Set([
    "unknown", "none", "n/a", "na", "unavailable", "unverified", "unresolved", "missing", "tbd",
  ]).has(normalized)) return false;
  if (/\b(see above|as mentioned|as above|the file above)\b/i.test(trimmed)) return false;
  if (/\s{2,}|\.\s+[A-Z]/.test(trimmed)) return false;
  // A real locator is a compact pointer, not a sentence — a run of 4+ space-separated
  // words is prose even when one of those words happens to look like a path or file
  // extension (e.g. "the parse call in result.ts" would otherwise slip through).
  if (trimmed.split(/\s+/).length >= 4) return false;
  return (
    /https?:\/\//.test(trimmed)
    || /[/:\\@#]/.test(trimmed)
    || /\.[A-Za-z0-9]{1,16}\b/.test(trimmed)
    || /^[A-Z][A-Z0-9]+-\d+$/.test(trimmed)
    || /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(trimmed)
    || /^[A-Za-z0-9][A-Za-z0-9._-]{2,}$/.test(trimmed)
  );
}

export const AgentResultEvidenceSchema = z.object({
  ref: z.string().trim().min(1).max(AGENT_RESULT_LIMITS.evidenceRefChars)
    .refine(isAgentResultEvidenceLocator, "evidence.ref must be a compact locator, not prose")
    .describe("Exact locator the claim rests on — path:line, request id, URL, table, or similar compact pointer."),
  claim: z.string().trim().min(1).max(AGENT_RESULT_LIMITS.evidenceClaimChars).describe(
    "The specific fact this locator supports — one sentence, not a paraphrase of the summary.",
  ),
}).strict();

export const AgentResultAnchorSchema = z.object({
  kind: AgentResultAnchorKindSchema.describe(
    "Open locator kind (e.g. path, url, jira_key) — deployment-defined, not fixed here.",
  ),
  value: z.string().trim().min(1).max(AGENT_RESULT_LIMITS.evidenceRefChars).describe(
    "The anchor's content (e.g. the symbol name, table name, or jira key itself).",
  ),
  ref: z.string().trim().min(1).max(AGENT_RESULT_LIMITS.evidenceRefChars)
    .refine(isAgentResultEvidenceLocator, "anchor.ref must be a compact locator, not prose")
    .describe("Tool-ready locator for this anchor — where a downstream agent should look to confirm it."),
}).strict();

export const AgentResultEvidenceListSchema = z.array(AgentResultEvidenceSchema)
  .max(AGENT_RESULT_LIMITS.evidenceEntries);
export const AgentResultAnchorListSchema = z.array(AgentResultAnchorSchema)
  .max(AGENT_RESULT_LIMITS.anchors);
export const AgentResultNextLanesSchema = z.array(AgentLaneIdentifierSchema)
  .max(AGENT_RESULT_LIMITS.nextLanes);

/** Actual resource spend for a task, if the runtime tracks it — informational only, never enforced here. */
export const AgentTaskConsumptionSchema = z.object({
  steps: z.number().int().nonnegative().optional().describe("Agent turns/tool calls actually spent."),
  wallClockMs: z.number().int().nonnegative().optional().describe("Actual elapsed wall-clock time in milliseconds."),
  costUsd: z.number().nonnegative().optional().describe("Actual metered cost, if the runtime meters cost."),
}).strict();
export type AgentTaskConsumption = z.infer<typeof AgentTaskConsumptionSchema>;

/**
 * Reusable finding invariants — exported so a consumer extending this shape
 * (e.g. a helper-specific finding schema) enforces the same rules instead of
 * hand-copying them. A status claiming evidence was found (answered/partial)
 * must cite it; a status claiming nothing was found (unresolved/error) must
 * NOT be forced to — that would train fabricated locators, not rigor.
 */
export function findingRequiresEvidence(status: string | undefined): boolean {
  return status === "answered" || status === "partial";
}
export function findingRequiresNextLanes(status: string | undefined): boolean {
  return status === "needs_lane";
}

export const AgentResultFindingSchema = z.object({
  question: AgentTaskObjectiveSchema.optional().describe(
    "Echoes the originating objective, when there's a plaintext one — omitted for a recovered task.",
  ),
  summary: z.string().trim().min(1).max(AGENT_RESULT_LIMITS.summaryChars).describe(
    "Decision-grade answer for the recipient; mirror tool-ready locators in evidence/anchors, not prose.",
  ),
  status: z.enum(AGENT_RESULT_STATUSES).describe(
    "answered = fully evidenced; partial = evidenced but incomplete; unresolved = no evidence found; "
    + "needs_lane = a different capability must continue; error = tool/auth/capability failure.",
  ),
  terminal: z.boolean().refine((value) => value, "terminal result must set terminal=true"),
  evidence: AgentResultEvidenceListSchema.describe(
    "Inline citations backing the summary; required whenever status claims something was found.",
  ),
  anchors: AgentResultAnchorListSchema.optional().describe(
    "One tool-ready locator per entry for a downstream agent to act on directly.",
  ),
  nextSuggestedLanes: AgentResultNextLanesSchema.optional(),
  confidence: AgentConfidenceSchema.optional().describe(
    "Sender's own trust in this finding, if it has one to report — never inferred by a receiver.",
  ),
}).strict()
  .refine((finding) => !findingRequiresEvidence(finding.status) || finding.evidence.length > 0, {
    message: "status=answered/partial requires at least one evidence entry",
    path: ["evidence"],
  })
  .refine(
    (finding) => !findingRequiresNextLanes(finding.status)
      || (finding.nextSuggestedLanes != null && finding.nextSuggestedLanes.length > 0),
    { message: "status=needs_lane requires nextSuggestedLanes", path: ["nextSuggestedLanes"] },
  );

export const RemoteResearchTraceSchema = z.object({
  executionMode: RemoteResearchExecutionModeSchema,
  immutablePin: z.string().trim().min(1).max(200).optional().describe(
    "Opaque immutable pin the trace was gathered against (a git commit, a file hash, a snapshot id, ...) — "
    + "no fixed format, since this trace can result from any authority kind, not just git.",
  ),
  checksPerformed: z.array(z.string().trim().min(1).max(500)).min(1).max(16).describe(
    "What was actually checked (e.g. \"semantic reference lookup\") — the verification steps, not the finding.",
  ),
  inspectedSurface: z.array(z.string().trim().min(1).max(500)).max(16).default([]).describe(
    "Files/symbols/surfaces actually looked at.",
  ),
  exclusions: z.array(z.string().trim().min(1).max(500)).max(16).default([]).describe(
    "What was deliberately NOT checked (e.g. \"live production state\") — scopes the finding's confidence.",
  ),
  searchModeRequested: RemoteSearchModeSchema.describe("Search mode the caller asked for."),
  searchModeUsed: RemoteSearchModeSchema.describe("Search mode actually used — may differ from requested with a fallbackReason."),
  fallbackReason: z.string().trim().min(1).max(500).optional().describe(
    "Required when searchModeUsed differs from searchModeRequested.",
  ),
  toolFamiliesUsed: z.array(RemoteSearchModeSchema).min(1).max(8)
    .refine((values) => new Set(values).size === values.length, "toolFamiliesUsed must be unique")
    .describe("Every distinct tool family actually invoked; must include searchModeUsed's mode."),
}).strict().superRefine((trace, ctx) => {
  if (trace.executionMode === "evidence_probe" && trace.searchModeRequested !== trace.searchModeUsed) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["searchModeUsed"],
      message: "evidence_probe is a bounded check and cannot silently fall back to a different search mode",
    });
  }
  if (trace.searchModeRequested !== trace.searchModeUsed && trace.fallbackReason == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["fallbackReason"],
      message: "fallbackReason is required when searchModeUsed differs from searchModeRequested",
    });
  }
  if (!trace.toolFamiliesUsed.includes(trace.searchModeUsed)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["toolFamiliesUsed"],
      message: `toolFamiliesUsed must include the search mode actually used ("${trace.searchModeUsed}")`,
    });
  }
});

/** Transport-neutral terminal result submitted by a remote research participant. */
export const AgentResultSchema = z.object({
  finding: AgentResultFindingSchema,
  researchTrace: RemoteResearchTraceSchema.optional().describe(
    "Present only for capability-specific remote research; a generic finding stays trace-free.",
  ),
  consumption: AgentTaskConsumptionSchema.optional(),
  supersedesResultId: AgentProtocolIdentifierSchema.optional().describe(
    "The prior resultId this one revises, after a needs_revision ack reopened the task.",
  ),
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentResult = z.infer<typeof AgentResultSchema>;

/** Remote research terminal result: the execution trace is mandatory at this boundary. */
const RemoteResearchResultObjectSchema = AgentResultSchema.extend({
  researchTrace: RemoteResearchTraceSchema,
}).strict();
export const RemoteResearchResultShape = RemoteResearchResultObjectSchema.shape;
export const RemoteResearchResultSchema = RemoteResearchResultObjectSchema.superRefine((result, ctx) => {
  if (result.finding.status === "answered" && result.researchTrace.immutablePin == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["researchTrace", "immutablePin"],
      message: "answered remote research results require an immutable pin",
    });
  }
});
export type RemoteResearchResult = z.infer<typeof RemoteResearchResultSchema>;

export const AgentSyncResultFrameSchema = z.object({
  schemaVersion: z.literal(1),
  sessionId: AgentProtocolIdentifierSchema,
  resultId: AgentProtocolIdentifierSchema,
  sender: z.literal("REMOTE_RESEARCH"),
  result: RemoteResearchResultSchema,
  submittedAt: AgentProtocolTimestampSchema,
}).strict();
export type AgentSyncResultFrame = z.infer<typeof AgentSyncResultFrameSchema>;

/**
 * Closes a real gap: nothing previously let the assigner respond to a terminal
 * AgentResult. Without this, a "challenge" AgentMessage had nowhere to attach —
 * the assigner could only silently re-task. resultId is optional because not
 * every transport hands the result an id of its own to reference.
 */
export const AGENT_RESULT_ACK_VERDICTS = ["accepted", "needs_revision", "rejected"] as const;
export const AgentResultAckVerdictSchema = z.enum(AGENT_RESULT_ACK_VERDICTS);
export type AgentResultAckVerdict = z.infer<typeof AgentResultAckVerdictSchema>;

export const AgentResultAckSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  resultId: AgentProtocolIdentifierSchema.optional().describe(
    "The transport's own id for the result being acknowledged, if it has one.",
  ),
  acknowledgedBy: AgentAckAuthorSchema,
  verdict: AgentResultAckVerdictSchema.describe(
    "accepted = final; needs_revision = keep working per reason; rejected = won't be acted on.",
  ),
  reason: z.string().trim().min(1).max(1_000).optional(),
  acknowledgedAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict().superRefine((ack, ctx) => {
  if (ack.verdict !== "accepted" && ack.reason == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["reason"],
      message: "needs_revision/rejected verdicts require a reason",
    });
  }
});
export type AgentResultAck = z.infer<typeof AgentResultAckSchema>;
