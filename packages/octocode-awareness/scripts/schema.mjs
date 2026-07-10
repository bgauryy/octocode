#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { z } from "zod";

const agentId = z.string().min(1).max(128).describe("Agent id.");
const nonEmptyText = (description, max = 4000) => z.string().trim().min(1).max(max).describe(description);
const tag = z
  .string()
  .trim()
  .min(1)
  .max(64)
  .regex(/^[a-zA-Z0-9_.:-]+$/, "tags may contain letters, numbers, underscore, dot, colon, or dash");
const tags = z.array(tag).max(32).default([]).describe("Filter tags.");
const workspacePath = z.string().trim().min(1).max(1024).describe("Workspace root.");
const artifactScope = z.string().trim().min(1).max(256).describe("Artifact/package scope.");
const repoScope = z.string().trim().min(1).max(256).describe("Repo slug.");
const refScope = z.string().trim().min(1).max(256).describe("Branch/tag/SHA.");
const references = z
  .array(z.string().trim().min(1).max(512))
  .max(20)
  .default([])
  .describe("Provenance refs, e.g. file:/abs/path, pr:owner/repo#1, URL.");
const MEMORY_LABELS = [
  "BUG",
  "FEATURE",
  "SUGGESTION",
  "GOTCHA",
  "IMPROVEMENT",
  "DECISION",
  "ARCHITECTURE",
  "SECURITY",
  "PERFORMANCE",
  "TEST",
  "BUILD",
  "DOCS",
  "CONFIG",
  "WORKFLOW",
  "REFACTOR",
  "API",
  "RELEASE",
  "INCIDENT",
  "EXPERIENCE",
  "OVERRIDE",
  "OTHER",
];
const normalizeMemoryLabel = (value) => {
  if (typeof value !== "string") return value;
  const cleaned = value.trim().toUpperCase().replace(/[\s-]+/g, "_");
  return cleaned || "OTHER";
};
const memoryLabel = z
  .preprocess(normalizeMemoryLabel, z.enum(MEMORY_LABELS).default("OTHER"))
  .describe("Memory label.");
const memorySort = z
  .enum(["smart", "score", "importance", "recent", "accessed"])
  .default("smart")
  .describe("Sort order.");
const importanceLevel = z
  .number()
  .int()
  .min(1)
  .max(10)
  .describe("1-10 importance.");
const targetFiles = z
  .array(z.string().trim().min(1).max(1024))
  .min(1)
  .max(200)
  .describe("Files to lock.");
const awarenessQueryView = z
  .enum([
    "all",
    "repo-profile",
    "memories",
    "gotchas",
    "lessons",
    "plans",
    "tasks",
    "runs",
    "locks",
    "agents",
    "signals",
    "refinements",
    "files",
    "activity",
    "workboard",
    "developer-review",
  ])
  .default("all")
  .describe("Awareness read view.");
const awarenessOutputFormat = z
  .enum(["json", "table", "csv", "markdown", "html"])
  .default("json")
  .describe("Output format.");
const repoContextMode = z
  .enum(["local", "share"])
  .default("local")
  .describe("Whether generated .octocode is intended as local-only or shared repo context. The command never edits .gitignore.");

// Signals — repo-scoped agent-to-agent messages. The `kind` enum is the
// "smart" part: typed messages let recipients filter (e.g. only blockers) and
// act, instead of parsing free prose.
const NOTIFICATION_KINDS = [
  "claim", // "I'm taking these files / this area"
  "handoff", // "finished X, you can start Y" (pair with a refinement id in refs)
  "question", // ask another agent something
  "reply", // answer within a thread
  "blocker", // "don't touch X — mid-change / broken"
  "request", // "can you run Y / verify Z"
  "decision", // "chose approach Z" — broadcast a call others should know
  "fyi", // low-stakes heads-up
];
const notificationKind = z.enum(NOTIFICATION_KINDS);
const fileList = z
  .array(z.string().trim().min(1).max(1024))
  .max(200)
  .default([])
  .describe("Related files.");
const refIds = z
  .array(z.string().trim().min(1).max(128))
  .max(50)
  .default([])
  .describe("Related ids.");
const evalFailure = z
  .object({
    id: z.string().trim().min(1).max(128).describe("Eval id."),
    dimension: z.string().trim().min(1).max(128).optional().describe("Eval dimension."),
    failure_signature: z
      .string()
      .trim()
      .min(1)
      .max(256)
      .optional()
      .describe("Failure cluster key."),
    suggested_lesson: z.string().trim().min(1).max(1000).optional().describe("Eval lesson."),
  })
  .strict()
  .refine((d) => d.failure_signature !== undefined || d.suggested_lesson !== undefined, {
    message: "eval failure needs failure_signature or suggested_lesson.",
  });

