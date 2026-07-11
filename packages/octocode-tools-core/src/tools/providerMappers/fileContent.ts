import type { FileContentResult as ProviderFileContentResult } from '../../providers/types.js';
import type { z } from 'zod';
import type { WithOptionalMeta } from '../../types/execution.js';

import { FileContentQueryLocalSchema } from '../github_fetch_content/scheme.js';

type LocalFileContentQuery = z.infer<typeof FileContentQueryLocalSchema> & {
  minify: import('../../scheme/fields.js').MinifyMode;
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
    contextLines: (query as { contextLines?: number }).contextLines ?? 5,
    fullContent,
    forceRefresh: Boolean((query as { forceRefresh?: boolean }).forceRefresh),
    charOffset: query.charOffset,
    charLength: query.charLength,
    minify: query.minify,
    matchStringIsRegex: query.matchStringIsRegex,
    matchStringCaseSensitive: query.matchStringCaseSensitive,
    mainResearchGoal: query.mainResearchGoal,
    researchGoal: query.researchGoal,
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
    ...(data.startLine && {
      startLine: data.startLine,
    }),
    ...(data.endLine && { endLine: data.endLine }),
    ...(data.matchRanges?.length && { matchRanges: data.matchRanges }),
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
