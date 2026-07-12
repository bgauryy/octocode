/**
 * `params` lowering for `octocode search` — target-specific parameter bags.
 *
 * Every active target (semantics, repositories, packages, pullRequests,
 * commits, diff, research/graph) accepts its own flat `params`
 * shape; this maps the shared shorthand flags onto each target's fields.
 */
import type { SearchShorthand } from './types.js';
import { clean } from './utils.js';

export function targetParams(parts: SearchShorthand): Record<string, unknown> {
  switch (parts.target) {
    case 'code':
      return clean({
        concise: parts.concise,
        extension: parts.extension,
        filename: parts.filename,
      });
    case 'semantics':
      return clean({
        type: parts.op ?? 'documentSymbols',
        uri: parts.uri,
        symbolName: parts.symbol,
        symbolKind: parts.symbolKind,
        lineHint: parts.line,
        orderHint: parts.order,
        depth: parts.depth,
        workspaceRoot: parts.workspaceRoot,
        format: parts.format,
      });
    case 'repositories':
      return clean({
        keywords: parts.text
          ? parts.text.includes(' ')
            ? parts.text.split(/\s+/).filter(Boolean)
            : [parts.text]
          : undefined,
        topicsToSearch: parts.topic?.length ? parts.topic : undefined,
        language: parts.lang,
        owner: parts.owner,
        stars: parts.stars,
        forks: parts.forks,
        goodFirstIssues: parts.goodFirstIssues,
        license: parts.license,
        created: parts.created,
        updated: parts.updated,
        size: parts.size,
        match: parts.match,
        archived: parts.archived,
        visibility: parts.visibility,
        concise: parts.concise,
        sort: parts.sort,
        limit: parts.limit,
        page: parts.page,
      });
    case 'packages':
      return clean({
        packageName: parts.text,
        page: parts.page,
      });
    case 'pullRequests':
      return clean({
        keywordsToSearch: parts.text ? [parts.text] : undefined,
        query: undefined,
        concise: parts.concise,
        state: parts.state,
        author: parts.author,
        label: parts.label,
        prNumber: parts.prNumber,
        head: parts.head,
        base: parts.base,
        created: parts.created,
        updated: parts.updated,
        closed: parts.closed,
        'merged-at': parts.mergedAt,
        draft: parts.draft,
        archived: parts.archived,
        sort: parts.sort,
        order: parts.orderDirection,
        reviewMode: parts.reviewMode,
        filePage: parts.filePage,
        commentPage: parts.commentPage,
        commitPage: parts.commitPage,
        charOffset: parts.charOffset,
        charLength: parts.charLength,
        content: prContent(parts),
        limit: parts.limit,
        page: parts.page,
        matchString: parts.matchString,
        matchScope: parts.matchString ? 'all' : undefined,
      });
    case 'commits':
      return clean({
        path: parts.corpus.kind === 'github' ? parts.corpus.path : undefined,
        branch: parts.branch,
        since: parts.since,
        until: parts.until,
        author: parts.author,
        includeDiff: parts.patches,
        limit: parts.limit,
        page: parts.page,
        filePage: parts.filePage,
        itemsPerPage: parts.itemsPerPage,
      });
    case 'diff': {
      const localTwoFileDiff =
        parts.corpus.kind === 'local' && parts.diffPath !== undefined;
      return clean({
        prNumber: parts.prNumber,
        baseRef: parts.baseRef ?? (localTwoFileDiff ? 'base' : undefined),
        headRef: parts.headRef ?? (localTwoFileDiff ? 'head' : undefined),
        path:
          parts.diffPath ??
          (parts.corpus.kind === 'github' ? parts.corpus.path : undefined),
      });
    }
    case 'research':
    case 'graph':
      return clean({
        goal: parts.text,
        intent: parts.intent,
        facets: parts.facets,
        proof: parts.proof,
        proofLimit: parts.proofLimit,
        includePackets: parts.includePackets,
        includeFacts: parts.includeFacts,
        includeEdges: parts.includeEdges,
        maxFiles: parts.maxFiles,
      });
    default:
      return {};
  }
}

function prContent(
  parts: SearchShorthand
): Record<string, unknown> | undefined {
  if (
    !parts.patches &&
    !parts.patchFile &&
    !parts.commentsContent &&
    !parts.commitsContent &&
    !parts.deep
  )
    return undefined;
  return clean({
    metadata: true,
    body: parts.deep ? true : undefined,
    changedFiles:
      parts.deep || parts.patches || parts.patchFile ? true : undefined,
    patches: parts.patchFile
      ? { mode: 'selected' as const, files: [parts.patchFile] }
      : parts.deep || parts.patches
        ? { mode: 'all' as const }
        : undefined,
    comments:
      parts.deep || parts.commentsContent
        ? { discussion: true, reviewInline: true }
        : undefined,
    reviews: parts.deep ? true : undefined,
    commits: parts.deep || parts.commitsContent ? { list: true } : undefined,
  });
}
