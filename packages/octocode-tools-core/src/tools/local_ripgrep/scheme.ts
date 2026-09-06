import { z } from 'zod';
import { RipgrepQuerySchema as CoreRipgrepQuerySchema } from '../../toolContract/input/resources/tools/localTextOperation.js';
import {
  LOCAL_MAX_DEPTH,
  MAX_MATCH_CONTENT_LENGTH,
  MAX_PAGE_NUMBER,
} from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const LOCAL_SEARCH_MODES = [
  'paginated',
  'discovery',
  'detailed',
  'structural',
] as const;

// sort ('relevance'|'matchCount'|'path'|'modified'|'accessed'|'created'),
// rankingProfile, and debugRanking are defined canonically in the local
// toolContract resource and flow through CoreRipgrepQuerySchema.
// Engine-incompatible sort values are translated
// to a deterministic filesystem walk in ripgrepExecutor; the relevance scorer
// runs in ripgrepResultBuilder. Keep tools-core overrides to tightening bounds
// only, not redefining ranking fields.
const REMOVED_CORE_FIELDS = ['semanticRanking'] as const;

const queryOverrides = {
  // This `mode` selects the SEARCH ALGORITHM (paginated/discovery/detailed/
  // structural). It's unrelated to the nested `content.patches.mode` on
  // ghGetHistoryItem pullRequest (diff detail level) — different concepts sharing this
  // field name across tools.
  mode: z.enum(LOCAL_SEARCH_MODES).optional().default('paginated'),
  // A single text/regex pattern; passing an array here fails validation.
  searchText: z.string().optional(),
  // The `output` enum's "files"/"filesWithout" shapes drop line content down to
  // matching / non-matching file paths, not tree-entry filtering.
  output: z
    .enum([
      'content',
      'files',
      'filesWithout',
      'countLines',
      'countMatches',
      'matchOnly',
    ])
    .optional()
    .default('content'),
  pattern: z.string().optional(),
  maxDepth: clampedInt(0, LOCAL_MAX_DEPTH).optional(),
  rule: z.string().optional(),
  captureText: z.boolean().optional(),
  contextLines: contextLinesField,
  // Description flows from the shared core contract resource; only the
  // bounds/default are tightened here.
  matchContentLength: clampedInt(1, MAX_MATCH_CONTENT_LENGTH)
    .optional()
    .default(500),
  maxMatchesPerFile: clampedInt(1, MAX_MATCH_CONTENT_LENGTH).optional(),
  maxFiles: clampedInt(1, MAX_MATCH_CONTENT_LENGTH).optional(),
  matchPage: relaxedPageNumberField.optional(),
  itemsPerPage: clampedInt(1, MAX_PAGE_NUMBER).optional(),
  page: relaxedPageNumberField.default(1),
  unique: z.enum(['off', 'list', 'count']).optional().default('off'),
} as const;

const bulkQueryOverrides = {
  ...queryOverrides,
  // Disabled feature: reject loudly instead of z.never()'s opaque type error —
  // the field is still visible in older clients/docs, so the rejection must
  // say WHY and what to do.
  semanticRanking: z
    .unknown()
    .optional()
    .superRefine((v, ctx) => {
      if (v === undefined) return;
      ctx.addIssue({
        code: 'custom',
        message:
          'semanticRanking is disabled in this build — sort:"relevance" already includes declaration/export/AST signals; remove this field.',
      });
    })
    .describe(
      'DISABLED in this build — do not pass. sort:"relevance" already includes declaration/export/AST signals.'
    ),
} as const;

const RipgrepQueryShape = createQueryShapeSchema(
  CoreRipgrepQuerySchema,
  bulkQueryOverrides
);

// Structural-mode validation (exactly one of pattern/rule, reject ripgrep-only
// fields, require keywords otherwise) is enforced by the core RipgrepQuerySchema
// superRefine, which describeQuerySchema preserves through to this schema.
const LocalRipgrepBaseQuerySchema = describeQuerySchema(
  CoreRipgrepQuerySchema,
  queryOverrides,
  { strict: true, omit: REMOVED_CORE_FIELDS }
);

