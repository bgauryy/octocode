import * as path from 'path';
import { safeReadFile } from '../../lsp/validation.js';
import picomatch from 'picomatch';

import type {
  FindReferencesResult,
  ReferenceLocation,
  LSPRange,
  ExactPosition,
} from '../../lsp/types.js';
import type { z } from 'zod';
import type { LSPFindReferencesQuerySchema } from '@octocodeai/octocode-core/schemas';

type LSPFindReferencesQuery = z.infer<typeof LSPFindReferencesQuerySchema>;
import type { WithOptionalMeta } from '../../types/execution.js';
import { acquirePooledClient } from '../../lsp/manager.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAME } from './constants.js';
import { LSP_ERROR_CODES } from '../../lsp/lspErrorCodes.js';
import {
  buildFindReferencesPageOutOfRangeResult,
  buildFindReferencesPageResult,
} from './referenceResultHelpers.js';

export function matchesFilePatterns(
  relativePath: string,
  includePattern?: string[],
  excludePattern?: string[]
): boolean {
  if (excludePattern?.length) {
    const isExcluded = picomatch(excludePattern);
    if (isExcluded(relativePath)) return false;
  }
  if (includePattern?.length) {
    const isIncluded = picomatch(includePattern);
    return isIncluded(relativePath);
  }
  return true;
}

function buildReferencesCapabilityError(): FindReferencesResult {
  return {
    status: 'error',
    error: 'Language server does not support find references',
    errorType: 'unknown',
    errorCode: LSP_ERROR_CODES.LSP_CAPABILITY_UNSUPPORTED,
    hints: [
      ...getHints(TOOL_NAME, 'error'),
      'The active language server does not advertise referencesProvider.',
      'Try localSearchCode for text-based usage search.',
    ],
  };
}

function buildNoPatternMatchResult(
  query: WithOptionalMeta<LSPFindReferencesQuery>,
  totalUnfiltered: number
): FindReferencesResult {
  return {
    status: 'empty',
    hints: [
      ...getHints(TOOL_NAME, 'empty'),
      `Found ${totalUnfiltered} reference(s) but none matched the file patterns`,
      query.includePattern?.length
        ? `Include patterns: ${query.includePattern.join(', ')}`
        : '',
      query.excludePattern?.length
        ? `Exclude patterns: ${query.excludePattern.join(', ')}`
        : '',
      'Try broader patterns or remove filtering to see all results',
    ].filter(Boolean),
  };
}

export async function findReferencesWithLSP(
  filePath: string,
  workspaceRoot: string,
  position: ExactPosition,
  query: WithOptionalMeta<LSPFindReferencesQuery>
): Promise<FindReferencesResult | null> {
  const client = await acquirePooledClient(workspaceRoot, filePath);
  if (!client) return null;

  if (client.hasCapability && !client.hasCapability('referencesProvider')) {
    return buildReferencesCapabilityError();
  }

  try {
    await client.prepareCallHierarchy(filePath, position);
  } catch {
    void 0;
  }

  const includeDeclaration = query.includeDeclaration ?? true;
  const locations = await client.findReferences(
    filePath,
    position,
    includeDeclaration
  );

  if (!locations || locations.length === 0) {
    return {
      status: 'empty',
      hints: [
        ...getHints(TOOL_NAME, 'empty'),
        'Language server found no references',
        'Symbol may be unused or only referenced dynamically',
        'Try localSearchCode for text-based search as fallback',
      ],
    };
  }

  let rawLocations: RawReferenceLocation[] = locations.map(loc => {
    const relativeUri = path.relative(workspaceRoot, loc.uri);
    const isDefinition =
      loc.uri === filePath &&
      loc.range.start.line === position.line &&
      loc.range.start.character === position.character;

    return {
      uri: relativeUri || loc.uri,
      absoluteUri: loc.uri,
      range: loc.range,
      content: loc.content,
      isDefinition,
    };
  });

  if (!includeDeclaration) {
    rawLocations = rawLocations.filter(loc => !loc.isDefinition);
  }

  const totalUnfiltered = rawLocations.length;

  const hasFilters = Boolean(
    query.includePattern?.length || query.excludePattern?.length
  );
  const filteredLocations = hasFilters
    ? rawLocations.filter(loc =>
        matchesFilePatterns(loc.uri, query.includePattern, query.excludePattern)
      )
    : rawLocations;

  if (filteredLocations.length === 0) {
    return buildNoPatternMatchResult(query, totalUnfiltered);
  }

  const referencesPerPage = query.referencesPerPage ?? 20;
  const page = query.page ?? 1;
  const totalReferences = filteredLocations.length;
  const totalPages = Math.ceil(totalReferences / referencesPerPage);

  if (totalReferences > 0 && page > totalPages) {
    return buildFindReferencesPageOutOfRangeResult(
      filteredLocations,
      page,
      totalPages,
      totalReferences,
      referencesPerPage
    );
  }

  const startIndex = (page - 1) * referencesPerPage;
  const endIndex = Math.min(startIndex + referencesPerPage, totalReferences);
  const paginatedRaw = filteredLocations.slice(startIndex, endIndex);

  const contextLines = query.contextLines ?? 2;
  const paginatedReferences = await Promise.all(
    paginatedRaw.map(raw => enhanceReferenceLocation(raw, contextLines))
  );

  return buildFindReferencesPageResult({
    locations: paginatedReferences,
    filteredReferences: filteredLocations,
    page,
    totalPages,
    totalReferences,
    referencesPerPage,
    hasFilters,
    totalUnfiltered,
  });
}

interface RawReferenceLocation {
  uri: string;
  absoluteUri: string;
  range: LSPRange;
  content: string;
  isDefinition: boolean;
}

async function enhanceReferenceLocation(
  raw: RawReferenceLocation,
  contextLines: number
): Promise<ReferenceLocation> {
  let content = raw.content;

  if (contextLines > 0) {
    try {
      const fileContent = await safeReadFile(raw.absoluteUri);
      if (!fileContent) throw new Error('Cannot read file');
      const lines = fileContent.split(/\r?\n/);
      const startLine = Math.max(0, raw.range.start.line - contextLines);
      const endLine = Math.min(
        lines.length - 1,
        raw.range.end.line + contextLines
      );

      const snippetLines = lines.slice(startLine, endLine + 1);
      content = snippetLines
        .map((line, i) => {
          const lineNum = startLine + i + 1;
          const isTarget = lineNum === raw.range.start.line + 1;
          const marker = isTarget ? '>' : ' ';
          return `${marker}${String(lineNum).padStart(4, ' ')}| ${line}`;
        })
        .join('\n');
    } catch {
      void 0;
    }
  }

  return {
    uri: raw.absoluteUri,
    range: raw.range,
    content,
    ...(raw.isDefinition ? { isDefinition: true as const } : {}),
  };
}
