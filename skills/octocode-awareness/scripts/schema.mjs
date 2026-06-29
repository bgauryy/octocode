#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { z } from "zod";

const agentId = z.string().min(1).max(128).describe("Stable human-readable agent identifier.");
const nonEmptyText = (description, max = 4000) => z.string().trim().min(1).max(max).describe(description);
const tag = z
  .string()
  .trim()
  .min(1)
  .max(64)
  .regex(/^[a-zA-Z0-9_.:-]+$/, "tags may contain letters, numbers, underscore, dot, colon, or dash");
const tags = z.array(tag).max(32).default([]).describe("Fast filtering keywords.");
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
  "OTHER",
];
const normalizeMemoryLabel = (value) => {
  if (typeof value !== "string") return value;
  const cleaned = value.trim().toUpperCase().replace(/[\s-]+/g, "_");
  return cleaned || "OTHER";
};
const memoryLabel = z
  .preprocess(normalizeMemoryLabel, z.enum(MEMORY_LABELS).default("OTHER"))
  .describe("Memory category label. Empty or omitted becomes OTHER.");
const memorySort = z
  .enum(["smart", "score", "importance", "recent", "updated", "accessed", "access", "label", "file"])
  .default("smart")
  .describe("Result order. smart/score use salience; alternatives sort by explicit fields.");
const importanceScore = z
  .number()
  .int()
  .min(1)
  .max(10)
  .describe("1 = minor detail, 10 = critical behavior or risk.");
const targetFiles = z
  .array(z.string().trim().min(1).max(1024))
  .min(1)
  .max(200)
  .describe("Absolute or workspace-relative files likely to be modified or affected.");

// Notifications — repo-scoped agent-to-agent messages. The `kind` enum is the
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
  .describe("Files this message concerns (normalized like locks). May be empty.");
const refIds = z
  .array(z.string().trim().min(1).max(128))
  .max(50)
  .default([])
  .describe("Ids this message is about: intent_id / refinement_id / memory_id / notification_id — makes it actionable.");
const evalFailure = z
  .object({
    id: z.string().trim().min(1).max(128).describe("Failed eval question/check id."),
    dimension: z.string().trim().min(1).max(128).optional().describe("Rubric or reasoning dimension."),
    failure_signature: z
      .string()
      .trim()
      .min(1)
      .max(256)
      .optional()
      .describe("Clusterable recurring-failure signature for mine-weakness."),
    suggested_lesson: z.string().trim().min(1).max(1000).optional().describe("Reusable lesson suggested by the eval."),
  })
  .strict()
  .refine((d) => d.failure_signature !== undefined || d.suggested_lesson !== undefined, {
    message: "eval failure needs failure_signature or suggested_lesson.",
  });

