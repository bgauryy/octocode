import { z } from "zod/v3";

export const AGENT_TASK_LIMITS = Object.freeze({
  objectiveChars: 4_000,
  successCriteria: 4,
  successCriterionChars: 600,
  contextChars: 16_000,
  activityChars: 200,
  reasonChars: 1_000,
  cancellationReasonChars: 500,
  errorCodeChars: 128,
  errorMessageChars: 2_000,
});

export const AgentTaskObjectiveSchema = z.string().trim().min(1).max(
  AGENT_TASK_LIMITS.objectiveChars,
).describe("The single question this task must answer — specific enough to grade, not the full briefing.");
export const AgentTaskSuccessCriteriaSchema = z.array(
  z.string().trim().min(1).max(AGENT_TASK_LIMITS.successCriterionChars),
).max(AGENT_TASK_LIMITS.successCriteria).describe(
  "Observable conditions that make this task done — not restatements of the objective.",
);
export const AgentTaskRequiredSuccessCriteriaSchema = AgentTaskSuccessCriteriaSchema.min(1);
export const AgentTaskContextSchema = z.string().trim().min(1).max(
  AGENT_TASK_LIMITS.contextChars,
).describe("Material facts the recipient needs and cannot otherwise infer — not a chat transcript dump.");
export const AgentCancellationReasonSchema = z.string().trim().min(1).max(
  AGENT_TASK_LIMITS.cancellationReasonChars,
);
export const AgentOutcomeReasonSchema = z.string().trim().min(1).max(
  AGENT_TASK_LIMITS.reasonChars,
);

/**
 * Fit arbitrary runtime text into one of this protocol's own declared bounds,
 * falling back when the source is empty/undefined. Every projector (local or
 * remote) needs this identically, since the bound belongs to the protocol,
 * not to whichever side happens to be mapping text into it.
 */
export function boundProtocolText(value: string | undefined, fallback: string, max: number): string {
  return (value?.trim() || fallback).slice(0, max);
}
