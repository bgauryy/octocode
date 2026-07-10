/**
 * Canonical pagination schemas for all direct tools.
 *
 * Single source of truth — every tool output schema composes from these.
 * No compatibility aliases: each concept has exactly one name.
 */
import { z } from 'zod';

// ---------------------------------------------------------------------------
// Item pagination — for search results, directory listings, archive entries
// ---------------------------------------------------------------------------

export const ItemPaginationSchema = z.object({
  currentPage: z.number(),
  totalPages: z.number(),
  hasMore: z.boolean(),
  nextPage: z.number().optional(),
  pageSize: z.number().optional(),
  totalItems: z.number().optional(),
});

export type ItemPagination = z.infer<typeof ItemPaginationSchema>;

// ---------------------------------------------------------------------------
// Char pagination — for file content windows, PR bodies, archive extraction
// ---------------------------------------------------------------------------

export const CharPaginationSchema = z.object({
  charOffset: z.number(),
  charLength: z.number(),
  totalChars: z.number(),
  hasMore: z.boolean(),
  nextCharOffset: z.number().optional(),
  currentPage: z.number().optional(),
  totalPages: z.number().optional(),
  chunkMode: z.enum(['semantic', 'char-limit']).optional(),
});

export type CharPagination = z.infer<typeof CharPaginationSchema>;

// ---------------------------------------------------------------------------
// Continuation — machine-ready next-call descriptor
// ---------------------------------------------------------------------------

export const ToolContinuationSchema = z.object({
  tool: z.string(),
  query: z.record(z.string(), z.unknown()),
  why: z.string().optional(),
  confidence: z.enum(['exact', 'heuristic']).optional(),
});

export type ToolContinuation = z.infer<typeof ToolContinuationSchema>;

/** Runtime item-pagination fields used by local tools (aliases of pageSize/totalItems). */
export const LocalItemPaginationSchema = ItemPaginationSchema.extend({
  filesPerPage: z.number().optional(),
  entriesPerPage: z.number().optional(),
  matchesPerPage: z.number().optional(),
  totalFiles: z.number().optional(),
  totalEntries: z.number().optional(),
  totalMatches: z.number().optional(),
  totalFilesFound: z.number().optional(),
  nextMatchPage: z.number().optional(),
});

export type LocalItemPagination = z.infer<typeof LocalItemPaginationSchema>;

/**
 * Build a machine-ready next-page continuation for list-style local tools.
 * Callers pass the full original query with the advanced page field already set.
 */
export function buildNextPageContinuation(
  tool: string,
  query: Record<string, unknown>,
  why = 'Continue to the next page of results.'
): ToolContinuation {
  return {
    tool,
    query,
    why,
    confidence: 'exact',
  };
}

// ---------------------------------------------------------------------------
// Diagnostic — structured tool-level diagnostic message
// ---------------------------------------------------------------------------

export const ToolDiagnosticSchema = z.object({
  level: z.enum(['info', 'warning', 'error']),
  message: z.string(),
  field: z.string().optional(),
  code: z.string().optional(),
});

export type ToolDiagnostic = z.infer<typeof ToolDiagnosticSchema>;