// The mutually-exclusive boolean clusters were collapsed to enums in
// the local contract (regex/caseMode/multiline/output/unique), so those
// pairings are now impossible by construction. describeQuerySchema rebuilds the
// object from its shape and DROPS the core superRefine, so this local schema
// must re-assert the full cross-field contract itself (mirrors the core
// operation-level schema refinement). Enum fields carry defaults, so they are
// always defined here — a non-default value is an explicit agent choice.
export const LocalRipgrepQuerySchema = LocalRipgrepBaseQuerySchema.superRefine(
  (query, ctx) => {
    if (query.mode === 'structural') {
      if (query.pattern === undefined && query.rule === undefined) {
        ctx.addIssue({
          code: 'custom',
          message: 'mode:"structural" requires `pattern` or `rule`.',
          path: ['pattern'],
        });
      }
      if (query.pattern !== undefined && query.rule !== undefined) {
        ctx.addIssue({
          code: 'custom',
          message: '`pattern` and `rule` are mutually exclusive.',
          path: ['rule'],
        });
      }
      for (const field of ['pattern', 'rule'] as const) {
        if (query[field] !== undefined && query[field].trim().length === 0) {
          ctx.addIssue({
            code: 'custom',
            message: `Structural \`${field}\` must not be empty or whitespace.`,
            path: [field],
          });
        }
      }
      // Search knobs are meaningless on an AST query — reject non-default values.
      if (query.wholeWord) {
        ctx.addIssue({
          code: 'custom',
          message: '`wholeWord` is not valid with mode:"structural".',
          path: ['wholeWord'],
        });
      }
      if (query.invertMatch) {
        ctx.addIssue({
          code: 'custom',
          message: '`invertMatch` is not valid with mode:"structural".',
          path: ['invertMatch'],
        });
      }
      if (query.regex && query.regex !== 'smart') {
        ctx.addIssue({
          code: 'custom',
          message: '`regex` is not valid with mode:"structural".',
          path: ['regex'],
        });
      }
      if (query.caseMode && query.caseMode !== 'smart') {
        ctx.addIssue({
          code: 'custom',
          message: '`caseMode` is not valid with mode:"structural".',
          path: ['caseMode'],
        });
      }
      if (query.multiline && query.multiline !== 'off') {
        ctx.addIssue({
          code: 'custom',
          message: '`multiline` is not valid with mode:"structural".',
          path: ['multiline'],
        });
      }
      if (
        query.output &&
        query.output !== 'content' &&
        query.output !== 'countMatches' &&
        query.output !== 'files'
      ) {
        ctx.addIssue({
          code: 'custom',
          message:
            'With mode:"structural", `output` supports only "content" (default), "countMatches" (per-file counts, no match content — cheapest for counting), or "files" (paths only).',
          path: ['output'],
        });
      }
      if (query.unique && query.unique !== 'off') {
        ctx.addIssue({
          code: 'custom',
          message: '`unique` is not valid with mode:"structural".',
          path: ['unique'],
        });
      }
      // A two-dollar metavar is not metavar syntax ($X = one node, $$$X =
      // list): the matcher treats `$$NAME` as a literal identifier, so the
      // pattern silently matches nothing. Reject with the fix instead. PHP
      // ($$var variable-variables) and shell ($$ = PID) are exempt — there
      // `$$` can be genuine source syntax.
      const twoDollarMeta =
        query.pattern &&
        !['php', 'bash', 'sh', 'zsh'].includes(query.langType ?? '')
          ? /(?<!\$)\$\$(?!\$)[A-Z_][A-Z0-9_]*/.exec(query.pattern)
          : null;
      if (twoDollarMeta) {
        const name = twoDollarMeta[0].slice(2);
        ctx.addIssue({
          code: 'custom',
          message: `\`${twoDollarMeta[0]}\` is not a metavariable and matches nothing — use \`$${name}\` for one node or \`$$$${name}\` for a list of nodes.`,
          path: ['pattern'],
        });
      }
      // `langType` IS valid with mode:"structural": structuralSearch derives
      // include globs from it (deriveInclude → toStructuralSearchIncludeGlobs)
      // so `langType:'ts'` scopes AST search without hand-written globs.
      return;
    }
    if (query.pattern || query.rule) {
      ctx.addIssue({
        code: 'custom',
        message: '`pattern`/`rule` require mode:"structural".',
        path: [query.pattern ? 'pattern' : 'rule'],
      });
    }
    if (!query.searchText) {
      ctx.addIssue({
        code: 'custom',
        message: '`searchText` is required unless mode:"structural".',
        path: ['searchText'],
      });
    }
    if (query.matchWindow !== undefined && query.output !== 'matchOnly') {
      ctx.addIssue({
        code: 'custom',
        message: 'matchWindow requires output:"matchOnly".',
        path: ['matchWindow'],
      });
    }
    if (
      (query.unique === 'list' || query.unique === 'count') &&
      query.output !== 'matchOnly'
    ) {
      ctx.addIssue({
        code: 'custom',
        message: 'unique requires output:"matchOnly".',
        path: ['unique'],
      });
    }
  }
);

export type RipgrepQuery = z.infer<typeof LocalRipgrepQuerySchema>;

export const LocalRipgrepBulkQuerySchema = createRelaxedBulkQuerySchema(
  RipgrepQueryShape,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output TYPES — describes what local text search returns per query result row.
// No zod: the MCP server registers no outputSchema, so the output is a plain
// type. Shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

export interface LocalSearchMatch {
  line: number;
  endLine?: number;
  value?: string;
  column?: number;
  endColumn?: number;
  count?: number;
  /** AST node-kind label when classifyMatches ran (declaration|callsite|…). */
  kind?: string;
  /** Deterministic hint derived from kind (0.0..1.0); not a ranker score. */
  scoreHint?: number;
  metavars?: Record<string, string[]>;
  metavarRanges?: Record<
    string,
    Array<{
      text: string;
      line: number;
      column: number;
      endLine: number;
      endColumn: number;
    }>
  >;
}

export interface LocalSearchFile {
  path: string;
  absolutePath?: string;
  uri?: string;
  matches?: LocalSearchMatch[];
  totalOccurrences?: number;
  totalMatchedLines?: number;
  totalMatchRows?: number;
  returnedMatchRows?: number;
  ranking?: {
    score: number;
    profile?: string;
    pathRole?: string;
    reasons?: string[];
  };
  matchPagination?: LocalItemPagination;
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
}

export interface LocalSearchCodeData {
  files?: LocalSearchFile[];
  summary?: string;
  searchEngine?: string;
  stats?: {
    totalOccurrences?: number;
    matchedLines?: number;
    filesMatched?: number;
    filesSearched?: number;
    bytesSearched?: number;
    searchTime?: string;
    [key: string]: unknown;
  };
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
  terminalLimit?: boolean;
  truncated?: boolean;
  partialReasons?: Array<'maxFiles' | 'structuralLimit' | 'skippedFiles'>;
  diagnostics?: Array<{
    code: string;
    severity: string;
    stage: string;
    message: string;
    path?: string;
    recovery?: string;
  }>;
}

export type LocalSearchCodeOutput = BulkToolOutput<LocalSearchCodeData>;
