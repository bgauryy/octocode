/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { z } from 'zod';
import {
  agentId, nonEmptyText, workspacePath, artifactScope, importanceLevel,
  notificationKind, fileList, refIds, evalFailure,
} from './common.js';

export const operationSchemas = {
refinement: z
    .object({
      agent_id: agentId,
      refinement_id: z.string().trim().min(1).max(128).optional()
        .describe("Existing refinement to update; omit when creating."),
      workspace_path: z
        .string()
        .trim()
        .min(1)
        .max(1024)
        .optional()
        .describe("Workspace root; required when creating."),
      artifact: artifactScope.optional(),
      repo: z.string().trim().min(1).max(256).optional().describe("Repo."),
      ref: z.string().trim().min(1).max(256).optional().describe("Ref."),
      files: z
        .array(z.string().trim().min(1).max(1024))
        .max(200)
        .optional()
        .describe("Related files."),
      reasoning: nonEmptyText("Why saved.", 2000).optional(),
      remember: nonEmptyText("What to remember.", 2000).optional(),
      quality: z.enum(["good", "bad", "handoff", "instructions"]).optional().describe("Outcome quality."),
      state: z
        .enum(["open", "ongoing", "done"])
        .optional()
        .describe("Lifecycle state; terminal done is update-only."),
      check_receipt: nonEmptyText("Verification evidence for terminal closure.", 2000).optional(),
    })
    .strict()
    .superRefine((value, ctx) => {
      if (!value.refinement_id) {
        for (const field of ["workspace_path", "reasoning", "remember"] as const) {
          if (!value[field]) ctx.addIssue({ code: "custom", path: [field], message: `${field} is required when creating.` });
        }
        if (value.state === "done") {
          ctx.addIssue({ code: "custom", path: ["state"], message: "Create open/ongoing, then close the existing refinement with a receipt." });
        }
        return;
      }
      const updateFields: Array<keyof typeof value> = ["state", "quality", "reasoning", "remember", "files"];
      if (!updateFields.some((field) => value[field] !== undefined)) {
        ctx.addIssue({ code: "custom", message: "An update requires at least one changed field." });
      }
      if (value.state === "done" && !value.check_receipt?.trim()) {
        ctx.addIssue({ code: "custom", path: ["check_receipt"], message: "Terminal closure requires a check receipt." });
      }
    })
    .describe("Create or update a refinement; terminal closure is evidence-gated."),
  refine_query: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      refinement_id: z.string().trim().min(1).max(128).optional(),
      artifact: artifactScope.optional(),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      quality: z.enum(["good", "bad", "handoff", "instructions"]).optional(),
      include_handoffs: z.boolean().default(false).describe("Include session handoff refinements."),
      states: z
        .array(z.enum(["open", "ongoing", "done"]))
        .default(["open", "ongoing"])
        .describe("States."),
      limit: z.number().int().min(1).max(200).default(20),
      include_env: z
        .boolean()
        .default(false)
        .describe("Include env."),
    })
    .strict()
    .describe("Read refinements."),
  refine_delete: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      artifact: artifactScope.optional(),
      refinement_id: z
        .array(z.string().trim().min(1).max(128))
        .min(1)
        .max(200)
        .describe("Refinement ids."),
      dry_run: z.boolean().default(false).describe("Preview only."),
    })
    .strict()
    .describe("Delete refinements."),
  agent_signal: z
    .object({
      action: z.enum(["publish", "list", "reply", "resolve", "ack"]).describe("Action."),
      agent_id: agentId.describe("Actor."),
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Workspace channel."),
      artifact: artifactScope.optional(),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      kind: notificationKind.optional().describe("Signal kind."),
      subject: nonEmptyText("Subject.", 200).optional(),
      body: nonEmptyText("Body.", 4000).optional(),
      to_agents: z.array(agentId).max(50).default([]).describe("Recipients."),
      files: fileList,
      refs: refIds,
      importance: importanceLevel.default(5),
      in_reply_to: z.string().trim().min(1).max(128).optional().describe("Reply target."),
      thread_id: z.string().trim().min(1).max(128).optional().describe("Thread id."),
      signal_id: z.array(z.string().trim().min(1).max(128)).max(200).default([]).describe("Signal ids."),
      unread_only: z.boolean().default(true),
      mark_read: z.boolean().default(false),
      kinds: z.array(notificationKind).max(8).default([]),
      limit: z.number().int().min(1).max(200).default(20),
      include_bodies: z
        .boolean()
        .default(false)
        .describe("Include full signal bodies on list (default summarizes to 160 chars)."),
      format: z
        .enum(["json", "hook"])
        .default("json")
        .describe("list only: json rows, or hook briefing shape for host notify delivery."),
    })
    .strict()
    .describe("Signal actions."),
  signal_prune: z
    .object({
      agent_id: agentId,
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Workspace channel."),
      artifact: artifactScope.optional(),
      signal_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Signal ids."),
      resolved: z.literal(true).describe("Only resolved messages may be pruned."),
      older_than_days: z
        .number()
        .int()
        .min(1)
        .max(3650)
        .describe("Required age cutoff."),
      dry_run: z.boolean().default(false).describe("Preview only."),
    })
    .strict()
    .describe("Preview or prune old resolved signals owned by a thread participant."),
  agent_registry: z
    .object({
      action: z.enum(["list", "register"]).default("list").describe("Action."),
      agent_id: agentId.optional().describe("Agent id."),
      agent_name: z.string().trim().max(256).optional().describe("Display name."),
      workspace: workspacePath.optional().describe("Workspace scope."),
      artifact: artifactScope.optional(),
      context: z.string().trim().min(1).max(64).optional().describe("Host context."),
      limit: z.number().int().min(1).max(200).default(50).describe("Row limit."),
    })
    .strict()
    .refine((d) => d.action !== "register" || d.agent_id !== undefined, {
      message: "agent_id is required when action is register.",
    })
    .describe("Agent registry."),
  reflect: z
    .object({
      agent_id: agentId,
      task: nonEmptyText("Task summary.", 2000),
      outcome: z.enum(["worked", "partial", "failed"]).describe("Outcome."),
      worked: nonEmptyText("What worked.", 2000).optional(),
      didnt_work: nonEmptyText("What didn't work.", 2000).optional(),
      judgment_note: nonEmptyText("Evidence/uncertainty note.", 2000).optional(),
      lesson: nonEmptyText("Reusable lesson.", 4000).optional(),
      failure_signature: z
        .string()
        .trim()
        .min(1)
        .max(256)
        .optional()
        .describe("Failure cluster key."),
      eval_failures: z
        .array(evalFailure)
        .max(20)
        .default([])
        .describe("Eval failures."),
      fix_repo: nonEmptyText("Repo fix note.", 2000).optional(),
      fix_file: fileList,
      fix_harness: nonEmptyText("Harness fix note.", 2000).optional(),
      fix_instructions: nonEmptyText("Developer instructions feedback.", 2000).optional(),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      artifact: artifactScope.optional(),
      importance: importanceLevel.optional().describe("Importance override."),
      duo: z
        .boolean()
        .optional()
        .describe("Advisory duo packet."),
      allow_similar: z.boolean().default(false)
        .describe("Keep a materially distinct recurrence despite the duplicate gate."),
    })
    .strict()
    .describe("Reflect after work.")
};
