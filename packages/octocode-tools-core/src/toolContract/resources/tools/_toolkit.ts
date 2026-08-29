import { z } from 'zod';

import { baseSchema, baseSchemaDescriptions } from '../global.js';
import type { ToolSchema, ToolSpec, ToolType } from '../../types.js';

// ---------------------------------------------------------------------------
// Numeric bounds & defaults live HERE, in the Zod schema — never in prose.
// They reach the agent through the generated JSON Schema (minimum / maximum /
// default keywords), so restating them in a field description would only
// duplicate the contract and let the two drift apart.
// ---------------------------------------------------------------------------
export const MAX_CONTEXT_LINES = 100;
export const MAX_PAGE_NUMBER = 1_000;
export const MAX_CHAR_OFFSET = 100_000_000;
export const MAX_CHAR_LENGTH = 50_000;
export const MAX_LINE_NUMBER = 1_000_000_000;
export const MAX_ORDER_HINT = 100_000;
export const MAX_LOCAL_DEPTH = 20;
export const MAX_FIND_DEPTH = 100;
export const MAX_LOCAL_LIMIT = 10_000;
export const MAX_LOCAL_ITEMS_PER_PAGE = 50;
export const MAX_GITHUB_SEARCH_LIMIT = 100;
export const DEFAULT_GITHUB_SEARCH_LIMIT = 30;
export const MAX_GITHUB_STRUCTURE_ITEMS_PER_PAGE = 200;
export const DEFAULT_GITHUB_STRUCTURE_ITEMS_PER_PAGE = 100;
export const MAX_PR_ITEMS_PER_PAGE = 100;
export const DEFAULT_PR_ITEMS_PER_PAGE = 20;
export const MAX_MATCH_CONTENT_LENGTH = 100_000;
export const DEFAULT_MATCH_CONTENT_LENGTH = 500;

// Count caps for localSearchCode result-limiting knobs (matches, files) and its
// files-per-page — kept distinct from the char-length / page-number constants
// they previously borrowed so the bound reads true to what it limits.
export const MAX_MATCH_COUNT = 100_000;
export const MAX_SEARCH_ITEMS_PER_PAGE = 1_000;

// lspGetSemantics call-graph depth and page size (own names, not borrowed from
// the local-depth / PR constants).
export const MAX_LSP_DEPTH = 20;
export const MAX_LSP_ITEMS_PER_PAGE = 100;

// ghViewRepoStructure recursion depth (a GitHub tree, not a local tree).
export const MAX_GITHUB_STRUCTURE_DEPTH = 20;

// localBinaryInspect bounds (were inline literals).
export const MAX_BINARY_ENTRIES = 10_000;
export const DEFAULT_BINARY_ENTRIES = 1_000;
export const MAX_BINARY_ENTRIES_PER_PAGE = 100;
export const MAX_BINARY_CONTEXT_LINES = 50;
export const DEFAULT_BINARY_CONTEXT_LINES = 3;
export const MAX_STRINGS_MIN_LENGTH = 128;
export const DEFAULT_STRINGS_MIN_LENGTH = 8;

/**
 * Authoring shape for a tool: the three human-written fields plus its schema.
 * `description` is NOT authored — `defineTool` composes it from the others.
 */
export interface ToolDefinition {
  readonly name: string;
  readonly type: ToolType;
  readonly shortDescription: string;
  readonly instructions: string;
  readonly schema: ToolSchema;
}

/**
 * Build a {@link ToolSpec}, deriving the client-facing `description` as
 * `"{type} | {shortDescription} | {instructions}"`. Clients that only read
 * `description` (e.g. the MCP tool registry) still get the full text, while the
 * three parts stay individually addressable and can't drift out of sync.
 */
export function defineTool(def: ToolDefinition): ToolSpec {
  return {
    ...def,
    description: `${def.type} | ${def.shortDescription} | ${def.instructions}`,
  };
}

export function intRange(min: number, max: number): z.ZodNumber {
  return z.number().int().min(min).max(max);
}

export const StringArray = z.array(z.string()).optional();

export function pageNumber(): z.ZodDefault<z.ZodNumber> {
  return intRange(1, MAX_PAGE_NUMBER).default(1);
}

export function optionalPageNumber(): z.ZodOptional<z.ZodNumber> {
  return intRange(1, MAX_PAGE_NUMBER).optional();
}

export function contextLines(): z.ZodOptional<z.ZodNumber> {
  return intRange(0, MAX_CONTEXT_LINES).optional();
}

export function lineNumber(): z.ZodOptional<z.ZodNumber> {
  return intRange(1, MAX_LINE_NUMBER).optional();
}

export function charOffset(): z.ZodOptional<z.ZodNumber> {
  return intRange(0, MAX_CHAR_OFFSET).optional();
}

export function charLength(): z.ZodOptional<z.ZodNumber> {
  return intRange(1, MAX_CHAR_LENGTH).optional();
}

// Meta fields shared by every query. Their shape AND descriptions both come
// from the global Zod `baseSchema` — this re-export keeps a single source of
// truth. They are intentionally absent from each tool's per-field prose map
// (default.json carries them once under baseSchema).
export const metaFields = baseSchema.shape;

