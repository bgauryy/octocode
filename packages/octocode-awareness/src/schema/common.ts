/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { z } from 'zod';
export const agentId = z.string().min(1).max(128).describe("Agent id.");
export const nonEmptyText = (description: string, max = 4000) => z.string().trim().min(1).max(max).describe(description);
export const tag = z
  .string()
  .trim()
  .min(1)
  .max(64)
  .regex(/^[a-zA-Z0-9_.:-]+$/, "tags may contain letters, numbers, underscore, dot, colon, or dash");
export const tags = z.array(tag).max(32).default([]).describe("Filter tags.");
export const workspacePath = z.string().trim().min(1).max(1024).describe("Workspace root.");
export const artifactScope = z.string().trim().min(1).max(256).describe("Artifact/package scope.");
export const repoScope = z.string().trim().min(1).max(256).describe("Repo slug.");
export const refScope = z.string().trim().min(1).max(256).describe("Branch/tag/SHA.");
export const references = z
  .array(z.string().trim().min(1).max(512))
  .max(20)
  .default([])
  .describe("Provenance refs, e.g. file:/abs/path, pr:owner/repo#1, URL.");
export const MEMORY_LABELS = [
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
] as const;
export const normalizeMemoryLabel = (value: unknown) => {
  if (typeof value !== "string") return value;
  const cleaned = value.trim().toUpperCase().replace(/[\s-]+/g, "_");
  return cleaned || "OTHER";
};
export const memoryLabel = z
  .preprocess(normalizeMemoryLabel, z.enum(MEMORY_LABELS).default("OTHER"))
  .describe("Memory label.");
export const memorySort = z
  .enum(["smart", "score", "importance", "recent", "accessed"])
  .default("smart")
  .describe("Sort order.");
export const importanceLevel = z
  .number()
  .int()
  .min(1)
  .max(10)
  .describe("1-10 importance.");
export const targetFiles = z
  .array(z.string().trim().min(1).max(1024))
  .min(1)
  .max(200)
  .describe("Files to lock.");
export const awarenessQueryView = z
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
export const awarenessOutputFormat = z
  .enum(["json", "table", "csv", "markdown", "html"])
  .default("json")
  .describe("Output format.");
export const repoContextMode = z
  .enum(["local", "share"])
  .default("local")
  .describe("Whether generated .octocode is intended as local-only or shared repo context. The command never edits .gitignore.");

// Signals — repo-scoped agent-to-agent messages. The `kind` enum is the
// "smart" part: typed messages let recipients filter (e.g. only blockers) and
// act, instead of parsing free prose.
export const NOTIFICATION_KINDS = [
  "claim", // "I'm taking these files / this area"
  "handoff", // "finished X, you can start Y" (pair with a refinement id in refs)
  "question", // ask another agent something
  "reply", // answer within a thread
  "blocker", // "don't touch X — mid-change / broken"
  "request", // "can you run Y / verify Z"
  "decision", // "chose approach Z" — broadcast a call others should know
  "fyi", // low-stakes heads-up
] as const;
export const notificationKind = z.enum(NOTIFICATION_KINDS);
export const fileList = z
  .array(z.string().trim().min(1).max(1024))
  .max(200)
  .default([])
  .describe("Related files.");
export const refIds = z
  .array(z.string().trim().min(1).max(128))
  .max(50)
  .default([])
  .describe("Related ids.");
export const evalFailure = z
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
