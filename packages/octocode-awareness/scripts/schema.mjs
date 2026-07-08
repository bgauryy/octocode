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
    "tasks",
    "locks",
    "agents",
    "signals",
    "refinements",
    "files",
    "activity",
    "workboard",
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
    .describe("Build one compact read-only start packet with profile, workboard, evidence, gaps, organ_state, and drive_state."),

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
      plan_doc_ref: z.string().trim().min(1).max(1024).optional(),
      rationale: nonEmptyText("Why edit.", 2000),
      target_files: targetFiles,
      test_plan: nonEmptyText("Verification plan.", 2000),
      lock_type: z.enum(["SHARED", "EXCLUSIVE"]).default("EXCLUSIVE"),
      wait_seconds: z.number().int().min(0).max(3600).default(0),
      retry_interval: z.number().int().min(1).max(300).default(5),
      ttl_minutes: z.number().int().min(1).max(10).default(10)
        .describe("Lock TTL minutes."),
      ttl_seconds: z.number().int().min(1).max(600).optional()
        .describe("Lock TTL seconds; overrides ttl_minutes when provided."),
    })
    .strict()
    .describe("Claim file locks."),

  wait_for_lock: z
    .object({
      agent_id: agentId.describe("Waiting agent."),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      target_files: targetFiles,
      lock_type: z.enum(["SHARED", "EXCLUSIVE"]).default("EXCLUSIVE"),
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
    })
    .strict()
    .describe("List or show skill reference docs."),

  digest: z
    .object({
      retention_days: z.number().int().min(1).max(3650).default(90),
      refinement_handoff_retention_days: z.number().int().min(1).max(3650).default(7),
      refinement_done_retention_days: z.number().int().min(1).max(3650).default(30),
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
      task_id: z.string().trim().min(1).max(128).optional(),
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      target_files: z.array(z.string().trim().min(1).max(1024)).max(200).optional(),
      status: z.enum(["PENDING", "SUCCESS", "FAILED"]).default("SUCCESS"),
      verified: z
        .boolean()
        .default(false)
        .describe("Tests ran."),
      verified_note: z.string().trim().min(1).max(2000).optional().describe("Verification note."),
    })
    .strict()
    .describe("Release locks."),

  verify: z
    .object({
      agent_id: agentId,
      workspace: z.string().trim().min(1).max(1024).optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      task_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Task ids."),
      all_pending: z.boolean().default(false).describe("All pending tasks."),
      status: z.enum(["SUCCESS", "FAILED"]).default("SUCCESS").describe("Verification status."),
      message: z.string().trim().min(1).max(2000).optional().describe("Verification note."),
    })
    .strict()
    .describe("Mark verified."),

  audit_unverified: z
    .object({
      agent_id: agentId,
      workspace: workspacePath.optional(),
      artifact: artifactScope.optional(),
      abandon: z.boolean().default(false).describe("Mark pending tasks FAILED instead of only listing."),
    })
    .strict()
    .describe("List or abandon unverified tasks."),

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
      workspace_path: z
        .string()
        .trim()
        .min(1)
        .max(1024)
        .describe("Workspace root."),
      artifact: artifactScope.optional(),
      repo: z.string().trim().min(1).max(256).optional().describe("Repo."),
      ref: z.string().trim().min(1).max(256).optional().describe("Ref."),
      files: z
        .array(z.string().trim().min(1).max(1024))
        .max(200)
        .default([])
        .describe("Related files."),
      reasoning: nonEmptyText("Why saved.", 2000),
      remember: nonEmptyText("What to remember.", 2000),
      quality: z.enum(["good", "bad", "handoff"]).default("good").describe("Outcome quality."),
      state: z
        .enum(["open", "ongoing", "done"])
        .default("open")
        .describe("Lifecycle state."),
    })
    .strict()
    .describe("Store refinement."),

  refine_query: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      refinement_id: z.string().trim().min(1).max(128).optional(),
      artifact: artifactScope.optional(),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      quality: z.enum(["good", "bad", "handoff"]).optional(),
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
    })
    .strict()
    .describe("Signal actions."),

  signal_prune: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Workspace channel."),
      artifact: artifactScope.optional(),
      signal_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Signal ids."),
      resolved: z.boolean().default(false).describe("Resolved only."),
      older_than_days: z
        .number()
        .int()
        .min(1)
        .max(3650)
        .optional()
        .describe("Age cutoff."),
      dry_run: z.boolean().default(false).describe("Preview only."),
    })
    .strict()
    .refine((d) => d.signal_id.length > 0 || d.resolved || d.older_than_days !== undefined, {
      message: "signal_prune requires a selector: signal_id, resolved, or older_than_days.",
    })
    .describe("Prune signals."),

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
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      artifact: artifactScope.optional(),
      importance: importanceLevel.optional().describe("Importance override."),
      duo: z
        .boolean()
        .optional()
        .describe("Advisory duo packet."),
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
    plan_doc_ref: "docs/plan.md",
    rationale: "edit",
    target_files: ["src/file.ts", "src/file.test.ts"],
    test_plan: "test passed",
    lock_type: "EXCLUSIVE",
    retry_interval: 5,
    ttl_seconds: 600,
  },
  wait_for_lock: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    target_files: ["src/file.ts"],
    lock_type: "EXCLUSIVE",
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
    dry_run: true,
    workspace: "/repo",
    artifact: "pkg",
  },
  release_file_lock: {
    agent_id: "agent",
    task_id: "task_abc123",
    workspace: "/repo",
    artifact: "pkg",
    status: "SUCCESS",
    verified: true,
    verified_note: "test passed",
  },
  verify: {
    agent_id: "agent",
    workspace: "/repo",
    artifact: "pkg",
    task_id: ["task_abc123"],
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
  "pre_flight_intent", "wait_for_lock", "prune_stale_locks", "release_file_lock", "verify", "audit_unverified",
  "forget_memory", "refinement", "refine_query", "refine_delete",
  "agent_registry", "agent_signal", "signal_prune",
  "mine_weakness", "doc_staleness", "docs_catalog", "digest", "reflect",
];

