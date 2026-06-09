import { z } from 'zod';
import { GitHubCodeSearchQuerySchema as UpstreamGitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  createRelaxedBulkQuerySchema,
  DEFAULT_PAGE_SIZE,
  describeField,
  optionalMetaFields,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import {
  EvidenceSchema,
  responseEnvelopeFields,
} from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

export const GitHubCodeSearchQueryLocalSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
  UpstreamGitHubCodeSearchQuerySchema.omit({ limit: true }).extend({
    ...optionalMetaFields,
    keywordsToSearch: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.keywordsToSearch,
      'Search terms AND-combined by GitHub. Each array element is a separate required term — do NOT put multi-word phrases in one element (split them: ["foo","bar"] not ["foo bar"]). Use a small set of distinctive identifiers; scope-only searches are usually low-signal.'
    ),
    owner: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.owner,
      'Optional GitHub owner/org scope. Pair with repo to search one repository.'
    ),
    repo: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.repo,
      'Optional repository scope. Use with owner to avoid broad global searches.'
    ),
    extension: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.extension,
      'Optional extension filter. Pass without a dot for clarity, e.g. "ts"; a leading dot is tolerated. Caution: combining extension or filename with multiple keywordsToSearch uses AND logic and often zeros out results — add these filters only after a keyword-only search confirms hits.'
    ),
    filename: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.filename,
      'Optional filename filter (GitHub `filename:` qualifier). Matches files whose name equals or contains this value — e.g. "Button.tsx" or "jest.config". Use this to target a specific file. Combine with `path` to restrict to a directory.'
    ),
    path: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.path,
      'Optional directory prefix filter (GitHub `path:` qualifier). Matches any file whose full repo path starts with this prefix — it is NOT a file path. Use `filename` to target one file; combine both to narrow to a directory. Passing a file-like path (e.g. "src/agent.ts") auto-extracts the filename part — but explicit `filename` is clearer and more reliable.'
    ),
    match: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.match,
      'Where to search: "file" (default) searches file contents; "path" searches the file path/name only — useful for finding files by name pattern without any keyword. When omitted, GitHub searches content.'
    ),
    page: z
      .number()
      .int()
      .min(1)
      .max(1000)
      .optional()
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} matches. Use page=2, page=3, … to walk through results.`
      ),
  })
);

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    GitHubCodeSearchQueryLocalSchema
  );

export const GitHubCodeSearchOutputLocalSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  evidence: EvidenceSchema,
  responsePagination: responseEnvelopeFields.responsePagination,
  results: z.array(
    z.object({
      id: z.string(),
      queryId: z.string().optional(),
      owner: z.string(),
      repo: z.string(),
      matches: z.array(
        z.object({
          path: z.string().describe('Repo-relative file path of the match.'),
          value: z
            .string()
            .optional()
            .describe(
              'Code snippet returned by GitHub for this match. NOT the full file — use githubGetFileContent to read the full file.'
            ),
          pathOnly: z
            .boolean()
            .optional()
            .describe(
              'True when GitHub returned a path match but no text snippet. Use githubGetFileContent with matchString to inspect content.'
            ),
          matchIndices: z
            .array(
              z
                .object({ start: z.number(), end: z.number() })
                .describe(
                  'Character offsets within the `value` snippet string (not line numbers in the file).'
                )
            )
            .optional()
            .describe(
              'Character-offset spans inside the `value` snippet that highlight the matched terms. These are NOT line numbers — use githubGetFileContent with matchString to get exact line positions.'
            ),
        })
      ),
    })
  ),
  pagination: z
    .object({
      currentPage: z.number(),
      totalPages: z.number(),
      perPage: z.number(),
      totalMatches: z.number(),
      hasMore: z.boolean(),
    })
    .optional(),
  hints: z.array(z.string()).optional(),
  emptyQueries: z
    .array(
      z.object({
        id: z.string(),
        hints: z.array(z.string()).optional(),
        nonExistentScope: z.literal(true).optional(),
      })
    )
    .optional(),
  errors: z
    .array(
      z.object({
        id: z.string(),
        error: z.string(),
        hints: z.array(z.string()).optional(),
      })
    )
    .optional(),
});

export type GitHubCodeSearchOutputLocal = z.infer<
  typeof GitHubCodeSearchOutputLocalSchema
>;
