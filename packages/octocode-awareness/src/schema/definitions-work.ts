/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { z } from 'zod';
import {
  agentId, nonEmptyText, tags, workspacePath, artifactScope, repoScope,
  refScope, targetFiles,
} from './common.js';

export const workSchemas = {
task: z
    .object({
      action: z.enum(["create", "list", "ready", "show", "claim", "heartbeat", "submit", "release", "depend"]),
      task_id: z.string().trim().min(1).max(128).optional(),
      plan_id: z.string().trim().min(1).max(128).optional(),
      workspace: z.string().trim().min(1).max(1024).optional().describe("Workspace filter for list/ready (matches the owning plan's workspace_path)."),
      run_id: z.string().trim().min(1).max(128).optional(),
      title: nonEmptyText("Task title.", 300).optional(),
      reasoning: nonEmptyText("Why this work exists and what decisions constrain it.", 4000).optional(),
      acceptance: nonEmptyText("Done/verification criteria.", 4000).optional(),
      path: z.array(z.string().trim().min(1).max(1024)).max(200).default([]),
      depends_on: z.array(z.string().trim().min(1).max(128)).max(200).default([]),
      created_by: agentId.optional(),
      agent_id: agentId.optional(),
      priority: z.number().int().min(-1000).max(1000).default(0),
      lease_minutes: z.number().int().min(1).max(60).default(30),
      test_plan: nonEmptyText("Run verification plan.", 4000).optional(),
      message: nonEmptyText("Submission message.", 2000).optional(),
      blocked_reason: nonEmptyText("Why the task is blocked.", 2000).optional(),
      status: z.enum(["OPEN", "IN_PROGRESS", "BLOCKED", "VERIFY", "DONE", "FAILED", "CANCELLED"]).optional(),
      next: z.boolean().default(false),
    })
    .strict()
    .superRefine((value, ctx) => {
      const required = (field: keyof typeof value) => {
        if (value[field] === undefined || value[field] === "") ctx.addIssue({ code: "custom", path: [field], message: `${field} is required for ${value.action}` });
      };
      if (value.action === "create") {
        for (const field of ["plan_id", "title", "reasoning", "agent_id"] as const) required(field);
        if (value.path.length === 0) ctx.addIssue({ code: "custom", path: ["path"], message: "at least one path is required for create" });
      }
      if (["show", "heartbeat", "submit", "release", "depend"].includes(value.action)) required("task_id");
      if (value.action === "claim" && !value.next) required("task_id");
      if (value.action === "claim" && value.next) required("plan_id");
      if (["claim", "heartbeat", "submit", "release", "depend"].includes(value.action)) required("agent_id");
      if (["heartbeat", "submit", "release"].includes(value.action)) required("run_id");
      if (value.action === "depend" && value.depends_on.length === 0) ctx.addIssue({ code: "custom", path: ["depends_on"], message: "at least one dependency is required" });
    })
    .describe("Create, choose, claim, and complete durable plan tasks."),
  work: z
    .object({
      action: z.enum(["start", "touch", "end", "list", "show"]),
      agent_id: agentId.optional(),
      session_id: z.string().trim().min(1).max(256).optional(),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      run_id: z.string().trim().min(1).max(128).optional(),
      rationale: nonEmptyText("Why these files are under work.", 2000).optional(),
      test_plan: nonEmptyText("Verification plan.", 2000).optional(),
      context_ref: z.string().trim().min(1).max(1024).optional(),
      target_files: z.array(z.string().trim().min(1).max(1024)).max(200).default([]),
      exclusive: z.boolean().default(false),
      ttl_minutes: z.number().int().min(1).max(60).default(10),
      ttl_seconds: z.number().int().min(1).max(3600).optional(),
      all: z.boolean().default(false),
      full: z.boolean().default(false),
    })
    .strict()
    .superRefine((value, ctx) => {
      const required = (field: keyof typeof value) => {
        if (value[field] === undefined || value[field] === "") ctx.addIssue({ code: "custom", path: [field], message: `${field} is required for ${value.action}` });
      };
      if (["start", "touch", "end"].includes(value.action)) required("agent_id");
      if (value.action === "start") {
        if (value.target_files.length === 0) ctx.addIssue({ code: "custom", path: ["target_files"], message: "at least one target file is required for start" });
        if (!value.run_id) for (const field of ["rationale", "test_plan"] as const) required(field);
      }
      if (["touch", "end"].includes(value.action)) required("run_id");
      if (value.action === "show" && value.target_files.length !== 1) ctx.addIssue({ code: "custom", path: ["target_files"], message: "exactly one target file is required for show" });
    })
    .describe("Declare, heartbeat, inspect, or end advisory file work; exclusivity is opt-in."),
  lock_wait: z
    .object({
      agent_id: agentId.describe("Waiting agent."),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      target_files: targetFiles,
      wait_seconds: z.number().int().min(0).max(3600).default(60),
      retry_interval: z.number().int().min(1).max(300).default(5),
    })
    .strict()
    .describe("Wait for locks."),
  lock_prune: z
    .object({
      older_than_minutes: z.number().int().min(1).max(10080).default(20),
      expired_only: z.boolean().default(false),
      agent_id: agentId.optional().describe("Holder filter."),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      target_files: z.array(z.string().trim().min(1).max(1024)).max(200).default([]),
      dry_run: z.boolean().default(false),
    })
    .strict()
    .describe("Prune stale locks."),
  mine_weakness: z
    .object({
      agent_id: agentId.optional().describe("Agent filter."),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      min_count: z.number().int().min(1).max(100).default(2),
      limit: z.number().int().min(1).max(200).default(20),
      cwd: z.string().trim().min(1).max(1024).optional().describe("Scope cwd."),
    })
    .strict()
    .describe("Mine failure clusters."),
  doc_staleness: z
    .object({
      targets_json: z
        .string()
        .trim()
        .min(1)
        .max(20000)
        .describe("Doc/source JSON."),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      min_edits: z.number().int().min(1).max(10000).default(5),
      min_lines: z.number().int().min(1).max(1000000).default(50),
      propose: z.boolean().default(false),
      agent_id: agentId.optional(),
      session_id: z.string().trim().min(1).max(128).optional(),
    })
    .strict()
    .describe("Check doc staleness."),
  docs_catalog: z
    .object({
      action: z.enum(["list", "show"]).default("list"),
      name: z.string().trim().min(1).max(256).optional().describe("Skill-ref name for docs show."),
      full: z.boolean().default(false).describe("Include abs path/root on docs list."),
    })
    .strict()
    .describe("List or show skill reference docs."),
  digest: z
    .object({
      retention_days: z.number().int().min(1).max(3650).default(90),
      refinement_handoff_retention_days: z.number().int().min(1).max(3650).default(7),
      refinement_done_retention_days: z.number().int().min(1).max(3650).default(30),
      operational_retention_days: z.number().int().min(1).max(3650).default(90)
        .describe("Compact old terminal standalone WORK/HOOK rows; receipts remain."),
      pressure_age_days: z.number().int().min(1).max(3650).default(1)
        .describe("Report old pending runs/signals/missing refs without mutating them."),
      dry_run: z.boolean().default(false),
      export_doc: z.union([z.boolean(), z.string().trim().min(1).max(1024)]).optional()
        .describe("Write report."),
      workspace: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
    })
    .strict()
    .describe("Prune/archive/reindex."),
  lock_release: z
    .object({
      agent_id: agentId,
      run_id: z.string().trim().min(1).max(128).optional(),
      lock_id: z.string().trim().min(1).max(128).optional()
        .describe("Lock id from lock acquire; resolved to its run and file."),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      target_files: z.array(z.string().trim().min(1).max(1024)).max(200).optional(),
      status: z.enum(["PENDING", "FAILED"]).default("PENDING")
        .describe("End editing; use verify mark with a receipt for SUCCESS."),
    })
    .strict()
    .refine((value) => value.run_id !== undefined || value.lock_id !== undefined || (value.target_files?.length ?? 0) > 0, {
      message: "run_id, lock_id, or target_files is required.",
    })
    .describe("Release locks."),
  verify: z
    .object({
      agent_id: agentId,
      workspace: z.string().trim().min(1).max(1024).optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      run_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Run ids."),
      all_pending: z.boolean().default(false).describe("All pending runs."),
      status: z.enum(["SUCCESS", "FAILED"]).default("SUCCESS").describe("Verification status."),
      message: z.string().trim().min(1).max(2000).optional().describe("Verification note."),
    })
    .strict()
    .superRefine((value, ctx) => {
      if (!value.all_pending && value.run_id.length === 0) {
        ctx.addIssue({ code: "custom", message: "run_id or all_pending is required." });
      }
      if (value.status === "SUCCESS" && !value.message?.trim()) {
        ctx.addIssue({ code: "custom", path: ["message"], message: "SUCCESS requires an evidence receipt in message." });
      }
      if (value.all_pending && !value.workspace && !value.artifact) {
        ctx.addIssue({ code: "custom", path: ["all_pending"], message: "all_pending requires workspace or artifact scope." });
      }
    })
    .describe("Mark verified."),
  verify_audit: z
    .object({
      agent_id: agentId,
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
                older_than_days: z.number().int().min(1).max(3650).optional()
                  .describe("Only include debt older than this age."),
      origin: z.array(z.enum(["TASK", "WORK", "HOOK"])).max(3).default([])
        .describe("Restrict migration/audit to selected run origins."),
      before: z.string().datetime().optional()
        .describe("Only runs created before this ISO timestamp."),
    })
    .strict()
              .describe("Read-only listing of unverified and stale ACTIVE runs."),
  forget_memory: z
    .object({
      memory_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Memory ids."),
      tags,
      before: z
        .string()
        .trim()
        .min(1)
        .max(64)
        .optional()
        .describe("Before ISO."),
      max_importance: z
        .number()
        .int()
        .min(1)
        .max(10)
        .optional()
        .describe("Importance ceiling."),
      workspace_path: workspacePath.optional().describe("Workspace scope."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo scope."),
      ref: refScope.optional().describe("Ref scope."),
      dry_run: z.boolean().default(false).describe("Preview only."),
    })
    .strict()
    .refine(
      (data) =>
        data.memory_id.length > 0 ||
        data.tags.length > 0 ||
        data.before !== undefined ||
        data.max_importance !== undefined,
      { message: "forget requires at least one selector: memory_id, tags, before, or max_importance." },
    )
    .describe("Forget memories."),
  memory_lifecycle: z
    .object({
      action: z.enum(["archive", "restore"]).describe("Lifecycle operation selected by the CLI noun/verb command."),
      memory_id: z.array(z.string().trim().min(1).max(128)).min(1).max(200)
        .describe("Explicit memory ids; lifecycle changes never use broad selectors."),
      workspace_path: workspacePath.optional().describe("Workspace scope."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo scope."),
      ref: refScope.optional().describe("Ref scope."),
      dry_run: z.boolean().default(false).describe("Preview selected ids without mutation."),
    })
    .strict()
    .describe("Reversibly archive memories or restore archived rows; replacement history cannot be restored.")
};