export const schemas = {
  tell_memory: z
    .object({
      agent_id: agentId,
      task_context: nonEmptyText("What goal or script produced the lesson.", 1000),
      observation: nonEmptyText("Exact lesson learned; specific enough to act on later.", 4000),
      importance_score: importanceScore,
      label: memoryLabel,
      tags,
      file: z
        .string()
        .trim()
        .min(1)
        .max(1024)
        .optional()
        .describe("The ONE file this memory correlates to (normalized to absolute). Omit for a general lesson."),
      file_tree_fingerprint: z
        .string()
        .trim()
        .min(1)
        .max(256)
        .optional()
        .describe("Optional Git SHA or workspace state hash."),
      supersedes: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Memory ids this new memory replaces; each is marked SUPERSEDED and points here."),
      failure_signature: z
        .string()
        .trim()
        .min(1)
        .max(256)
        .optional()
        .describe("Recurring-failure key for weakness mining, e.g. 'mechanism:retry-loop|cause:test-timeout'."),
      valid_from: z.string().trim().min(1).max(64).optional().describe("ISO time the fact becomes true (default now)."),
      valid_to: z.string().trim().min(1).max(64).optional().describe("ISO time the fact stops being true (open-ended if omitted)."),
    })
    .strict()
    .describe("Save a new insight, lesson learned, or architectural choice."),

  get_memory: z
    .object({
      query: z.string().trim().max(1000).default("").describe("Natural-language semantic or lexical recall query; may be empty when using filters."),
      limit: z.number().int().min(1).max(50).default(3),
      min_importance: z.number().int().min(1).max(10).default(1),
      labels: z.array(memoryLabel).max(32).default([]).describe("Filter by memory category label."),
      tags,
      files: z
        .array(z.string().trim().min(1).max(1024))
        .max(200)
        .default([])
        .describe("Exact stored memory file paths to match; CLI normalizes --file to absolute paths."),
      file_regex: z
        .array(z.string().trim().min(1).max(512))
        .max(20)
        .default([])
        .describe("Regex patterns matched against stored memory file paths."),
      regex: z
        .array(z.string().trim().min(1).max(512))
        .max(20)
        .default([])
        .describe("Regex patterns matched against task, observation, tags, label, file, and failure signature."),
      sort: memorySort,
      smart: z
        .boolean()
        .default(false)
        .describe("If strict recall under-fills, broaden safely: lower importance, drop label/tag filters, and try semantic if indexed."),
      states: z
        .array(z.enum(["ACTIVE", "SUPERSEDED"]))
        .default(["ACTIVE"])
        .describe("Lifecycle states to recall. Default: ACTIVE only."),
      no_decay: z
        .boolean()
        .default(false)
        .describe("Rank by importance+lexical only, skipping recency/access salience decay."),
      half_life: z
        .number()
        .positive()
        .optional()
        .describe("Decay half-life in days (default 30, measured from last use)."),
      explain: z
        .boolean()
        .default(false)
        .describe("Include per-result score_components (importance/recency/access/lexical)."),
      semantic: z
        .boolean()
        .default(false)
        .describe("Use local embedding recall (model2vec); falls back to lexical if unavailable."),
      as_of: z
        .string()
        .trim()
        .min(1)
        .max(64)
        .optional()
        .describe("Bi-temporal point-in-time recall: only memories valid at this ISO time."),
    })
    .strict()
    .describe("Query shared memory; default ranking blends importance, recency-of-use, access, and lexical match."),

  pre_flight_intent: z
    .object({
      agent_id: agentId,
      workspace: z.string().trim().min(1).max(1024).optional().describe("Workspace root for verification scoping."),
      plan_doc_ref: z.string().trim().min(1).max(1024).optional(),
      rationale: nonEmptyText("Detailed reason why this change is necessary.", 2000),
      target_files: targetFiles,
      test_plan: nonEmptyText("How the agent intends to verify the change.", 2000),
      lock_type: z.enum(["SHARED", "EXCLUSIVE"]).default("EXCLUSIVE"),
      wait_seconds: z.number().int().min(0).max(3600).default(0),
      retry_interval: z.number().int().min(1).max(300).default(5),
      ttl_minutes: z.number().int().min(1).max(10080).default(240),
    })
    .strict()
    .describe("Register edit intent and acquire target file locks."),

  wait_for_lock: z
    .object({
      agent_id: agentId.describe("Waiting agent id; used so an agent does not wait on its own locks."),
      target_files: targetFiles,
      lock_type: z.enum(["SHARED", "EXCLUSIVE"]).default("EXCLUSIVE"),
      wait_seconds: z.number().int().min(0).max(3600).default(60),
      retry_interval: z.number().int().min(1).max(300).default(5),
    })
    .strict()
    .describe("Poll until target file locks clear without acquiring a lock. Exit 2 on timeout with conflicts."),

  prune_stale_locks: z
    .object({
      older_than_minutes: z.number().int().min(1).max(10080).default(20),
      expired_only: z.boolean().default(false),
      agent_id: agentId.optional().describe("Optional holder filter."),
      target_files: z.array(z.string().trim().min(1).max(1024)).max(200).default([]),
      dry_run: z.boolean().default(false),
    })
    .strict()
    .describe("Delete expired or age-stale locks. Affected ACTIVE intents become PENDING, never SUCCESS."),

  release_file_lock: z
    .object({
      agent_id: agentId,
      intent_id: z.string().trim().min(1).max(128).optional(),
      target_files: z.array(z.string().trim().min(1).max(1024)).max(200).optional(),
      status: z.enum(["PENDING", "SUCCESS", "FAILED"]).default("SUCCESS"),
      verified: z
        .boolean()
        .default(false)
        .describe("Record that the intent's test_plan was actually run before releasing."),
      verified_note: z.string().trim().min(1).max(2000).optional().describe("What was verified."),
    })
    .strict()
    .describe("Release locks. PENDING releases the file but keeps verification owed; unverified SUCCESS is downgraded to PENDING and warns."),

  verify: z
    .object({
      agent_id: agentId,
      workspace: z.string().trim().min(1).max(1024).optional().describe("Restrict --all-pending to this workspace."),
      intent_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Intent ids whose work was checked; empty when all_pending is true."),
      all_pending: z.boolean().default(false).describe("Verify every unverified pending/live intent for this agent."),
      message: z.string().trim().min(1).max(2000).optional().describe("What was verified (test output, artifact)."),
    })
    .strict()
    .describe("Record that an intent's work was actually checked (validate-before-conclude)."),

  forget_memory: z
    .object({
      memory_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Exact memory ids to delete."),
      tags,
      before: z
        .string()
        .trim()
        .min(1)
        .max(64)
        .optional()
        .describe("Delete memories created before this ISO timestamp."),
      max_importance: z
        .number()
        .int()
        .min(1)
        .max(10)
        .optional()
        .describe("Only delete memories at or below this importance (safety ceiling)."),
      dry_run: z.boolean().default(false).describe("Report matches without deleting."),
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
    .describe("Delete memories by id, tag, age, or importance ceiling. Filters combine with AND."),

  refinement: z
    .object({
      agent_id: agentId,
      workspace_path: z
        .string()
        .trim()
        .min(1)
        .max(1024)
        .describe("Local workspace root the refinement belongs to."),
      repo: z.string().trim().min(1).max(256).optional().describe("Repository name."),
      ref: z.string().trim().min(1).max(256).optional().describe("Branch name or commit hash."),
      files: z
        .array(z.string().trim().min(1).max(1024))
        .max(200)
        .default([])
        .describe("Related file paths; may be empty when not file-specific."),
      reasoning: nonEmptyText("Why this is saved for the next agent.", 2000),
      remember: nonEmptyText("What to remember — the good or bad lesson.", 2000),
      quality: z.enum(["good", "bad"]).default("good").describe("Was this a good or bad outcome."),
      state: z
        .enum(["open", "ongoing", "done"])
        .default("open")
        .describe("Work lifecycle: open (identified), ongoing (in progress), done (finished)."),
    })
    .strict()
    .describe("A workspace work-handoff record for the next agent. Stored per workspace."),

  refine_query: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      refinement_id: z.string().trim().min(1).max(128).optional(),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      quality: z.enum(["good", "bad"]).optional(),
      states: z
        .array(z.enum(["open", "ongoing", "done"]))
        .default(["open", "ongoing"])
        .describe("States to read. Default: open + ongoing (the unfinished-work handoff view)."),
      limit: z.number().int().min(1).max(200).default(20),
    })
    .strict()
    .describe("Read workspace refinements, defaulting to the unfinished-work handoff view."),

  refine_delete: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      refinement_id: z
        .array(z.string().trim().min(1).max(128))
        .min(1)
        .max(200)
        .describe("Refinement ids to delete."),
      dry_run: z.boolean().default(false).describe("Report matches without deleting."),
    })
    .strict()
    .describe("Hard-delete workspace refinements by id (counterpart to refine-set)."),

  notify: z
    .object({
      agent_id: agentId.describe("Sender (the from_agent)."),
      workspace_path: z
        .string()
        .trim()
        .min(1)
        .max(1024)
        .optional()
        .describe("The repo channel this message belongs to (workspace root). Default: cwd."),
      repo: z.string().trim().min(1).max(256).optional().describe("Repository name (auto-filled from git if omitted)."),
      ref: z.string().trim().min(1).max(256).optional().describe("Branch or commit (auto-filled from git if omitted)."),
      to: agentId
        .optional()
        .describe("Recipient agent_id. Omit to broadcast to every other agent on this repo."),
      kind: notificationKind.describe(
        "Typed message — recipients can filter by kind (e.g. only blockers) and act on it.",
      ),
      subject: nonEmptyText("One-line summary of the message.", 200),
      body: nonEmptyText("Optional detail; specific enough for the recipient to act.", 4000).optional(),
      files: fileList,
      refs: refIds,
      in_reply_to: z
        .string()
        .trim()
        .min(1)
        .max(128)
        .optional()
        .describe("notification_id this replies to; inherits its thread so agents can discuss."),
      importance: importanceScore.default(5),
    })
    .strict()
    .describe("Post a message to other agents working this repo, or reply in a thread."),

  notify_query: z
    .object({
      agent_id: agentId.describe("Reader — delivery targets this agent."),
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Repo channel to read. Default: cwd."),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      unread_only: z
        .boolean()
        .default(true)
        .describe("Only messages addressed to me or broadcast that I have not read yet."),
      kinds: z.array(notificationKind).max(8).default([]).describe("Filter to these kinds (empty = all)."),
      thread_id: z.string().trim().min(1).max(128).optional().describe("Read one discussion thread end-to-end."),
      mark_read: z
        .boolean()
        .default(false)
        .describe("Advance my read cursor over the returned messages (delivery hook sets this true)."),
      limit: z.number().int().min(1).max(200).default(20),
    })
    .strict()
    .describe("Read messages from other agents on this repo (the inbox). Default: my unread + broadcasts."),

  notify_resolve: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Repo channel. Default: cwd."),
      notification_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Notification ids to resolve."),
      thread_id: z.string().trim().min(1).max(128).optional().describe("Resolve every message in this thread."),
    })
    .strict()
    .refine((d) => d.notification_id.length > 0 || d.thread_id !== undefined, {
      message: "notify_resolve requires notification_id or thread_id.",
    })
    .describe("Mark messages resolved (close a message or a whole thread)."),

  notify_prune: z
    .object({
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Repo channel. Default: cwd."),
      notification_id: z
        .array(z.string().trim().min(1).max(128))
        .max(200)
        .default([])
        .describe("Notification ids to delete."),
      resolved: z.boolean().default(false).describe("Delete only messages already marked resolved."),
      older_than_days: z
        .number()
        .int()
        .min(1)
        .max(3650)
        .optional()
        .describe("Delete messages created more than N days ago."),
      dry_run: z.boolean().default(false).describe("Report matches without deleting."),
    })
    .strict()
    .refine((d) => d.notification_id.length > 0 || d.resolved || d.older_than_days !== undefined, {
      message: "notify_prune requires a selector: notification_id, resolved, or older_than_days.",
    })
    .describe("Delete notifications by id, resolved status, or age (retention for the repo channel)."),

  reflect: z
    .object({
      agent_id: agentId,
      task: nonEmptyText("What you did — the task being reflected on.", 2000),
      outcome: z.enum(["worked", "partial", "failed"]).describe("Did it work?"),
      worked: nonEmptyText("What worked.", 2000).optional(),
      didnt_work: nonEmptyText("What didn't work.", 2000).optional(),
      judgment_note: nonEmptyText("Advisory note naming checked evidence, remaining uncertainty, and why eval prompts mattered or did not.", 2000).optional(),
      lesson: nonEmptyText("Reusable lesson → recorded as a general memory (feeds recall + mine-weakness).", 4000).optional(),
      failure_signature: z
        .string()
        .trim()
        .min(1)
        .max(256)
        .optional()
        .describe("Clusterable recurring-failure signature for mine-weakness."),
      eval_failures: z
        .array(evalFailure)
        .max(20)
        .default([])
        .describe("Structured failed eval questions/checks preserved in the reflection memory; advisory evidence, not an auto-patch."),
      fix_repo: nonEmptyText("Indication to fix something in the repo/code → an open 'bad' refinement for the next agent.", 2000).optional(),
      fix_file: fileList,
      fix_harness: nonEmptyText("Improvement to this skill/harness itself → a 'harness'-tagged memory that export-harness surfaces.", 2000).optional(),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      importance: importanceScore.optional().describe("Override the outcome-derived importance (failed 8 / partial 6 / worked 5)."),
      duo: z
        .boolean()
        .optional()
        .describe("Request an advisory two-agent reflection packet. It guides semantic review and does not affect storage."),
    })
    .strict()
    .describe("Post-task self-reflection: record what worked/didn't as learning, plus actionable fix indications for the repo and/or the harness."),

  harness_apply: z
    .object({
      agent_id: agentId,
      approved_by: nonEmptyText("Human who approved this harness change (the gate).", 128),
      change: nonEmptyText("One-line summary of the skill/harness change.", 2000),
      file: fileList.describe("Skill files to be edited."),
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Workspace for the announcement notification."),
    })
    .strict()
    .describe(
      "Gated, branch-only, announced approval for an agent to edit the skill/harness itself. " +
        "Requires the human to have opened the gate (OCTOCODE_ALLOW_HARNESS_APPLY=1) and a dedicated branch.",
    ),

  memory_export: z
    .object({
      out: z.string().trim().min(1).max(1024).optional().describe("Output JSONL path. Default: <workspace>/.octocode/memories.jsonl."),
      workspace_path: z.string().trim().min(1).max(1024).optional(),
      min_importance: importanceScore.optional().describe("Only export memories at or above this importance."),
    })
    .strict()
    .describe("Export ACTIVE memories to a committable JSONL file so a team can share self-knowledge as files."),

  memory_import: z
    .object({
      file: nonEmptyText("JSONL file to import.", 1024),
      mode: z.enum(["skip", "replace"]).default("skip").describe("On memory_id collision: skip (keep local) or replace."),
    })
    .strict()
    .describe("Import memories from a JSONL file (team-shared self-knowledge). Dedupes by memory_id."),
};

