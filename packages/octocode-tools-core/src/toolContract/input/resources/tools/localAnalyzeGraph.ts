import { z } from 'zod';

import type { ToolSpec } from '../../types/index.js';
import {
  defineTool,
  MAX_LOCAL_ITEMS_PER_PAGE,
  MAX_PAGE_NUMBER,
  metaFields,
} from './_toolkit.js';

export const LOCAL_ANALYZE_GRAPH_TOOL_NAME = 'localAnalyzeGraph' as const;

export const localAnalyzeGraph: ToolSpec = defineTool({
  name: LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  type: 'Local',
  shortDescription:
    'Analyze a repository file graph through one bounded operation.',
  instructions:
    'Map file topology, not symbol identity. dependencies/dependents need file; path needs file+target; deadCode/reachability accept roots; cycles needs no selector. Import edges and dead-code results are candidates—verify changes and deletions with exact reads plus LSP.',
  schema: {
    path: 'Absolute repository or package root to scan.',
    operation:
      'One bounded operation: deadCode, cycles, dependencies, dependents, path, or reachability.',
    file: 'Repository-relative source file for dependencies, dependents, or path.',
    target: 'Repository-relative destination file for path.',
    depth: 'Traversal depth for dependencies or dependents.',
    entrypoints: 'Repository-relative roots for deadCode or reachability.',
    includeTests: 'Treat test files as reachability roots.',
    excludeDir: 'Directory names to prune from graph construction.',
    maxFiles: 'Maximum source files scanned; truncation is reported.',
    limit: 'Result cap applied before pagination.',
    page: 'Result page, 1-based; advance only while pagination.hasMore.',
    pageSize: 'Results returned per page.',
  },
});

const clampedInt = (min: number, max: number) =>
  z.preprocess(
    value =>
      typeof value === 'number' && Number.isFinite(value)
        ? Math.min(Math.max(value, min), max)
        : value,
    z.number().int().min(min).max(max)
  );

const operation = <T extends string>(value: T, description: string) =>
  z.literal(value).describe(description);

const commonFields = {
  ...metaFields,
  path: z.string().describe('Absolute repository or package root to scan.'),
  excludeDir: z
    .array(z.string())
    .optional()
    .describe('Directory names to prune from graph construction.'),
  maxFiles: clampedInt(1, 50_000)
    .optional()
    .describe('Maximum source files scanned; truncation is reported.'),
  limit: clampedInt(1, 5_000)
    .optional()
    .describe('Result cap applied before pagination.'),
  page: clampedInt(1, MAX_PAGE_NUMBER)
    .optional()
    .default(1)
    .describe('Result page, 1-based.'),
  pageSize: clampedInt(1, MAX_LOCAL_ITEMS_PER_PAGE)
    .optional()
    .describe('Results returned per page.'),
} as const;

const entrypointFields = {
  entrypoints: z
    .array(z.string())
    .optional()
    .describe(
      'Repo-relative reachability roots; omit to detect package.json main/exports/bin.'
    ),
  includeTests: z
    .boolean()
    .optional()
    .default(true)
    .describe('Treat tests as reachability roots (default true).'),
} as const;

const traversalFields = {
  file: z.string().describe('Repo-relative source file.'),
  depth: clampedInt(1, 50)
    .optional()
    .default(1)
    .describe('Maximum traversal depth (default 1).'),
} as const;

export const LocalAnalyzeGraphQuerySchema = z.discriminatedUnion('operation', [
  z
    .object({
      ...commonFields,
      ...entrypointFields,
      operation: operation(
        'deadCode',
        'Find unreachable or unretained exported symbols and dead SCCs.'
      ),
    })
    .strict(),
  z
    .object({
      ...commonFields,
      operation: operation(
        'cycles',
        'Find strongly connected file components.'
      ),
    })
    .strict(),
  z
    .object({
      ...commonFields,
      ...traversalFields,
      operation: operation(
        'dependencies',
        'Traverse files imported or re-exported by the source file; results include edgeKinds and syntactic confidence.'
      ),
    })
    .strict(),
  z
    .object({
      ...commonFields,
      ...traversalFields,
      operation: operation(
        'dependents',
        'Traverse files that import or re-export the source file; results include edgeKinds and syntactic confidence.'
      ),
    })
    .strict(),
  z
    .object({
      ...commonFields,
      file: z.string().describe('Repo-relative source file.'),
      target: z.string().describe('Repo-relative destination file.'),
      operation: operation(
        'path',
        'Find the shortest directed import/re-export path with per-edge provenance.'
      ),
    })
    .strict(),
  z
    .object({
      ...commonFields,
      ...entrypointFields,
      operation: operation(
        'reachability',
        'Classify scanned files by reachability from entrypoints.'
      ),
    })
    .strict(),
]);
