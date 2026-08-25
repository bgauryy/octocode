import { z } from "zod/v3";
import {
  AgentProtocolIdentifierSchema,
  AgentProtocolTimestampSchema,
  RESEARCH_PROTOCOL_VERSION,
} from "./identity.js";
import { AgentExtensionsSchema } from "./extensions.js";
import { RESEARCH_CONTINUATION_MAX_SERIALIZED_CHARS } from "./limits.js";

const IdentifierSchema = z.string().trim().min(1).max(256);
/** Not locator-shaped on purpose — see result.ts's isAgentResultEvidenceLocator for why that check stays scoped to terminal results. */
const EvidenceRefSchema = z.string().trim().min(1).max(2_048);

/**
 * What a claim's evidence is actually pinned to — a discriminated union, not
 * one bag of optional fields, so a "local_checkout" claim cannot silently
 * omit the commit it was checked against, and a "live_system" claim cannot
 * pretend to have one.
 */
export const ResearchAuthoritySchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("unspecified"),
    reason: z.string().trim().min(1).max(500).describe("Why no authority could be established."),
  }).strict(),
  z.object({
    kind: z.literal("local_checkout"),
    repository: IdentifierSchema,
    checkoutPath: z.string().trim().min(1).max(1_024).describe("Absolute path to the inspected working tree."),
    headCommit: z.string().trim().regex(/^[0-9a-f]{7,64}$/i).describe("The commit HEAD pointed at when inspected."),
    dirty: z.boolean().describe("True if the working tree had uncommitted changes when inspected."),
    workingTreeFingerprint: IdentifierSchema.optional().describe(
      "Opaque hash of uncommitted state, if the runtime tracks one — lets a later session detect drift.",
    ),
  }).strict(),
  z.object({
    kind: z.literal("remote_ref"),
    repository: IdentifierSchema,
    requestedRef: IdentifierSchema.describe("Branch, tag, or ref the caller asked to inspect."),
    resolvedCommit: z.string().trim().regex(/^[0-9a-f]{7,64}$/i).optional().describe(
      "The immutable commit requestedRef resolved to, once known.",
    ),
  }).strict(),
  z.object({
    kind: z.literal("live_system"),
    system: IdentifierSchema.describe("The running system inspected (e.g. a service or database name)."),
    environment: IdentifierSchema.describe("Which deployment environment (e.g. staging, prod) was inspected."),
    timeWindow: z.object({
      from: AgentProtocolTimestampSchema,
      to: AgentProtocolTimestampSchema,
    }).strict().optional().describe("Bounds the observation window for a claim that can change moment to moment."),
  }).strict(),
  z.object({
    kind: z.literal("local_artifact"),
    path: z.string().trim().min(1).max(1_024).describe("Absolute path to the inspected file/database/dataset."),
    fingerprint: IdentifierSchema.optional().describe(
      "Opaque immutable pin (a content hash, mtime, version id, ...), if one is available — no fixed format, "
      + "unlike local_checkout/remote_ref which are specifically git-shaped.",
    ),
  }).strict(),
]);
export type ResearchAuthority = z.infer<typeof ResearchAuthoritySchema>;

/** Network surfaces cannot inspect a caller's local filesystem. */
export const TransportResearchAuthoritySchema = ResearchAuthoritySchema.refine(
  (authority) => authority.kind !== "local_checkout" && authority.kind !== "local_artifact",
  "local_checkout/local_artifact authority is only valid for a trusted in-process runtime with local-file evidence",
);

/**
 * Open, deployment-defined vocabulary — same rationale as AgentResultAnchorKindSchema:
 * a closed enum here would privilege code research ("code_behavior", "code_absence")
 * over any other domain a claim might be about (ops, legal, scientific, ...), and
 * nothing in ResearchClaimSchema branches on the specific kind value.
 */
export const RESEARCH_CLAIM_KIND_MAX_CHARS = 32;
export const ResearchClaimKindSchema = z.string().trim().min(1).max(RESEARCH_CLAIM_KIND_MAX_CHARS)
  .regex(/^[a-z][a-z0-9_]*$/, "claim kind must be a lowercase snake_case identifier")
  .describe(
    "Open claim category (e.g. \"code_behavior\", \"operational\", \"historical\") — deployment-defined, not fixed here.",
  );
export type ResearchClaimKind = z.infer<typeof ResearchClaimKindSchema>;
export const RESEARCH_CLAIM_STATUSES = ["open", "supported", "contradicted", "blocked"] as const;
export const RESEARCH_VERIFICATION_LEVELS = ["standard", "critical_path"] as const;

/**
 * Open, deployment-defined vocabulary — same rationale as ResearchClaimKindSchema.
 * Deliberately NOT the closed four-value taxonomy (semantic/structural/lexical/
 * provider) from the evidence-grading literature this operationalizes: a main
 * researcher orchestrating subagents with heterogeneous tools (code search, a
 * language server, a metrics dashboard, a monitoring system, the open web) needs
 * grades for sources that taxonomy never anticipated (e.g. "grafana_metric",
 * "web_search") — hardcoding the four would repeat the exact code-research bias
 * ResearchClaimKindSchema was opened up to avoid.
 */
export const EVIDENCE_SOURCE_GRADE_MAX_CHARS = 32;
export const EvidenceSourceGradeSchema = z.string().trim().min(1).max(EVIDENCE_SOURCE_GRADE_MAX_CHARS)
  .regex(/^[a-z][a-z0-9_]*$/, "evidence source grade must be a lowercase snake_case identifier")
  .describe(
    "Open evidence source/grade (e.g. \"semantic\", \"lexical\", \"grafana_metric\", \"web_search\") this "
    + "corroborating evidence came from — deployment-defined, not fixed here.",
  );