const commandIndex = [
  { command: "attend", schema: "attend", use: "Build one compact start packet with profile, workboard, evidence, gaps, organ_state, and drive_state.", example: 'octocode-awareness attend --query "current task" --workspace "$PWD" --compact' },
  { command: "workspace status", schema: "workspace_status", use: "Check DB health, locks, pending verification, memory counts.", example: 'octocode-awareness workspace status --workspace "$PWD" --compact' },
  { command: "memory recall", schema: "get_memory", use: "Recall repo lessons before planning or editing.", example: 'octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact' },
  { command: "memory record", schema: "tell_memory", use: "Store durable lessons, decisions, gotchas, or observations.", example: 'octocode-awareness memory record --agent-id agent --task-context "task" --observation "lesson" --importance 7 --workspace "$PWD" --compact' },
  { command: "memory forget", schema: "forget_memory", use: "Delete selected stale memories; dry-run first.", example: "octocode-awareness memory forget --memory-id mem_123 --dry-run --compact" },
  { command: "refinement get", schema: "refine_query", use: "Read unfinished handoffs or follow-up work.", example: 'octocode-awareness refinement get --workspace "$PWD" --state open --compact' },
  { command: "refinement set", schema: "refinement", use: "Save handoff/work state for the next agent.", example: 'octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact' },
  { command: "refinement delete", schema: "refine_delete", use: "Delete stale refinement rows; dry-run first.", example: "octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact" },
  { command: "lock acquire", schema: "pre_flight_intent", use: "Claim files before edits; exit 2 means conflict.", example: 'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --test-plan "yarn test" --compact' },
  { command: "lock wait", schema: "wait_for_lock", use: "Wait for existing file locks without claiming.", example: "octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact" },
  { command: "lock release", schema: "release_file_lock", use: "Release file claims as SUCCESS, FAILED, or PENDING.", example: "octocode-awareness lock release --agent-id agent --task-id task_123 --status SUCCESS --verified --compact" },
  { command: "lock prune", schema: "prune_stale_locks", use: "Clean expired/stale locks; never marks success.", example: 'octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact' },
  { command: "verify audit", schema: "audit_unverified", use: "Find pending or stale work before finishing.", example: 'octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact' },
  { command: "verify mark", schema: "verify", use: "Mark declared verification as run.", example: 'octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact' },
  { command: "signal list", schema: "agent_signal", use: "Read inbox/messages; add --mark-read only after acting.", example: 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact' },
  { command: "signal publish", schema: "agent_signal", use: "Send blocker/question/request/handoff/decision/fyi.", example: 'octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact' },
  { command: "signal reply", schema: "agent_signal", use: "Reply in an existing signal thread.", example: "octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject \"Re: File locked\" --body \"done\" --compact" },
  { command: "signal ack", schema: "agent_signal", use: "Mark specific signals read after handling.", example: "octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact" },
  { command: "signal resolve", schema: "agent_signal", use: "Close handled signals or threads.", example: "octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact" },
  { command: "signal prune", schema: "signal_prune", use: "Delete resolved/old/selected signals; dry-run first.", example: 'octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact' },
  { command: "agent register", schema: "agent_registry", use: "Register/touch an agent identity.", example: 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact' },
  { command: "agent list", schema: "agent_registry", use: "List known agents in scope.", example: 'octocode-awareness agent list --workspace "$PWD" --compact' },
  { command: "query", schema: "query", use: "Read DB views as json/table/csv/markdown/html, including the derived workboard.", example: 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact' },
  { command: "repo inject", schema: "repo_inject", use: "Generate .octocode repo context without editing .gitignore.", example: 'octocode-awareness repo inject --workspace "$PWD" --mode local --compact' },
  { command: "session capture", schema: "session_capture", use: "Hook-driven handoff capture from locks + dirty git tree.", example: 'octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact' },
  { command: "reflect record", schema: "reflect", use: "Record outcome and lessons after work.", example: 'octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "lesson" --compact' },
  { command: "reflect mine-weakness", schema: "mine_weakness", use: "Find recurring failure clusters.", example: 'octocode-awareness reflect mine-weakness --workspace "$PWD" --compact' },
  { command: "reflect export-harness", schema: "export_harness", use: "Preview harness guidance candidates from memories.", example: 'octocode-awareness reflect export-harness --workspace "$PWD" --compact' },
  { command: "docs list", schema: "docs_catalog", use: "List skill reference docs (references/*.md).", example: "octocode-awareness docs list --compact" },
  { command: "docs show", schema: "docs_catalog", use: "Show one skill reference by name.", example: "octocode-awareness docs show full-flow" },
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
  node scripts/schema.mjs commands [--compact]
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
  const filteredArgv = argv.filter((arg) => arg !== "--compact");
  const [command, schemaName, file] = filteredArgv;

  if (!command || command === "--help" || command === "-h") {
    console.log(usage());
    return 0;
  }

  if (command === "commands") {
    printJson({ ok: true, hint: "Use `octocode-awareness <command> --help` for flags. Add --compact to JSON commands.", commands: commandIndex }, compact);
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
