import { z } from "zod/v3";

import {
  AgentAckAuthorSchema,
  AgentIdentitySchema,
  AgentProtocolIdentifierSchema,
  AgentProtocolTimestampSchema,
  RESEARCH_PROTOCOL_VERSION,
} from "./identity.js";
import { AgentExtensionsSchema } from "./extensions.js";
import { ResearchAuthoritySchema } from "./research.js";
import { AgentResultEvidenceListSchema, AgentResultSchema } from "./result.js";
import {
  AGENT_TASK_LIMITS,
  AgentCancellationReasonSchema,
  AgentOutcomeReasonSchema,
  AgentTaskContextSchema,
  AgentTaskObjectiveSchema,
  AgentTaskRequiredSuccessCriteriaSchema,
  AgentTaskSuccessCriteriaSchema,
} from "./task-fields.js";

export const AGENT_TASK_STATUSES = [
  "spawning",
  "running",
  "revising",
  "done",
  "failed",
  "cancelled",
  "expired",
] as const;
export const AgentTaskStatusSchema = z.enum(AGENT_TASK_STATUSES).describe(
  "revising = reopened after a needs_revision ack — distinct from a first-attempt running.",
);
export type AgentTaskStatus = z.infer<typeof AgentTaskStatusSchema>;

/** Soft resource ceiling a runtime enforces; the protocol only communicates intent, never enforcement. */
export const AgentTaskBudgetSchema = z.object({
  maxSteps: z.number().int().positive().max(10_000).optional().describe("Agent turns/tool calls allowed."),
  maxWallClockMs: z.number().int().positive().max(86_400_000).optional().describe("Wall-clock ceiling in milliseconds."),
  maxCostUsd: z.number().positive().max(10_000).optional().describe("Metered-cost ceiling, if the runtime meters cost."),
}).strict();
export type AgentTaskBudget = z.infer<typeof AgentTaskBudgetSchema>;

const TaskSemanticsShape = {
  context: AgentTaskContextSchema.optional(),
  priority: z.enum(["critical", "optional"]).optional().describe(
    "Omit for normal priority; critical/optional are the only two axes a scheduler needs.",
  ),
  authority: ResearchAuthoritySchema.optional(),
  deadlineAt: AgentProtocolTimestampSchema.optional(),
  budget: AgentTaskBudgetSchema.optional(),
  extensions: AgentExtensionsSchema.optional(),
} as const;

export const AgentAvailableTaskSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  agent: AgentIdentitySchema,
  assignedBy: AgentIdentitySchema.optional().describe(
    "Who handed off this task, if the runtime tracks it — mirrors AgentHandoff.from once a task is accepted.",
  ),
  name: z.string().trim().min(1).max(60).optional(),
  objective: AgentTaskObjectiveSchema,
  semanticAvailability: z.literal("available"),
  ...TaskSemanticsShape,
  successCriteria: AgentTaskSuccessCriteriaSchema,
  status: AgentTaskStatusSchema,
  createdAt: AgentProtocolTimestampSchema,
}).strict();
export type AgentAvailableTask = z.infer<typeof AgentAvailableTaskSchema>;

/**
 * Recovered transports that intentionally persist only an objective digest
 * still emit an AgentTask, but must not invent or expose replacement plaintext
 * semantics — so objective stays absent here, not merely optional, and a
 * consumer narrowing on semanticAvailability gets that guarantee in the type,
 * not just at runtime.
 */
export const AgentRecoveredTaskSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  agent: AgentIdentitySchema,
  assignedBy: AgentIdentitySchema.optional(),
  name: z.string().trim().min(1).max(60).optional(),
  objective: z.undefined().optional(),
  semanticAvailability: z.literal("unavailable_after_recovery"),
  ...TaskSemanticsShape,
  successCriteria: AgentTaskSuccessCriteriaSchema,
  status: AgentTaskStatusSchema,
  createdAt: AgentProtocolTimestampSchema,
}).strict();
export type AgentRecoveredTask = z.infer<typeof AgentRecoveredTaskSchema>;

/**
 * Transport-neutral task projection after an execution adapter accepts a
 * handoff. A real discriminatedUnion (not a plain union) so an invalid
 * semanticAvailability value reports "expected 'available' | "
 * "'unavailable_after_recovery'" directly, instead of the far less useful
 * "no union member matched" a plain z.union produces.
 */