export type EvidenceSourceGrade = z.infer<typeof EvidenceSourceGradeSchema>;

export const ResearchClaimSchema = z.object({
  id: IdentifierSchema,
  text: z.string().trim().min(1).max(1_000).describe("The claim itself, stated as a single checkable assertion."),
  kind: ResearchClaimKindSchema,
  status: z.enum(RESEARCH_CLAIM_STATUSES).describe(
    "open = not yet checked; supported = evidenced; contradicted = evidence disagrees; "
    + "blocked = cannot proceed without decidingCheck.",
  ),
  verificationLevel: z.enum(RESEARCH_VERIFICATION_LEVELS).optional().describe(
    "critical_path only when a wrong conclusion would materially affect remediation, incident response, or a major decision.",
  ),
  evidenceRefs: z.array(EvidenceRefSchema).max(16),
  evidenceGrades: z.array(EvidenceSourceGradeSchema).max(16).optional().describe(
    "The distinct source grades represented across evidenceRefs (e.g. [\"semantic\", \"grafana_metric\"]) — "
    + "lets a claim's corroboration be checked across sources, not just counted.",
  ),
  authorityRequired: z.string().trim().min(1).max(500).describe(
    "What kind of authority (see ResearchAuthoritySchema) would be sufficient to check this claim.",
  ),
  decidingCheck: z.string().trim().min(1).max(1_000).optional().describe(
    "The next concrete check that would move a blocked claim forward.",
  ),
}).strict().superRefine((claim, ctx) => {
  if (claim.status === "supported" && claim.evidenceRefs.length === 0) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["evidenceRefs"],
      message: "Supported claims require at least one evidence reference",
    });
  }
  if (claim.verificationLevel === "critical_path") {
    const distinctGrades = new Set(claim.evidenceGrades ?? []);
    if (distinctGrades.size < 2) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["evidenceGrades"],
        message: "critical_path claims require evidence from at least two distinct source grades — "
          + "never conclude a high-stakes claim from one grade alone",
      });
    }
  }
  if (claim.status === "blocked" && claim.decidingCheck == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["decidingCheck"],
      message: "Blocked claims require the next deciding check",
    });
  }
});
export type ResearchClaim = z.infer<typeof ResearchClaimSchema>;

export const RESEARCH_BRANCH_STATUSES = ["pending", "active", "resolved", "blocked", "abandoned"] as const;

export const ResearchBranchSchema = z.object({
  id: IdentifierSchema,
  claimIds: z.array(IdentifierSchema).min(1).max(12).describe("Claims this branch of investigation exists to resolve."),
  objective: z.string().trim().min(1).max(2_000),
  reason: z.string().trim().min(1).max(1_000).optional().describe(
    "Why this branch exists or was abandoned — e.g. a claim it follows from, or what made it a dead end. "
    + "Required when status is abandoned; a pivot with no stated reason can't be told apart from a mistake.",
  ),
  dependencyIds: z.array(IdentifierSchema).max(12).describe("Other branches that must resolve before this one can start."),
  lane: z.string().trim().min(1).max(128).describe("Capability/queue this branch is routed to."),
  expectedEvidence: z.string().trim().min(1).max(1_000).describe("What evidence, if found, would resolve this branch."),
  status: z.enum(RESEARCH_BRANCH_STATUSES).describe(
    "pending = not started; active = in progress; resolved = answered; blocked = stuck, may resume; "
    + "abandoned = deliberately dropped in favor of a better lead — distinct from blocked, which implies still worth resuming.",
  ),
}).strict().superRefine((branch, ctx) => {
  if (branch.status === "abandoned" && branch.reason == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["reason"],
      message: "Abandoned branches require a reason",
    });
  }
});
export type ResearchBranch = z.infer<typeof ResearchBranchSchema>;

export const ResearchContinuationSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  contextId: AgentProtocolIdentifierSchema.describe("The research session this continuation resumes."),
  savedAt: AgentProtocolTimestampSchema.describe("When this continuation was persisted, for staleness checks."),
  authority: ResearchAuthoritySchema,
  claims: z.array(ResearchClaimSchema).max(48),
  branches: z.array(ResearchBranchSchema).max(32),
  durableAnchors: z.array(EvidenceRefSchema).max(64).describe(
    "Evidence refs worth carrying forward regardless of which claim cited them.",
  ),
  contradictions: z.array(z.string().trim().min(1).max(1_000)).max(24).describe(
    "Findings that conflict with an existing claim — kept distinct so merging can't silently drop them.",
  ),
  priorAnswerDigest: z.string().trim().min(1).max(12_000).optional().describe(
    "Compact summary of a prior answer this continuation builds on, if any.",
  ),
  extensions: AgentExtensionsSchema.optional(),
}).strict().superRefine((continuation, ctx) => {
  const size = JSON.stringify(continuation).length;
  if (size > RESEARCH_CONTINUATION_MAX_SERIALIZED_CHARS) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["claims"],
      message: `Continuation is ${size} serialized chars, exceeding the `
        + `${RESEARCH_CONTINUATION_MAX_SERIALIZED_CHARS}-char aggregate ceiling — `
        + "drop stale/resolved claims or branches before persisting",
    });
  }
});
export type ResearchContinuation = z.infer<typeof ResearchContinuationSchema>;
