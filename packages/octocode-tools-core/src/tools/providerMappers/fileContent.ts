import type { FileContentResult as ProviderFileContentResult } from '../../providers/providerResults.js';
import type { z } from 'zod';
import type { WithOptionalMeta } from '../../types/execution.js';

import { FileContentQueryLocalSchema } from '@octocodeai/octocode-core/schema';

type LocalFileContentQuery = z.infer<typeof FileContentQueryLocalSchema> & {
  minify: import('@octocodeai/octocode-core/schema').MinifyMode;
};

export function mapFileContentToolQuery(query: LocalFileContentQuery) {
  const fullContent = Boolean(query.fullContent);

  return {
    projectId: `${query.owner}/${query.repo}`,
    path: String(query.path),
    ref: query.branch ? String(query.branch) : undefined,
    startLine: fullContent ? undefined : query.startLine,
    endLine: fullContent ? undefined : query.endLine,
    matchString:
      fullContent || !query.matchString ? undefined : String(query.matchString),
    contextLines: query.contextLines,
    contextBytes: query.contextBytes,
    fullContent,
    forceRefresh: Boolean((query as { forceRefresh?: boolean }).forceRefresh),
    chunkType: query.chunkType,
    offset: query.offset,
    limit: query.limit,
    minify: query.minify,
    matchStringIsRegex: query.matchStringIsRegex,
    matchStringCaseSensitive: query.matchStringCaseSensitive,
    goal: query.goal,
    reasoning: query.reasoning,
  };
}

export function mapFileContentProviderResult(
  data: ProviderFileContentResult,
  query: WithOptionalMeta<LocalFileContentQuery>
): Record<string, unknown> {
  return {
    path: data.path,
    content: data.content,
    ...(data.sourceLineRanges?.length
      ? { sourceLineRanges: data.sourceLineRanges }
      : {}),
    returnedChars: data.returnedChars,
    returnedBytes: data.returnedBytes,
    returnedLines: data.returnedLines,
    selectedMatchCount: data.selectedMatchCount,
    minifyFallback: data.minifyFallback,
    next: data.next,
    ...(typeof data.size === 'number' &&
      data.size > 0 && {
        fileSize: data.size,
      }),
    ...(typeof data.totalLines === 'number' && {
      totalLines: data.totalLines,
    }),
    ...(typeof data.sourceChars === 'number' && {
      sourceChars: data.sourceChars,
    }),
    ...(typeof data.sourceBytes === 'number' && {
      sourceBytes: data.sourceBytes,
    }),
    ...(data.contentView && {
      contentView: data.contentView,
    }),
    ...(data.isPartial && {
      isPartial: data.isPartial,
    }),
    ...(data.errorCode && { errorCode: data.errorCode }),
    ...(data.terminalLimit && { terminalLimit: data.terminalLimit }),
    ...(data.partialReasons?.length && { partialReasons: data.partialReasons }),
    ...(data.startLine && {
      startLine: data.startLine,
    }),
    ...(data.endLine && { endLine: data.endLine }),
    ...(data.matchRanges?.length && { matchRanges: data.matchRanges }),
    ...(data.matchedLines?.length && { matchedLines: data.matchedLines }),
    ...(data.lastModified && {
      lastModified: data.lastModified,
    }),
    ...(data.lastModifiedBy && {
      lastModifiedBy: data.lastModifiedBy,
    }),
    ...(data.pagination && {
      pagination: data.pagination,
    }),
    ...(data.warnings?.length && {
      warnings: data.warnings,
    }),
    ...(data.matchNotFound === true && {
      matchNotFound: true,
    }),
    ...(data.searchedFor && {
      searchedFor: data.searchedFor,
    }),
    ...(data.ref && query.branch !== data.ref
      ? { resolvedBranch: data.ref }
      : {}),
  };
}