export const examples = {
  tell_memory: {
    agent_id: "codex-local",
    task_context: "Refactoring auth router validation",
    observation:
      "The auth router normalizes tenant IDs before policy lookup; keep that order or cross-tenant tests fail.",
    importance_score: 8,
    label: "GOTCHA",
    tags: ["auth", "routing"],
    file: "src/auth/router.ts",
    file_tree_fingerprint: "git:HEAD",
    failure_signature: "mechanism:retry-loop|cause:test-timeout",
  },
  get_memory: {
    query: "What should I know before editing the auth router?",
    limit: 3,
    min_importance: 4,
    labels: ["GOTCHA"],
    tags: ["auth"],
    sort: "smart",
    smart: true,
  },
  pre_flight_intent: {
    agent_id: "codex-local",
    workspace: "/repo",
    plan_doc_ref: "docs/designs/active_plan.md",
    rationale: "Refactor auth router validation without breaking tenant policy order.",
    target_files: ["src/auth/router.ts", "src/auth/router.test.ts"],
    test_plan: "yarn test src/auth/router.test.ts",
    lock_type: "EXCLUSIVE",
  },
  wait_for_lock: {
    agent_id: "codex-local",
    target_files: ["src/auth/router.ts"],
    lock_type: "EXCLUSIVE",
    wait_seconds: 120,
    retry_interval: 5,
  },
  prune_stale_locks: {
    older_than_minutes: 20,
    expired_only: false,
    target_files: ["src/auth/router.ts"],
    dry_run: true,
  },
  release_file_lock: {
    agent_id: "codex-local",
    intent_id: "intent_abc123",
    status: "SUCCESS",
    verified: true,
    verified_note: "yarn test src/auth/router.test.ts: 273 passed",
  },
  verify: {
    agent_id: "codex-local",
    workspace: "/repo",
    intent_id: ["intent_abc123"],
    all_pending: false,
    message: "yarn test src/auth/router.test.ts: 273 passed",
  },
  forget_memory: {
    tags: ["auth"],
    max_importance: 3,
    dry_run: true,
  },
  refinement: {
    agent_id: "codex-local",
    workspace_path: "/Users/me/work/octocode-mcp",
    repo: "octocode-mcp",
    ref: "support-OQL",
    files: ["src/oql/planner.ts"],
    reasoning: "Next agent should finish the OQL pushdown work started here.",
    remember: "glob/size filters still materialize; only equality predicates push down to GitHub.",
    quality: "good",
    state: "ongoing",
  },
  refine_query: {
    repo: "octocode-mcp",
    ref: "support-OQL",
    states: ["open", "ongoing"],
  },
  refine_delete: {
    refinement_id: ["ref_abc123"],
    dry_run: true,
  },
  notify: {
    agent_id: "codex-2",
    repo: "octocode-mcp",
    ref: "support-OQL",
    kind: "blocker",
    subject: "Mid-refactor on oql/planner.ts — tests red",
    body: "Hold off editing src/oql/planner.ts until I push; pushdown rewrite in progress.",
    files: ["src/oql/planner.ts"],
    refs: ["intent_abc123"],
    importance: 8,
  },
  notify_query: {
    agent_id: "claude-1",
    repo: "octocode-mcp",
    unread_only: true,
    mark_read: true,
  },
  notify_resolve: {
    thread_id: "ntf_966efa90808a48648dea6cb858e8e0c6",
  },
  notify_prune: {
    resolved: true,
    older_than_days: 7,
    dry_run: true,
  },
  reflect: {
    agent_id: "codex-local",
    task: "Add equality pushdown to the OQL planner",
    outcome: "partial",
    worked: "Equality predicates now push down to ghSearchCode.",
    didnt_work: "Glob/size filters still force materialization — slower than hoped.",
    judgment_note: "Verified equality pushdown with planner tests; glob behavior remains intentionally unresolved.",
    lesson: "OQL pushdown only covers field equality today; document the residual-filter fallback.",
    fix_repo: "Teach planner.ts to push basename glob patterns down as path prefixes.",
    fix_file: ["src/oql/planner.ts"],
    eval_failures: [
      {
        id: "binary-glob-pushdown",
        dimension: "planning",
        failure_signature: "mechanism:oql-pushdown|cause:glob-materializes",
        suggested_lesson: "Keep residual-filter fallbacks visible when pushdown is incomplete.",
      },
    ],
    duo: true,
  },
  harness_apply: {
    agent_id: "codex-local",
    approved_by: "guy",
    change: "Add a reflect step to the agent loop in SKILL.md",
    file: ["SKILL.md"],
  },
  memory_export: {
    min_importance: 5,
  },
  memory_import: {
    file: ".octocode/memories.jsonl",
    mode: "skip",
  },
};