export const AgentTaskSchema = z.discriminatedUnion("semanticAvailability", [
  AgentAvailableTaskSchema,
  AgentRecoveredTaskSchema,
]);
export type AgentTask = z.infer<typeof AgentTaskSchema>;

/** Semantic delegation packet; capability-specific launch settings wrap this schema. */
export const AgentHandoffSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  handoffId: AgentProtocolIdentifierSchema,
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  from: AgentIdentitySchema,
  to: AgentIdentitySchema,
  objective: AgentTaskObjectiveSchema,
  ...TaskSemanticsShape,
  successCriteria: AgentTaskRequiredSuccessCriteriaSchema,
  createdAt: AgentProtocolTimestampSchema,
}).strict();
export type AgentHandoff = z.infer<typeof AgentHandoffSchema>;

/** Researcher-visible projection; adapters retain their authoritative runtime states. */
export const AgentLifecycleEventSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  eventId: z.string().trim().min(1).max(256),
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  sequence: z.number().int().positive(),
  agent: AgentIdentitySchema,
  kind: z.literal("lifecycle"),
  status: AgentTaskStatusSchema,
  activity: z.string().trim().min(1).max(AGENT_TASK_LIMITS.activityChars).optional().describe(
    "Short human-readable progress note — not a substitute for interimEvidence.",
  ),
  interimEvidence: AgentResultEvidenceListSchema.optional().describe(
    "Evidence surfaced so far in a still-running task — reporting this does not terminate the task.",
  ),
  occurredAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentLifecycleEvent = z.infer<typeof AgentLifecycleEventSchema>;

export const AgentCancellationRequestSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  requestId: AgentProtocolIdentifierSchema,
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  requestedBy: AgentIdentitySchema,
  reason: AgentCancellationReasonSchema,
  requestedAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentCancellationRequest = z.infer<typeof AgentCancellationRequestSchema>;

export const AgentCancellationAckSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  requestId: AgentProtocolIdentifierSchema,
  taskId: AgentProtocolIdentifierSchema,
  acknowledgedBy: AgentAckAuthorSchema,
  accepted: z.boolean(),
  status: AgentTaskStatusSchema,
  reason: AgentCancellationReasonSchema.optional(),
  acknowledgedAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict().superRefine((ack, ctx) => {
  if (!ack.accepted && ack.reason == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["reason"],
      message: "Rejected cancellation acknowledgements require a reason",
    });
  }
});
export type AgentCancellationAck = z.infer<typeof AgentCancellationAckSchema>;

export const AgentErrorDetailSchema = z.object({
  code: z.string().trim().min(1).max(AGENT_TASK_LIMITS.errorCodeChars).regex(
    /^[A-Z][A-Z0-9_]*$/,
    "agent error code must be an uppercase stable identifier",
  ),
  message: z.string().trim().min(1).max(AGENT_TASK_LIMITS.errorMessageChars),
  /** null means the runtime owner did not classify retry safety. */
  recoverable: z.boolean().nullable(),
}).strict();
export type AgentErrorDetail = z.infer<typeof AgentErrorDetailSchema>;

export const AgentErrorSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  errorId: AgentProtocolIdentifierSchema,
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  agent: AgentIdentitySchema,
  ...AgentErrorDetailSchema.shape,
  occurredAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentError = z.infer<typeof AgentErrorSchema>;

const TerminalOutcomeBaseShape = {
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  outcomeId: AgentProtocolIdentifierSchema,
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  from: AgentIdentitySchema,
  to: AgentIdentitySchema,
  occurredAt: AgentProtocolTimestampSchema,
  extensions: AgentExtensionsSchema.optional(),
} as const;

export const AgentTerminalOutcomeSchema = z.discriminatedUnion("status", [
  z.object({
    ...TerminalOutcomeBaseShape,
    status: z.literal("done"),
    result: AgentResultSchema,
  }).strict(),
  z.object({
    ...TerminalOutcomeBaseShape,
    status: z.literal("failed"),
    error: AgentErrorDetailSchema,
  }).strict(),
  z.object({
    ...TerminalOutcomeBaseShape,
    status: z.enum(["cancelled", "expired"]),
    reason: AgentOutcomeReasonSchema,
  }).strict(),
]);
export type AgentTerminalOutcome = z.infer<typeof AgentTerminalOutcomeSchema>;
