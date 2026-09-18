/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { z } from 'zod';
import { PLAN_STATUSES } from '../entities.js';
import { AttendRevisionInputSchema } from './attend-revision.js';
import {
  agentId, nonEmptyText, tags, workspacePath, artifactScope, repoScope,
  refScope, references, memoryLabel, memorySort, importanceLevel, targetFiles,
  awarenessQueryView, awarenessOutputFormat,
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
      references: references.describe('Provenance links; fingerprint capture accepts only file:<local-path>. Git pointers: memory store-verified history_ref.'),
      workspace_path: workspacePath
        .optional()
        .describe("Memory scope."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo scope."),
      ref: refScope.optional().describe("Ref scope."),
      file: z
        .union([z.string().trim().min(1).max(1024), targetFiles])
        .optional()
        .describe("Declared files; repeat --file to capture all evidence dependencies."),
      file_tree_fingerprint: z
        .string()
        .trim()
        .min(1)
        .max(256)
        .optional()
        .describe("Opaque provenance; use capture_fingerprint for a runtime-generated declared-file fingerprint. Never verification proof."),
      capture_fingerprint: z.boolean().default(false)
        .describe("Capture local files only, not git:/signal:/artifact:/URL references. Rejects partial capture and file_tree_fingerprint."),
      supersedes: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("ACTIVE memory record IDs with the same agent and scope; excludes shared store-verified records. Omit for independent evidence."),
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
    .describe("Save verified reusable learning with scope and evidence. Routine status belongs in the current conversation, not durable memory."),
  memory_recall: z
    .object({
      check_fingerprint: z.boolean().default(false)
        .describe("Recheck declared bytes (fresh/stale/unknown), not claims or dependency coverage."),
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
      all_workspaces: z.boolean().default(false).describe("Search every workspace."),
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
        .describe("Full memory rows; default is lean."),
      as_of: z
        .string()
        .trim()
        .min(1)
        .max(64)
        .optional()
        .describe("Point-in-time ISO."),
    })
    .strict()
    .describe("Recall scoped lessons when prior learning could change the approach. Ranked hits are leads; revalidate their evidence before reuse."),
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
      details: z.boolean().default(false).describe("Request the coordination inspection view. This is also selected by query, file, artifact, repo, ref, include_bodies, explain_organ, or revision."),
      changes: z.boolean().default(false).describe("Read paged Git path/status and declared work across linked checkouts. This view cannot combine with details, query, file, artifact, repo, ref, or explain_organ; include_bodies and revision remain valid."),
      offset: z.number().int().min(0).default(0).describe("Continuation offset for presence or changes. Reuse returned revision for change pages."),
      revision: AttendRevisionInputSchema.optional(),
      agent_id: agentId.optional().describe("Stable agent identity used to prioritize owned work."),
      query: z.string().trim().max(1000).default("").describe("Current task, risk, or design question."),
      limit: z.number().int().min(1).max(50).default(10).describe("Peers or Git/work rows per page; detailed view rows per column and evidence cap."),
      workspace_path: workspacePath.optional().describe("Workspace filter."),
      artifact: artifactScope.optional(),
      repo: repoScope.optional().describe("Repo filter."),
      ref: refScope.optional().describe("Ref filter."),
      file: z.array(z.string().trim().min(1).max(1024)).max(50).default([]).describe("File filters."),
      include_bodies: z.boolean().default(false).describe("Include full signal bodies in routed reads, or full work intent with changes."),
      explain_organ: z.boolean().default(false).describe("Request detailed diagnostics with the organ reference."),
    })
    .strict()
    .describe("Meet registered workspace peers. Default reads presence only; details, query, file, artifact, repo, ref, include_bodies, explain_organ, or revision request coordination inspection. changes selects the separate Git view. Reuse the initial briefing until shared state changes."),
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
      status: z.enum(PLAN_STATUSES).optional(),
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
