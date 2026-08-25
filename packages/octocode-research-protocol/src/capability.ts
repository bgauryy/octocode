import { z } from "zod/v3";

import {
  AgentAckAuthorSchema,
  AgentIdentitySchema,
  AgentProtocolIdentifierSchema,
  AgentProtocolTimestampSchema,
  RESEARCH_PROTOCOL_VERSION,
} from "./identity.js";
import { AgentExtensionsSchema } from "./extensions.js";

export const AGENT_CAPABILITY_LIMITS = Object.freeze({
  kindChars: 64,
  descriptionChars: 500,
  capabilities: 32,
  reasonChars: 1_000,
});

/**
 * Open, deployment-defined vocabulary — same rationale as AgentResultAnchorKindSchema:
 * a fixed enum here would hardcode one deployment's tool names into a package
 * that must stay capability-registry-neutral (see README).
 */
export const AgentCapabilityKindSchema = z.string().trim().min(1).max(
  AGENT_CAPABILITY_LIMITS.kindChars,
).regex(/^[a-z][a-z0-9_]*$/, "capability kind must be a lowercase snake_case identifier");
export type AgentCapabilityKind = z.infer<typeof AgentCapabilityKindSchema>;

export const AgentCapabilitySchema = z.object({
  kind: AgentCapabilityKindSchema.describe(
    "Open capability/tool-family name (e.g. \"github_search\") — deployment-defined, not fixed here.",
  ),
  description: z.string().trim().min(1).max(AGENT_CAPABILITY_LIMITS.descriptionChars).optional().describe(
    "Detail a peer can't infer from kind alone.",
  ),
}).strict();
export type AgentCapability = z.infer<typeof AgentCapabilitySchema>;

/**
 * A bilateral offer from one peer to another, scoped to one contextId — NOT a
 * discovery registry. README keeps capability REGISTRIES (which agents
 * exist, what they're allowed to do) runtime-owned deployment configuration;
 * this only standardizes the ENVELOPE one already-connected peer uses to tell
 * another what it personally brings to this session, so the receiver can
 * reason about what to delegate or ask for — a real gap confirmed against
 * A2A (tools kept opaque behind an Agent Card), LangGraph swarm (peers know
 * only a name string), and every other surveyed framework.
 */
export const AgentCapabilityDeclarationSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  declarationId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  declaredBy: AgentIdentitySchema.describe("Who is declaring these capabilities."),
  declaredTo: AgentIdentitySchema.optional().describe(
    "Who this is specifically for; omit to broadcast to every other participant in contextId.",
  ),
  capabilities: z.array(AgentCapabilitySchema).min(1).max(AGENT_CAPABILITY_LIMITS.capabilities),
  declaredAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentCapabilityDeclaration = z.infer<typeof AgentCapabilityDeclarationSchema>;

export const AGENT_CAPABILITY_ACK_VERDICTS = ["accepted", "partially_accepted", "declined"] as const;
export const AgentCapabilityAckVerdictSchema = z.enum(AGENT_CAPABILITY_ACK_VERDICTS);
export type AgentCapabilityAckVerdict = z.infer<typeof AgentCapabilityAckVerdictSchema>;

/**
 * `relyingOn` is a RELIANCE statement by the receiver ("these are the ones I'll
 * actually ask you for"), never an AUTHORIZATION grant ("you're permitted
 * these") — that distinction is what keeps this on the declaration side of
 * README's "capability registries remain runtime-owned" line. Renaming it to
 * something like "approved"/"granted" would cross that line; don't.
 *
 * Zod cannot verify relyingOn is actually a subset of what was declared (the
 * original declaration isn't in scope of this schema) — this produces a
 * PROPOSED negotiated set, not a guaranteed one. A runtime that wants the
 * guarantee must cross-check it against the referenced declarationId itself.
 */
export const AgentCapabilityAckSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  declarationId: AgentProtocolIdentifierSchema.describe(
    "The AgentCapabilityDeclaration this verdict responds to.",
  ),
  contextId: AgentProtocolIdentifierSchema,
  acknowledgedBy: AgentAckAuthorSchema,
  verdict: AgentCapabilityAckVerdictSchema.describe(
    "accepted = will rely on all declared kinds; partially_accepted = only the relyingOn subset; "
    + "declined = will rely on none of them.",
  ),
  relyingOn: z.array(AgentCapabilityKindSchema).max(AGENT_CAPABILITY_LIMITS.capabilities).optional().describe(
    "The subset of declared kinds actually relied on. Required (non-empty) when partially_accepted; "
    + "omit for accepted (means all) and declined (means none).",
  ),
  reason: z.string().trim().min(1).max(AGENT_CAPABILITY_LIMITS.reasonChars).optional().describe(
    "Required detail when verdict is not a plain accepted.",
  ),
  acknowledgedAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict().superRefine((ack, ctx) => {
  if (ack.verdict !== "accepted" && ack.reason == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["reason"],
      message: "partially_accepted/declined verdicts require a reason",
    });
  }
  if (ack.verdict === "partially_accepted" && (ack.relyingOn == null || ack.relyingOn.length === 0)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["relyingOn"],
      message: "partially_accepted requires a non-empty relyingOn subset",
    });
  }
  if (ack.verdict !== "partially_accepted" && ack.relyingOn != null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["relyingOn"],
      message: "relyingOn is only meaningful for partially_accepted",
    });
  }
});
export type AgentCapabilityAck = z.infer<typeof AgentCapabilityAckSchema>;