const META_KEYS = new Set(Object.keys(metaFields));

/**
 * Attach the per-field prose from a tool's `schema` map onto its Zod shape,
 * producing the validated query object. This is the single merge point between
 * a tool's content (the prose map) and its scheme (the Zod shape): both live in
 * the same tool file, and this guarantees they stay in lock-step.
 *
 * - `prefix` builds dotted paths (e.g. "content.patches.mode") for nested
 *   objects, matching the flat keys default.json ships.
 * - Meta fields are described from `baseSchema`; a field with no entry in the
 *   prose map is left undescribed (it simply won't appear in default.json).
 */
/**
 * Fallback prose for fields shared across many tools (page, itemsPerPage,
 * owner, repo, …). Per-tool prose in each ToolSpec's `schema` map always
 * wins; this only fills fields that would otherwise ship UNDESCRIBED — an
 * undescribed field forces the agent to guess semantics (is `page` 0- or
 * 1-based? what unit is `charLength`?). Enforced by
 * `field-descriptions.test.ts`: no query field may reach the served schema
 * blank.
 */
const SHARED_FIELD_PROSE: Readonly<Record<string, string>> = {
  owner: 'Repository owner (user or org).',
  repo: 'Repository name, without the owner.',
  branch: 'Branch, tag, or commit SHA; omit for the default branch.',
  path: 'Target path. GitHub tools: repo-relative, exact case, no leading slash. Local tools: absolute path.',
  page: 'Result page, 1-based. Advance only while pagination.hasMore.',
  itemsPerPage: 'Items per result page.',
  limit: 'Maximum results to return.',
  contextLines: 'Extra source lines around each match/anchor.',
  charLength:
    'Characters per content window; continue via the returned next offset when partial.',
  charOffset:
    "Content window start offset — copy from the previous response's pagination, do not compute. Partial responses report the file's full size (totalLines/sourceChars) and isPartial: a slice is not the whole file — never conclude a key/field/value is absent from it; continue here first.",
};

export function buildObject<S extends z.ZodRawShape>(
  prose: ToolSchema,
  shape: S,
  prefix = ''
): z.ZodObject<S> {
  const describedShape = {} as { [K in keyof S]: S[K] };

  for (const key of Object.keys(shape) as Array<keyof S & string>) {
    const fieldSchema = shape[key] as unknown as z.ZodTypeAny;
    const path = prefix ? `${prefix}.${key}` : key;
    const isMeta = prefix === '' && META_KEYS.has(key);
    const description = isMeta
      ? baseSchemaDescriptions[path as keyof typeof baseSchemaDescriptions]
      : (prose[path] ??
        (fieldSchema.description ? undefined : SHARED_FIELD_PROSE[key]));

    describedShape[key] = (description
      ? fieldSchema.describe(description)
      : fieldSchema) as unknown as S[typeof key];
  }

  // Unknown keys are stripped, never rejected: a removed/renamed/legacy field
  // (or a typo) must not hard-fail the whole call with a schema mismatch.
  return z.object(describedShape);
}

/** Shared refinement: fullContent / matchString / startLine+endLine are
 * mutually exclusive extraction modes; startLine and endLine must be supplied
 * together; and endLine must be >= startLine. Used by both the local and
 * GitHub file-content readers. */
export function validateFileContentExtractionMode(
  data: {
    fullContent?: boolean | undefined;
    matchString?: string | undefined;
    startLine?: number | undefined;
    endLine?: number | undefined;
  },
  ctx: z.RefinementCtx
): void {
  const hasFullContent = data.fullContent === true;
  const hasMatchString = data.matchString !== undefined;
  const hasLineRange =
    data.startLine !== undefined || data.endLine !== undefined;

  if (hasFullContent && hasMatchString) {
    ctx.addIssue({
      code: 'custom',
      message:
        'Cannot use fullContent with matchString — these are mutually exclusive extraction methods.',
      path: ['matchString'],
    });
  }
  if (hasFullContent && hasLineRange) {
    ctx.addIssue({
      code: 'custom',
      message:
        'Cannot use fullContent with startLine/endLine — these are mutually exclusive extraction methods.',
      path: ['startLine'],
    });
  }
  if (hasMatchString && hasLineRange) {
    ctx.addIssue({
      code: 'custom',
      message:
        'Cannot use matchString with startLine/endLine — these are mutually exclusive extraction methods.',
      path: ['startLine'],
    });
  }
  if ((data.startLine === undefined) !== (data.endLine === undefined)) {
    ctx.addIssue({
      code: 'custom',
      message:
        'startLine and endLine must be provided together — a line range needs both bounds.',
      path: [data.startLine === undefined ? 'startLine' : 'endLine'],
    });
  }
  if (
    data.startLine !== undefined &&
    data.endLine !== undefined &&
    data.endLine < data.startLine
  ) {
    ctx.addIssue({
      code: 'custom',
      message:
        'Invalid line range: endLine must be greater than or equal to startLine.',
      path: ['endLine'],
    });
  }
}