export const schemas = {
  tell_memory: z
    .object({
      agent_id: agentId,
      task_context: nonEmptyText("Source task.", 1000),
      observation: nonEmptyText("Reusable lesson.", 4000),
      importance: importanceLevel,
      label: memoryLabel,
      tags,
      references,
      workspace_path: workspacePath
        .optional()
        .describe("Memory scope."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo scope."),
      ref: refScope.optional().describe("Ref scope."),
      file: z
        .string()
        .trim()
        .min(1)
        .max(1024)
        .optional()
        .describe("Primary file."),
      file_tree_fingerprint: z
        .string()
        .trim()
        .min(1)
        .max(256)
        .optional()
        .describe("Workspace hash."),
      supersedes: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Memory ids replaced."),
      failure_signature: z
        .string()
        .trim()
        .min(1)
        .max(256)
        .optional()
        .describe("Failure cluster key."),
      valid_from: z.string().trim().min(1).max(64).optional().describe("Valid from ISO."),
      valid_to: z.string().trim().min(1).max(64).optional().describe("Valid until ISO."),
      allow_similar: z.boolean().default(false)
        .describe("Keep a materially distinct recurrence despite the duplicate gate."),
    })
    .strict()
    .describe("Record a memory."),

  get_memory: z
    .object({
      query: z.string().trim().max(1000).default("").describe("Recall query."),
      limit: z.number().int().min(1).max(50).default(3),
      min_importance: z.number().int().min(1).max(10).default(1),
      labels: z.array(memoryLabel).max(32).default([]).describe("Labels."),
      tags,
      files: z
        .array(z.string().trim().min(1).max(1024))
        .max(200)
        .default([])
        .describe("Exact file refs."),
      file_regex: z
        .array(z.string().trim().min(1).max(512))
        .max(20)
        .default([])
        .describe("File regex filters."),
      regex: z
        .array(z.string().trim().min(1).max(512))
        .max(20)
        .default([])
        .describe("Text regex filters."),
      references: references.describe("Exact refs; all must match."),
      workspace_path: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo filter."),
      ref: refScope.optional().describe("Ref filter."),
      strict_scope: z
        .boolean()
        .default(false)
        .describe("Exact scope only."),
      global_only: z.boolean().default(false).describe("Only unscoped rows."),
      sort: memorySort,
      smart: z
        .boolean()
        .default(false)
        .describe("Broaden recall."),
      states: z
        .array(z.enum(["ACTIVE", "SUPERSEDED"]))
        .default(["ACTIVE"])
        .describe("Memory states."),
      explain: z
        .boolean()
        .default(false)
        .describe("Include score parts."),
      semantic: z
        .boolean()
        .default(false)
        .describe("Request semantic recall."),
      full: z
        .boolean()
        .default(false)
        .describe("Return full MemoryRecord rows (default is lean projection)."),
      as_of: z
        .string()
        .trim()
        .min(1)
        .max(64)
        .optional()
        .describe("Point-in-time ISO."),
    })
    .strict()
    .describe("Recall memories."),

  workspace_status: z
    .object({
      workspace: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      limit: z.number().int().min(1).max(200).default(20),
    })
    .strict()
    .describe("Workspace status."),

  query: z
    .object({
      view: awarenessQueryView,
      query: z.string().trim().max(1000).default("").describe("Text filter."),
      limit: z.number().int().min(1).max(500).default(50),
      format: awarenessOutputFormat,
      out: z.string().trim().min(1).max(1024).optional().describe("Optional output path."),
      workspace_path: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo filter."),
      ref: refScope.optional().describe("Ref filter."),
      agent_id: agentId.optional().describe("Agent filter."),
      state: z.array(z.string().trim().min(1).max(64)).max(20).default([]).describe("State/status filters."),
      label: z.array(memoryLabel).max(32).default([]).describe("Memory label filters."),
      file: z.string().trim().min(1).max(1024).optional().describe("File filter."),
      since: z.string().trim().min(1).max(64).optional().describe("Created/updated since ISO."),
      include_bodies: z.boolean().default(false).describe("Include full signal bodies."),
    })
    .strict()
    .describe("Query awareness views for agents, scripts, and humans."),

  attend: z
    .object({
      query: z.string().trim().max(1000).default("").describe("Current task, risk, or design question."),
      limit: z.number().int().min(1).max(50).default(10).describe("Rows per workboard column and evidence cap."),
      workspace_path: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo filter."),
      ref: refScope.optional().describe("Ref filter."),
      file: z.array(z.string().trim().min(1).max(1024)).max(50).default([]).describe("File filters."),
      include_bodies: z.boolean().default(false).describe("Include full signal bodies in routed reads."),
      explain_organ: z.boolean().default(false).describe("Include/retain organ-state explanation fields."),
    })
    .strict()
    .describe("Build a bounded read-only lobby: actionable workboard state, relevant evidence/gaps, and one next command; noncompact mode adds diagnostics."),

  repo_inject: z
    .object({
      workspace_path: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo filter."),
      ref: refScope.optional().describe("Ref filter."),
      query: z.string().trim().max(1000).default("").describe("Optional text filter."),
      limit: z.number().int().min(1).max(500).default(50),
      out_dir: z.string().trim().min(1).max(1024).optional().describe("Output directory, defaults to <workspace>/.octocode."),
      mode: repoContextMode,
      include_view: z.boolean().default(true).describe("Write awareness/index.html."),
      check: z.boolean().default(true).describe("Report gitignore/share policy warnings."),
    })
    .strict()
    .describe("Generate .octocode repo context projections without editing .gitignore."),

  export_harness: z
    .object({
      limit: z.number().int().min(1).max(200).default(10),
      min_importance: z.number().int().min(1).max(10).default(7),
      workspace: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
    })
    .strict()
    .describe("Export AGENTS block."),

  developer_review: z
    .object({
      workspace: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional(),
      ref: refScope.optional(),
      state: z.union([z.string(), z.array(z.string())]).optional().describe("Filter by refinement state: open|ongoing|done."),
      limit: z.number().int().min(1).max(500).default(60),
      format: z.enum(["json", "markdown"]).default("json").describe("json rows or the markdown digest."),
      query: z.string().trim().min(1).max(200).optional().describe("Text filter over feedback."),
    })
    .strict()
    .describe("Read agent feedback to the instruction author (from reflect record --fix-instructions)."),

  session_capture: z
    .object({
      agent_id: agentId.optional().describe("Agent filter."),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      repo: repoScope.optional(),
      ref: refScope.optional(),
      reason: z.string().trim().min(1).max(500).optional().describe("Capture reason."),
      cwd: z.string().trim().min(1).max(1024).optional().describe("Scope cwd."),
    })
    .strict()
    .describe("Capture session handoff."),

  pre_flight_intent: z
    .object({
      agent_id: agentId,
      workspace: z.string().trim().min(1).max(1024).optional().describe("Workspace scope."),
      artifact: artifactScope.optional(),
      run_id: z.string().trim().min(1).max(128).optional().describe("Existing claimed run to attach locks to."),
      context_ref: z.string().trim().min(1).max(1024).optional(),
      rationale: nonEmptyText("Why edit.", 2000),
      target_files: targetFiles,
      test_plan: nonEmptyText("Verification plan.", 2000),
      wait_seconds: z.number().int().min(0).max(3600).default(0),
      retry_interval: z.number().int().min(1).max(300).default(5),
      ttl_minutes: z.number().int().min(1).max(10).default(10)
        .describe("Lock TTL minutes."),
      ttl_seconds: z.number().int().min(1).max(600).optional()
        .describe("Lock TTL seconds; overrides ttl_minutes when provided."),
    })
    .strict()
    .describe("Acquire exclusive protection for sensitive files."),

  plan: z
    .object({
      action: z.enum(["create", "list", "show", "join", "doc", "status"]),
      plan_id: z.string().trim().min(1).max(128).optional(),
      name: nonEmptyText("Plan name.", 200).optional(),
      objective: nonEmptyText("Plan objective.", 4000).optional(),
      lead_agent_id: agentId.optional(),
      agent_id: agentId.optional(),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      status: z.enum(["DRAFT", "ACTIVE", "PAUSED", "COMPLETED", "CANCELLED"]).optional(),
      path: z.string().trim().min(1).max(1024).optional(),
      title: nonEmptyText("Supporting document title.", 300).optional(),
    })
    .strict()
    .superRefine((value, ctx) => {
      const required = (field) => {
        if (value[field] === undefined) ctx.addIssue({ code: "custom", path: [field], message: `${field} is required for ${value.action}` });
      };
      if (value.action === "create") for (const field of ["name", "objective", "lead_agent_id", "workspace"]) required(field);
      if (["show", "join", "doc", "status"].includes(value.action)) required("plan_id");
      if (["join", "doc", "status"].includes(value.action)) required("agent_id");
      if (value.action === "doc") for (const field of ["path", "title"]) required(field);
      if (value.action === "status") required("status");
    })
    .describe("Create, inspect, join, or transition a collaborative plan."),

  task: z
    .object({
      action: z.enum(["create", "list", "ready", "show", "claim", "heartbeat", "submit", "release", "depend"]),
      task_id: z.string().trim().min(1).max(128).optional(),
      plan_id: z.string().trim().min(1).max(128).optional(),
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
      const required = (field) => {
        if (value[field] === undefined || value[field] === "") ctx.addIssue({ code: "custom", path: [field], message: `${field} is required for ${value.action}` });
      };
      if (value.action === "create") {
        for (const field of ["plan_id", "title", "reasoning", "agent_id"]) required(field);
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
      const required = (field) => {
        if (value[field] === undefined || value[field] === "") ctx.addIssue({ code: "custom", path: [field], message: `${field} is required for ${value.action}` });
      };
      if (["start", "touch", "end"].includes(value.action)) required("agent_id");
      if (value.action === "start") {
        if (value.target_files.length === 0) ctx.addIssue({ code: "custom", path: ["target_files"], message: "at least one target file is required for start" });
        if (!value.run_id) for (const field of ["rationale", "test_plan"]) required(field);
      }
      if (["touch", "end"].includes(value.action)) required("run_id");
      if (value.action === "show" && value.target_files.length !== 1) ctx.addIssue({ code: "custom", path: ["target_files"], message: "exactly one target file is required for show" });
    })
    .describe("Declare, heartbeat, inspect, or end advisory file work; exclusivity is opt-in."),

  wait_for_lock: z
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

  prune_stale_locks: z
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

  release_file_lock: z
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

  audit_unverified: z
    .object({
      agent_id: agentId,
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      abandon: z.boolean().default(false).describe("Mark pending runs FAILED instead of only listing."),
      older_than_days: z.number().int().min(1).max(3650).optional()
        .describe("Only include debt older than this age; inspect before --abandon."),
      origin: z.array(z.enum(["TASK", "WORK", "HOOK"])).max(3).default([])
        .describe("Restrict migration/audit to selected run origins."),
      before: z.string().datetime().optional()
        .describe("Only runs created before this ISO timestamp."),
    })
    .strict()
    .describe("List or abandon unverified runs."),

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
        for (const field of ["workspace_path", "reasoning", "remember"]) {
          if (!value[field]) ctx.addIssue({ code: "custom", path: [field], message: `${field} is required when creating.` });
        }
        if (value.state === "done") {
          ctx.addIssue({ code: "custom", path: ["state"], message: "Create open/ongoing, then close the existing refinement with a receipt." });
        }
        return;
      }
      if (!["state", "quality", "reasoning", "remember", "files"].some((field) => value[field] !== undefined)) {
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
    .describe("Reflect after work."),
};

export const examples = {
  tell_memory: {
    agent_id: "agent",
    task_context: "task",
    observation: "lesson",
    importance: 8,
    label: "GOTCHA",
    tags: ["tag"],
    references: ["file:src/file.ts"],
    workspace_path: "/repo",
    artifact: "pkg",
    repo: "repo",
    ref: "main",
    file: "src/file.ts",
    file_tree_fingerprint: "git:HEAD",
    failure_signature: "sig",
  },
  get_memory: {
    query: "task",
    limit: 3,
    min_importance: 4,
    labels: ["GOTCHA"],
    tags: ["tag"],
    references: ["file:src/file.ts"],
    workspace_path: "/repo",
    artifact: "pkg",
    repo: "repo",
    strict_scope: false,
    sort: "smart",
    smart: true,
  },
  workspace_status: {
    workspace: "/repo",
    artifact: "pkg",
  },
  query: {
    view: "workboard",
    query: "build",
    limit: 10,
    format: "json",
    workspace_path: "/repo",
    artifact: "pkg",
    label: ["GOTCHA"],
  },
  attend: {
    query: "current task",
    limit: 10,
    workspace_path: "/repo",
    artifact: "pkg",
    file: ["src/file.ts"],
    include_bodies: false,
    explain_organ: false,
  },
  repo_inject: {
    workspace_path: "/repo",
    out_dir: "/repo/.octocode",
    mode: "local",
    include_view: true,
    check: true,
  },
  export_harness: {
    limit: 10,
    min_importance: 7,
    workspace: "/repo",
    artifact: "pkg",
  },
  developer_review: {
    workspace: "/repo",
    format: "markdown",
  },
  session_capture: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    repo: "repo",
    ref: "main",
    reason: "handoff",
  },
  pre_flight_intent: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    context_ref: "docs/decision.md",
    rationale: "edit",
    target_files: ["src/file.ts", "src/file.test.ts"],
    test_plan: "test passed",
    retry_interval: 5,
    ttl_seconds: 600,
  },
  plan: {
    action: "create",
    name: "Release readiness",
    objective: "Coordinate the remaining release work.",
    lead_agent_id: "agent-lead",
    workspace: "/repo",
  },
  task: {
    action: "create",
    plan_id: "plan_abc123",
    title: "Add schema migration",
    reasoning: "Consumers need the new storage contract first.",
    acceptance: "Migration and focused tests pass.",
    path: ["src/db.ts", "tests/migration.test.ts"],
    agent_id: "agent-lead",
    priority: 10,
  },
  work: {
    action: "start",
    agent_id: "agent",
    workspace: "/repo",
    rationale: "refactor parser",
    test_plan: "run parser tests",
    target_files: ["src/parser.ts"],
    exclusive: false,
    ttl_minutes: 10,
  },
  wait_for_lock: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    target_files: ["src/file.ts"],
    wait_seconds: 120,
    retry_interval: 5,
  },
  prune_stale_locks: {
    older_than_minutes: 20,
    expired_only: false,
    workspace: "/repo",
    artifact: "pkg",
    target_files: ["src/file.ts"],
    dry_run: true,
  },
  mine_weakness: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    min_count: 2,
    limit: 20,
  },
  doc_staleness: {
    targets_json: JSON.stringify([{ docFile: "docs/a.md", sourceDirs: ["src"] }]),
    workspace: "/repo",
    artifact: "pkg",
    min_edits: 5,
    min_lines: 50,
    propose: false,
  },
  docs_catalog: {
    action: "list",
  },
  digest: {
    retention_days: 90,
    refinement_handoff_retention_days: 7,
    refinement_done_retention_days: 30,
    operational_retention_days: 90,
    dry_run: true,
    workspace: "/repo",
    artifact: "pkg",
  },
  release_file_lock: {
    agent_id: "agent",
    run_id: "run_abc123",
    workspace: "/repo",
    artifact: "pkg",
    status: "PENDING",
  },
  verify: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    run_id: ["run_abc123"],
    all_pending: false,
    status: "SUCCESS",
    message: "test passed",
  },
  audit_unverified: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    abandon: false,
  },
  forget_memory: {
    tags: ["tag"],
    workspace_path: "/repo",
    max_importance: 3,
    dry_run: true,
  },
  refinement: {
    agent_id: "agent",
    workspace_path: "/repo",
    artifact: "pkg",
    repo: "repo",
    ref: "main",
    files: ["src/file.ts"],
    reasoning: "handoff",
    remember: "lesson",
    quality: "handoff",
    state: "ongoing",
  },
  refine_query: {
    repo: "repo",
    ref: "main",
    quality: "handoff",
    states: ["open", "ongoing"],
    include_handoffs: true,
    include_env: false,
  },
  refine_delete: {
    refinement_id: ["ref_abc123"],
    dry_run: true,
  },
  agent_signal: {
    action: "publish",
    agent_id: "agent-a",
    to_agents: ["agent-b"],
    kind: "question",
    subject: "question",
    body: "body",
    files: ["src/file.ts"],
    artifact: "pkg",
    refs: ["task_123"],
    importance: 7,
  },
  signal_prune: {
    agent_id: "agent-a",
    resolved: true,
    older_than_days: 7,
    dry_run: true,
  },
  agent_registry: {
    action: "register",
    agent_id: "agent",
    agent_name: "Agent",
    workspace: "/repo",
    artifact: "pkg",
    context: "codex",
    limit: 50,
  },
  reflect: {
    agent_id: "agent",
    task: "task",
    outcome: "partial",
    worked: "worked",
    didnt_work: "blocked",
    judgment_note: "evidence",
    lesson: "lesson",
    fix_repo: "fix",
    fix_file: ["src/file.ts"],
    fix_instructions: "clarify when agents should install hooks",
    eval_failures: [
      {
        id: "eval-1",
        dimension: "planning",
        failure_signature: "sig",
        suggested_lesson: "lesson",
      },
    ],
    duo: true,
  },
};

