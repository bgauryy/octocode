import { z } from 'zod';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import { MAX_CHAR_LENGTH } from '../../config.js';

export const HistoryQueryShape = z.object({
  id: z.string().optional(),
  mainResearchGoal: z.string().optional(),
  researchGoal: z.string().optional(),
  reasoning: z.string().optional(),

  type: z
    .enum(['file', 'repo'])
    .describe(
      '"file": history of one specific file — path is required. ' +
        '"repo": commit activity for a repo or directory — path is an optional ' +
        'directory prefix (e.g. "src/auth/") to scope results; omit for whole-repo activity.'
    ),

  owner: z.string().describe('Repository owner or organization.'),

  repo: z.string().describe('Repository name.'),

  path: z
    .string()
    .optional()
    .describe(
      'type:"file" — repo-relative file path, required (e.g. "src/auth/session.ts"). ' +
        'type:"repo" — optional directory prefix to scope history to a subtree ' +
        '(e.g. "src/auth/"). Omit for whole-repo activity.'
    ),

  branch: z
    .string()
    .optional()
    .describe(
      'Branch, tag, or SHA to start from. Defaults to the default branch.'
    ),

  author: z
    .string()
    .optional()
    .describe('Filter by GitHub username or email address.'),

  since: z
    .string()
    .optional()
    .describe('ISO 8601 timestamp — only commits after this date.'),

  until: z
    .string()
    .optional()
    .describe('ISO 8601 timestamp — only commits before this date.'),

  page: relaxedPageNumberField,

  perPage: clampedInt(1, 100)
    .optional()
    .default(30)
    .describe('Commits per page. Maximum 100.'),

  includeDiff: z
    .boolean()
    .optional()
    .default(false)
    .describe(
      'Fetch file-level diffs (patch) per commit. ' +
        'Costs one extra API call per commit — use sparingly on large pages.'
    ),

  charOffset: clampedInt(0, 100_000_000).optional(),

  charLength: clampedInt(1, MAX_CHAR_LENGTH)
    .optional()
    .describe(
      `Max chars for output. Truncates patch strings. Maximum ${MAX_CHAR_LENGTH}.`
    ),
});

export type HistoryQueryInput = z.input<typeof HistoryQueryShape>;
export type HistoryQuery = z.infer<typeof HistoryQueryShape>;

// Display schema (single query) — used by MCP tool registration
export const HistoryQueryLocalSchema = HistoryQueryShape;

// Bulk schema — up to 5 parallel queries
export const HistoryBulkQuerySchema = createRelaxedBulkQuerySchema(
  HistoryQueryShape,
  { maxQueries: 5 }
);
