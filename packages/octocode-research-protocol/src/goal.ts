import { z } from "zod/v3";

import {
  AgentAckAuthorSchema,
  AgentIdentitySchema,
  AgentProtocolIdentifierSchema,
  AgentProtocolTimestampSchema,
  RESEARCH_PROTOCOL_VERSION,
} from "./identity.js";
import { AgentExtensionsSchema } from "./extensions.js";
import { AgentTaskObjectiveSchema } from "./task-fields.js";

export const AGENT_GOAL_LIMITS = Object.freeze({
  decompositionSteps: 16,
  stepChars: 500,
  assumptions: 16,
  assumptionChars: 500,
  openQuestions: 16,
  openQuestionChars: 500,
  reasonChars: 1_000,
});

/**
 * Operationalizes Clark & Brennan's grounding criterion: a receiver restates
 * the goal in its own words, with its own decomposition/assumptions/gaps,
 * before work starts — checkable acceptance, not an assumed one. taskId is
 * optional because this can happen during handoff negotiation, before either
 * side has committed to a formal AgentTask — exactly the "two peers still
 * introducing themselves" case a symmetric relationship needs and an
 * assigner-driven AgentHandoff does not.
 */
export const AgentGoalRestatementSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  restatementId: AgentProtocolIdentifierSchema,
  taskId: AgentProtocolIdentifierSchema.optional(),
  contextId: AgentProtocolIdentifierSchema,
  restatedBy: AgentIdentitySchema,
  restatedTo: AgentIdentitySchema.optional().describe(
    "Who this responds to; omit to broadcast to every other participant in contextId.",
  ),
  restatedObjective: AgentTaskObjectiveSchema.describe(
    "The restater's own wording of the goal — not a copy of the original.",
  ),
  plannedDecomposition: z.array(z.string().trim().min(1).max(AGENT_GOAL_LIMITS.stepChars))
    .max(AGENT_GOAL_LIMITS.decompositionSteps).optional().describe(
      "How the restater intends to break the goal into sub-steps.",
    ),
  assumptions: z.array(z.string().trim().min(1).max(AGENT_GOAL_LIMITS.assumptionChars))
    .max(AGENT_GOAL_LIMITS.assumptions).optional().describe(
      "What the restater is assuming that wasn't stated explicitly.",
    ),
  openQuestions: z.array(z.string().trim().min(1).max(AGENT_GOAL_LIMITS.openQuestionChars))
    .max(AGENT_GOAL_LIMITS.openQuestions).optional().describe(
      "What the restater still doesn't understand about the goal.",
    ),
  restatedAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentGoalRestatement = z.infer<typeof AgentGoalRestatementSchema>;

export const AGENT_GOAL_ACK_VERDICTS = ["confirmed", "confirmed_with_amendment", "misaligned"] as const;
export const AgentGoalAckVerdictSchema = z.enum(AGENT_GOAL_ACK_VERDICTS);
export type AgentGoalAckVerdict = z.infer<typeof AgentGoalAckVerdictSchema>;

export const AgentGoalAckSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  taskId: AgentProtocolIdentifierSchema.optional(),
  contextId: AgentProtocolIdentifierSchema,
  restatementId: AgentProtocolIdentifierSchema.optional().describe(
    "The AgentGoalRestatement this verdict responds to, if the transport assigns one.",
  ),
  acknowledgedBy: AgentAckAuthorSchema,
  verdict: AgentGoalAckVerdictSchema.describe(
    "confirmed = matches intent; confirmed_with_amendment = close, correction in reason; "
    + "misaligned = real mismatch, needs another restatement.",
  ),
  reason: z.string().trim().min(1).max(AGENT_GOAL_LIMITS.reasonChars).optional().describe(
    "Required detail when verdict is not a plain confirmed.",
  ),
  acknowledgedAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict().superRefine((ack, ctx) => {
  if (ack.verdict !== "confirmed" && ack.reason == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["reason"],
      message: "confirmed_with_amendment/misaligned verdicts require a reason",
    });
  }
});
export type AgentGoalAck = z.infer<typeof AgentGoalAckSchema>;

export const AGENT_GOAL_REVISION_KINDS = ["achieved", "impossible", "irrelevant"] as const;
export const AgentGoalRevisionKindSchema = z.enum(AGENT_GOAL_REVISION_KINDS);
export type AgentGoalRevisionKind = z.infer<typeof AgentGoalRevisionKindSchema>;

/**
 * Operationalizes Cohen & Levesque's standing obligation to make goal
 * termination mutually known — either peer may raise this, unlike
 * AgentTerminalOutcome which is assigner-oriented (from/to). Advisory only:
 * raising this does NOT itself close a task or supersede AgentResult/
 * AgentTerminalOutcome — a runtime still closes the task through its normal
 * terminal path once it acts on this signal.
 */
export const AgentGoalRevisionSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  taskId: AgentProtocolIdentifierSchema.optional(),
  contextId: AgentProtocolIdentifierSchema,
  raisedBy: AgentIdentitySchema,
  revision: AgentGoalRevisionKindSchema.describe(
    "achieved = satisfied by other means; impossible = cannot be achieved; irrelevant = no longer worth pursuing.",
  ),
  reason: z.string().trim().min(1).max(AGENT_GOAL_LIMITS.reasonChars),
  raisedAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentGoalRevision = z.infer<typeof AgentGoalRevisionSchema>;