const listableSchemas = [
  "tell_memory", "get_memory",
  "attend", "query", "repo_inject",
  "workspace_status", "export_harness", "session_capture",
  "plan", "task", "work", "pre_flight_intent", "wait_for_lock", "prune_stale_locks", "release_file_lock", "verify", "audit_unverified",
  "forget_memory", "refinement", "refine_query", "refine_delete",
  "agent_registry", "agent_signal", "signal_prune",
  "mine_weakness", "developer_review", "doc_staleness", "docs_catalog", "digest", "reflect",
];

const commandIndex = [
  { command: "attend", schema: "attend", use: "Build one bounded lobby with actions, relevant evidence/gaps, and a next command.", example: 'octocode-awareness attend --query "current task" --workspace "$PWD" --compact' },
  { command: "workspace status", schema: "workspace_status", use: "Check DB health, locks, pending verification, memory counts.", example: 'octocode-awareness workspace status --workspace "$PWD" --compact' },
  { command: "plan create", schema: "plan", use: "Create a shared plan and its managed narrative document folder.", example: 'octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact' },
  { command: "plan list", schema: "plan", use: "List plans in the current workspace scope.", example: 'octocode-awareness plan list --workspace "$PWD" --compact' },
  { command: "plan show", schema: "plan", use: "Inspect one plan, its docs, and participating agents.", example: "octocode-awareness plan show --plan-id plan_123 --compact" },
  { command: "plan join", schema: "plan", use: "Join an agent to a shared plan.", example: "octocode-awareness plan join --plan-id plan_123 --agent-id agent --compact" },
  { command: "plan doc", schema: "plan", use: "Register a supporting document inside the managed plan folder.", example: "octocode-awareness plan doc --plan-id plan_123 --agent-id agent --path docs/DESIGN.md --title Design --compact" },
  { command: "plan status", schema: "plan", use: "Let the lead transition the plan lifecycle.", example: "octocode-awareness plan status --plan-id plan_123 --agent-id lead --status ACTIVE --compact" },
  { command: "task create", schema: "task", use: "Create dependency-aware plan work with reasoning and paths.", example: 'octocode-awareness task create --plan-id plan_123 --title "Schema" --reasoning "Consumers need it first" --acceptance "schema tests pass" --path src/db.ts --agent-id lead --compact' },
  { command: "task list", schema: "task", use: "List durable tasks and current claim state.", example: "octocode-awareness task list --plan-id plan_123 --compact" },
  { command: "task ready", schema: "task", use: "List unblocked, unclaimed tasks agents may choose.", example: "octocode-awareness task ready --plan-id plan_123 --compact" },
  { command: "task show", schema: "task", use: "Inspect task reasoning, paths, dependencies, and claim.", example: "octocode-awareness task show --task-id task_123 --compact" },
  { command: "task claim", schema: "task", use: "Atomically claim one task and create its execution run.", example: "octocode-awareness task claim --task-id task_123 --agent-id agent --compact" },
  { command: "task heartbeat", schema: "task", use: "Extend an active task claim lease.", example: "octocode-awareness task heartbeat --task-id task_123 --run-id run_123 --agent-id agent --compact" },
  { command: "task submit", schema: "task", use: "Submit claimed work to the verification lane.", example: 'octocode-awareness task submit --task-id task_123 --run-id run_123 --agent-id agent --message "ready for verification" --compact' },
  { command: "task release", schema: "task", use: "Release or block claimed work without declaring success.", example: "octocode-awareness task release --task-id task_123 --run-id run_123 --agent-id agent --compact" },
  { command: "task depend", schema: "task", use: "Add dependency edges within one plan.", example: "octocode-awareness task depend --task-id task_2 --depends-on task_1 --agent-id lead --compact" },
  { command: "work start", schema: "work", use: "Declare advisory file work; add --exclusive only for sensitive changes.", example: 'octocode-awareness work start --agent-id agent --workspace "$PWD" --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact' },
  { command: "work touch", schema: "work", use: "Heartbeat active file presence without repeating its reasoning.", example: "octocode-awareness work touch --agent-id agent --run-id run_123 --compact" },
  { command: "work end", schema: "work", use: "End standalone WORK presence and move its run to verification.", example: "octocode-awareness work end --agent-id agent --run-id run_123 --compact" },
  { command: "work list", schema: "work", use: "List active file presence in the workspace.", example: 'octocode-awareness work list --workspace "$PWD" --compact' },
  { command: "work show", schema: "work", use: "Show all active agents and reasons for one file.", example: 'octocode-awareness work show --workspace "$PWD" --file src/a.ts --compact' },
  { command: "memory recall", schema: "get_memory", use: "Recall repo lessons before planning or editing.", example: 'octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact' },
  { command: "memory record", schema: "tell_memory", use: "Store durable lessons, decisions, gotchas, or observations.", example: 'octocode-awareness memory record --agent-id agent --task-context "task" --observation "lesson" --importance 7 --workspace "$PWD" --compact' },
  { command: "memory forget", schema: "forget_memory", use: "Delete selected stale memories; dry-run first.", example: "octocode-awareness memory forget --memory-id mem_123 --dry-run --compact" },
  { command: "refinement get", schema: "refine_query", use: "Read unfinished handoffs or follow-up work.", example: 'octocode-awareness refinement get --workspace "$PWD" --state open --compact' },
  { command: "refinement set", schema: "refinement", use: "Save handoff/work state for the next agent.", example: 'octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact' },
  { command: "refinement delete", schema: "refine_delete", use: "Delete stale refinement rows; dry-run first.", example: "octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact" },
  { command: "lock acquire", schema: "pre_flight_intent", use: "Acquire exclusive sensitive-file protection; exit 2 means conflict.", example: 'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "sensitive edit" --test-plan "yarn test" --compact' },
  { command: "lock wait", schema: "wait_for_lock", use: "Wait for existing file locks without claiming.", example: "octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact" },
  { command: "lock release", schema: "release_file_lock", use: "Release exclusive protection to PENDING; verify success separately with evidence.", example: "octocode-awareness lock release --agent-id agent --run-id run_123 --status PENDING --compact" },
  { command: "lock prune", schema: "prune_stale_locks", use: "Clean expired/stale locks; never marks success.", example: 'octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact' },
  { command: "verify audit", schema: "audit_unverified", use: "Find pending or stale work before finishing.", example: 'octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact' },
  { command: "verify mark", schema: "verify", use: "Mark declared verification as run.", example: 'octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact' },
  { command: "signal list", schema: "agent_signal", use: "Read inbox/messages; add --mark-read only after acting. --format hook returns host briefing shape.", example: 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact' },
  { command: "signal publish", schema: "agent_signal", use: "Send blocker/question/request/handoff/decision/fyi.", example: 'octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact' },
  { command: "signal reply", schema: "agent_signal", use: "Reply in an existing signal thread.", example: "octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject \"Re: File locked\" --body \"done\" --compact" },
  { command: "signal ack", schema: "agent_signal", use: "Mark specific signals read after handling.", example: "octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact" },
  { command: "signal resolve", schema: "agent_signal", use: "Close handled signals or threads.", example: "octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact" },
  { command: "signal prune", schema: "signal_prune", use: "Preview/delete old resolved participant-owned signals.", example: 'octocode-awareness signal prune --agent-id agent --workspace "$PWD" --resolved --older-than-days 7 --dry-run --compact' },
  { command: "agent register", schema: "agent_registry", use: "Register/touch an agent identity.", example: 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact' },
  { command: "agent list", schema: "agent_registry", use: "List known agents in scope.", example: 'octocode-awareness agent list --workspace "$PWD" --compact' },
  { command: "query", schema: "query", use: "Read DB views as json/table/csv/markdown/html, including file-reference health.", example: 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact' },
  { command: "query files", schema: "query", use: "Filter/sort tracked paths and stale file references; rows include file_exists and missing_file.", example: 'octocode-awareness query files --workspace "$PWD" --format table --limit 50' },
  { command: "query workboard", schema: "query", use: "Read the smart agent queue, including stale_file_refs memory-review items.", example: 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact' },
  { command: "query all", schema: "query", use: "Export all live views; use html for the sortable/filterable browser view.", example: 'octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html' },
  { command: "query developer-review", schema: "query", use: "Read instruction-feedback rows that also feed DEVELOPER_REVIEW.md.", example: 'octocode-awareness query developer-review --workspace "$PWD" --format markdown --compact' },
  { command: "repo inject", schema: "repo_inject", use: "Generate .octocode Markdown, CSV, HTML, manifest, and missing-ref warnings without editing .gitignore.", example: 'octocode-awareness repo inject --workspace "$PWD" --mode local --compact' },
  { command: "session capture", schema: "session_capture", use: "Hook-driven handoff capture from locks + dirty git tree.", example: 'octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact' },
  { command: "reflect record", schema: "reflect", use: "Record outcome and lessons after work.", example: 'octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "lesson" --compact' },
  { command: "reflect mine-weakness", schema: "mine_weakness", use: "Find recurring failure clusters.", example: 'octocode-awareness reflect mine-weakness --workspace "$PWD" --compact' },
  { command: "reflect export-harness", schema: "export_harness", use: "Preview harness guidance candidates from memories.", example: 'octocode-awareness reflect export-harness --workspace "$PWD" --compact' },
  { command: "reflect developer-review", schema: "developer_review", use: "Read agent feedback on the instructions themselves (from reflect record --fix-instructions).", example: 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact' },
  { command: "docs list", schema: "docs_catalog", use: "List skill reference docs (references/*.md).", example: "octocode-awareness docs list --compact" },
  { command: "docs show", schema: "docs_catalog", use: "Show one skill reference by name.", example: "octocode-awareness docs show architecture" },
  { command: "docs staleness", schema: "doc_staleness", use: "Find docs likely stale from edit activity.", example: 'octocode-awareness docs staleness --targets-json \'[{"docFile":"README.md","sourceDirs":["src"]}]\' --compact' },
  { command: "maintenance digest", schema: "digest", use: "Preview or run memory/signal/refinement cleanup.", example: 'octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact' },
  { command: "maintenance init", schema: null, use: "Initialize the awareness DB.", example: "octocode-awareness maintenance init --compact" },
  { command: "maintenance self-test", schema: null, use: "Run in-memory DB smoke checks.", example: "octocode-awareness maintenance self-test --compact" },
  { command: "hooks install", schema: null, use: "Install hook config after preview/approval.", example: "octocode-awareness hooks install --host codex --dry-run --compact" },
  { command: "hooks check", schema: null, use: "Check installed hook config and detect drift.", example: "octocode-awareness hooks check --host codex --strict --compact" },
  { command: "hooks remove", schema: null, use: "Remove awareness-owned hook config.", example: "octocode-awareness hooks remove --host codex --dry-run --compact" },
  { command: "hook run", schema: null, use: "Internal hook dispatcher used by wrappers.", example: "octocode-awareness hook run pre-edit < hook-payload.json" },
  { command: "schema commands", schema: null, use: "Print this command-to-schema map.", example: "octocode-awareness schema commands --compact" },
  { command: "schema list", schema: null, use: "Print schema names only.", example: "octocode-awareness schema list --compact" },
  { command: "schema json-schema", schema: null, use: "Print one JSON schema.", example: "octocode-awareness schema json-schema get_memory --compact" },
  { command: "schema example", schema: null, use: "Print example JSON for one schema.", example: "octocode-awareness schema example get_memory --compact" },
  { command: "schema validate", schema: null, use: "Validate JSON payload against one schema.", example: "octocode-awareness schema validate get_memory payload.json --compact" },
];

function printJson(payload, compact = false) {
  console.log(JSON.stringify(payload, null, compact ? 0 : 2));
}

function usage() {
  return `Usage:
  node scripts/schema.mjs commands [--compact] [--examples]
  node scripts/schema.mjs list
  node scripts/schema.mjs json-schema <schema-name>
  node scripts/schema.mjs example <schema-name>
  node scripts/schema.mjs validate <schema-name> <json-file|->`;
}

function toJsonSchema(schema) {
  if (typeof z.toJSONSchema === "function") {
    return z.toJSONSchema(schema);
  }
  throw new Error("This script requires Zod v4 with z.toJSONSchema().");
}

function parseJson(input) {
  try {
    return JSON.parse(input);
  } catch (error) {
    throw new Error(`Invalid JSON: ${error.message}`);
  }
}

function formatZodError(error) {
  return error.issues.map((issue) => ({
    path: issue.path.join(".") || "<root>",
    code: issue.code,
    message: issue.message,
  }));
}

function printJsonError(payload, code = 2, compact = false) {
  console.log(JSON.stringify({ ok: false, ...payload }, null, compact ? 0 : 2));
  return code;
}

async function main(argv) {
  const compact = argv.includes("--compact") || process.env.OCTOCODE_AWARENESS_COMPACT === "1";
  const includeExamples = argv.includes("--examples");
  const filteredArgv = argv.filter((arg) => arg !== "--compact" && arg !== "--examples");
  const [command, schemaName, file] = filteredArgv;

  if (!command || command === "--help" || command === "-h") {
    console.log(usage());
    return 0;
  }

  if (command === "commands") {
    const commands = includeExamples
      ? commandIndex
      : commandIndex.map(({ command: cmd, schema }) => ({ command: cmd, schema }));
    printJson({
      ok: true,
      hint: includeExamples
        ? "Use `octocode-awareness <command> --help` for flags. Add --compact to JSON commands."
        : "Lean command/schema index. Pass --examples for purpose and recipes; use `<command> --help` for flags.",
      commands,
    }, compact);
    return 0;
  }

  if (command === "list") {
    printJson(listableSchemas, compact);
    return 0;
  }

  const schema = listableSchemas.includes(schemaName) ? schemas[schemaName] : undefined;
  if (!schema) {
    return printJsonError({
      error_code: "UNKNOWN_SCHEMA",
      error: `Unknown schema: ${schemaName || "<missing>"}`,
      hint: "Use one of the schemas returned by `schema list`.",
      known_schemas: listableSchemas,
    }, 2, compact);
  }

  if (command === "json-schema") {
    printJson(toJsonSchema(schema), compact);
    return 0;
  }

  if (command === "example") {
    printJson(examples[schemaName], compact);
    return 0;
  }

  if (command === "validate") {
    if (!file) {
      return printJsonError({
        error_code: "MISSING_INPUT",
        error: "Missing <json-file|->.",
        hint: "Use `schema validate <schema-name> <json-file|->`.",
      }, 2, compact);
    }
    const raw = file === "-" ? await readStdin() : await readFile(file, "utf8");
    let parsed;
    try {
      parsed = parseJson(raw);
    } catch (error) {
      return printJsonError({
        error_code: "INVALID_JSON",
        schema: schemaName,
        error: error.message,
        hint: "Pass valid JSON matching the selected schema.",
      }, 2, compact);
    }
    const result = schema.safeParse(parsed);
    if (!result.success) {
      return printJsonError({
        schema: schemaName,
        issues: formatZodError(result.error),
      }, 1, compact);
    }
    printJson({ ok: true, schema: schemaName, data: result.data }, compact);
    return 0;
  }

  return printJsonError({
    error_code: "UNKNOWN_COMMAND",
    error: `Unknown command: ${command}`,
    hint: usage(),
  }, 2, compact);
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

main(process.argv.slice(2)).then(
  (code) => {
    process.exitCode = code;
  },
  (error) => {
    console.log(JSON.stringify({
      ok: false,
      error_code: "SCHEMA_RUNTIME_ERROR",
      error: error.message,
    }, null, 2));
    process.exitCode = 1;
  },
);
