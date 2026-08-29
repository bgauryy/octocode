import { z } from 'zod';

import type { ToolNames } from '../types.js';

// ---------------------------------------------------------------------------
// Stable, agent-facing tool names. Referenced everywhere a tool is addressed
// by key (system prompt, generated config). Renaming a value here is a
// breaking change to the public toolset.
// ---------------------------------------------------------------------------
export const toolNames: ToolNames = {
  GITHUB_SEARCH_CODE: 'ghSearchCode',
  GITHUB_FETCH_CONTENT: 'ghGetFileContent',
  GITHUB_VIEW_REPO_STRUCTURE: 'ghViewRepoStructure',
  GITHUB_SEARCH_REPOSITORIES: 'ghSearchRepos',
  GITHUB_PULL_REQUESTS: 'ghSearchPullRequests',
  GITHUB_ISSUES: 'ghSearchIssues',
  GITHUB_COMMITS: 'ghSearchCommits',
  GITHUB_RELEASES: 'ghListReleases',
  GITHUB_DISCUSSIONS: 'ghSearchDiscussions',
  PACKAGE_SEARCH: 'npmSearch',
  GITHUB_CLONE_REPO: 'ghCloneRepo',
  LOCAL_RIPGREP: 'localSearchCode',
  LOCAL_VIEW_STRUCTURE: 'localViewStructure',
  LOCAL_FIND_FILES: 'localFindFiles',
  LOCAL_ANALYZE_GRAPH: 'localAnalyzeGraph',
  LOCAL_FETCH_CONTENT: 'localGetFileContent',
  LSP_GET_SEMANTIC_CONTENT: 'lspGetSemantics',
} as const;

// ---------------------------------------------------------------------------
// Meta fields carried by EVERY tool query, defined once as a Zod schema. This
// is the single source of truth for both their shape (optional strings) and
// their agent-facing prose:
//   - `_toolkit` spreads `baseSchema.shape` into each tool's query schema.
//   - `generateDefaultJson` / `dumpAgentContext` serialize the derived
//     `baseSchemaDescriptions` string map (so default.json stays a flat map).
// Keep `.describe()` outermost on each field so the description reads back off
// the optional wrapper via `.description`.
// ---------------------------------------------------------------------------
export const baseSchema = z.object({
  id: z
    .string()
    .optional()
    .describe('Stable ID to correlate this query with its result in a batch.'),
  mainResearchGoal: z
    .string()
    .optional()
    .describe('Goal shared by every query in this batch; set once per call.'),
  researchGoal: z
    .string()
    .optional()
    .describe('The specific sub-question this query answers.'),
  reasoning: z
    .string()
    .optional()
    .describe('Why this query advances the goal.'),
});

export type BaseSchemaShape = typeof baseSchema.shape;

/**
 * Flat `field -> description` map derived from the Zod `baseSchema`. Consumed by
 * `generateDefaultJson` (serialized verbatim under the `baseSchema` JSON key)
 * and by `buildObject`, which merges these onto each tool's meta fields. Keeps
 * the shipped JSON a plain string map while the source of truth stays Zod.
 */
export const baseSchemaDescriptions = Object.fromEntries(
  Object.entries(baseSchema.shape).map(([key, field]) => [
    key,
    field.description ?? '',
  ])
) as Record<keyof BaseSchemaShape, string>;
