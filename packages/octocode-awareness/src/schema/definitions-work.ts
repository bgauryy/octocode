/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { z } from 'zod';
import { TASK_STATUSES } from '@octocodeai/agent-contracts/entities';
import {
  agentId, nonEmptyText, workspacePath, artifactScope, targetFiles, repoScope, refScope,
} from './common.js';

export const workSchemas = {
  agents: z
    .object({
      workspace: workspacePath.optional().describe("Host-bound workspace used for linked-checkout visibility."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repository scope filter."),
      ref: refScope.optional().describe("Git ref scope filter."),
      query: z.string().trim().max(1000).default("").describe("Agent id, name, or context filter."),
      limit: z.number().int().min(1).max(500).default(50),
      offset: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).default(0)
        .describe("Zero-based offset in the stable, deduplicated agent projection."),
    })
    .strict()
    .describe("List every visible agent identity in the bound store and workspace scope."),
task: z
    .object({
      action: z.enum(["create", "list", "ready", "show", "claim", "heartbeat", "submit", "release", "depend", "retry"]),
      task_id: z.string().trim().min(1).max(128).optional(),
      plan_id: z.string().trim().min(1).max(128).optional(),
      workspace: z.string().trim().min(1).max(1024).optional().describe("Workspace filter for list/ready (matches the owning plan's workspace_path)."),
      run_id: z.string().trim().min(1).max(128).optional(),
      title: nonEmptyText("Task title.", 300).optional(),
      reasoning: nonEmptyText("Why this work exists and what decisions constrain it.", 4000).optional(),
      acceptance: nonEmptyText("Done/verification criteria.", 4000).optional(),
      path: z.array(z.string().trim().min(1).max(1024)).max(200).default([]),
      depends_on: z.array(z.string().trim().min(1).max(128)).max(200).default([]),
      agent_id: agentId.optional(),
      priority: z.number().int().min(-1000).max(1000).default(0),
      lease_minutes: z.number().int().min(1).max(60).default(30),
      test_plan: nonEmptyText("Run verification plan.", 4000).optional(),
      message: nonEmptyText("Submission message.", 2000).optional(),
      blocked_reason: nonEmptyText("Why the task is blocked.", 2000).optional(),
      status: z.enum(TASK_STATUSES).optional(),
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
      limit: z.number().int().min(1).max(200).optional().describe("Maximum work rows per page; defaults to 5 compact or 20 otherwise."),
      offset: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional().describe("Continue from the offset in the preceding work page."),
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
  lock_release: z
    .object({
      agent_id: agentId,
      run_id: z.string().trim().min(1).max(128).optional(),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      target_files: z.array(z.string().trim().min(1).max(1024)).max(200).optional(),
      status: z.enum(["PENDING", "FAILED"]).default("PENDING")
        .describe("End editing; use verify mark with a receipt for SUCCESS."),
    })
    .strict()
    .refine((value) => value.run_id !== undefined || (value.target_files?.length ?? 0) > 0, {
      message: "run_id or target_files is required.",
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
        .describe("Exact runs covered by the observed check; required unless all_pending is true."),
      all_pending: z.boolean().default(false).describe("Select all pending runs in workspace/artifact scope only when the observed check covers every selected run."),
      adopt_verification: z.boolean().default(false).describe("Explicitly let this actor verify one run owned by another actor; requires one run_id and workspace."),
      status: z.enum(["SUCCESS", "FAILED"]).default("SUCCESS").describe("Observed check result. Unrun checks remain pending; never use SUCCESS to clear debt."),
      message: z.string().trim().min(1).max(2000).optional().describe("Observed command and result; required for SUCCESS. Worker confidence alone is not a receipt."),
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
      if (value.adopt_verification && (value.all_pending || value.run_id.length !== 1 || !value.workspace)) {
        ctx.addIssue({ code: "custom", path: ["adopt_verification"], message: "adopt_verification requires one run_id and workspace." });
      }
    })
    .describe("Record an observed check against selected runs. Ending work leaves it pending; only evidence establishes SUCCESS or FAILED."),
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
      limit: z.number().int().min(1).max(200).optional()
        .describe("Maximum verification rows returned; compact mode defaults to 20."),
      offset: z.number().int().min(0).max(1_000_000).optional()
        .describe("Zero-based verification row offset; copy the returned continuation."),
    })
    .strict()
              .describe("Read-only listing of unverified and stale ACTIVE runs."),
};
