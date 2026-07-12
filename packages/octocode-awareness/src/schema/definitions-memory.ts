/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { z } from 'zod';
import {
  agentId, nonEmptyText, tags, workspacePath, artifactScope, repoScope,
  refScope, references, memoryLabel, memorySort, importanceLevel, targetFiles,
  awarenessQueryView, awarenessOutputFormat, repoContextMode,
} from './common.js';

export const memorySchemas = {
memory_record: z
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
        .describe("Opaque source fingerprint for provenance; Awareness does not validate it or use it as freshness proof."),
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
  memory_recall: z
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
      agent_id: agentId.optional().describe("Stable agent identity used to prioritize owned work."),
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
  wiki_sync: z
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
      prune_orphans: z.boolean().default(false)
        .describe("Remove retired files owned by the previous Awareness manifest; run once without it to preview candidates."),
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
  lock_acquire: z
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
      workspace: workspacePath.optional()
        .describe("Plan scope normalizes to the repo root; on create, the exact path passed also decides where .octocode/plan scaffolding is written."),
      artifact: artifactScope.optional(),
      status: z.enum(["DRAFT", "ACTIVE", "PAUSED", "COMPLETED", "CANCELLED"]).optional(),
      path: z.string().trim().min(1).max(1024).optional(),
      title: nonEmptyText("Supporting document title.", 300).optional(),
    })
    .strict()
    .superRefine((value, ctx) => {
      const required = (field: keyof typeof value) => {
        if (value[field] === undefined) ctx.addIssue({ code: "custom", path: [field], message: `${field} is required for ${value.action}` });
      };
      if (value.action === "create") for (const field of ["name", "objective", "lead_agent_id", "workspace"] as const) required(field);
      if (["show", "join", "doc", "status"].includes(value.action)) required("plan_id");
      if (["join", "doc", "status"].includes(value.action)) required("agent_id");
      if (value.action === "doc") for (const field of ["path", "title"] as const) required(field);
      if (value.action === "status") required("status");
    })
    .describe("Create, inspect, join, or transition a collaborative plan.")
};