function usage() {
  return `Usage:
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

async function main(argv) {
  const [command, schemaName, file] = argv;

  if (!command || command === "--help" || command === "-h") {
    console.log(usage());
    return 0;
  }

  if (command === "list") {
    console.log(JSON.stringify(Object.keys(schemas), null, 2));
    return 0;
  }

  const schema = schemas[schemaName];
  if (!schema) {
    console.error(`Unknown schema: ${schemaName || "<missing>"}`);
    console.error(`Known schemas: ${Object.keys(schemas).join(", ")}`);
    return 2;
  }

  if (command === "json-schema") {
    console.log(JSON.stringify(toJsonSchema(schema), null, 2));
    return 0;
  }

  if (command === "example") {
    console.log(JSON.stringify(examples[schemaName], null, 2));
    return 0;
  }

  if (command === "validate") {
    if (!file) {
      console.error("Missing <json-file|->.");
      return 2;
    }
    const raw = file === "-" ? await readStdin() : await readFile(file, "utf8");
    const result = schema.safeParse(parseJson(raw));
    if (!result.success) {
      console.log(
        JSON.stringify(
          {
            ok: false,
            schema: schemaName,
            issues: formatZodError(result.error),
          },
          null,
          2,
        ),
      );
      return 1;
    }
    console.log(JSON.stringify({ ok: true, schema: schemaName, data: result.data }, null, 2));
    return 0;
  }

  console.error(`Unknown command: ${command}`);
  console.error(usage());
  return 2;
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
    console.error(error.message);
    process.exitCode = 1;
  },
);
