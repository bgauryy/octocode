export function attachLspEvidence<T>(
  result: T,
  opts: {
    kind: 'calls' | 'references';
    paginationKey: 'pagination' | 'outputPagination';
  }
): T {
  const status = (result as { status?: string }).status;
  if (status !== undefined && status !== 'empty') return result;

  const hasResults = status === undefined;
  const pagination = (
    result as Record<string, { hasMore?: boolean } | undefined>
  )[opts.paginationKey];
  const paginationHasMore = pagination?.hasMore ?? false;
  const reasons: string[] = [];

  if (!hasResults) {
    reasons.push(
      opts.kind === 'references'
        ? 'No references were resolved for the supplied symbol and line hint.'
        : 'No calls were resolved for the supplied symbol and line hint.'
    );
  }
  if (paginationHasMore) {
    reasons.push(
      opts.paginationKey === 'pagination'
        ? 'LSP result pagination has more results.'
        : 'LSP output pagination has more data.'
    );
  }

  const evidence = {
    kind: opts.kind,
    answerReady: hasResults,
    complete: hasResults && !paginationHasMore,
    confidence: 'high' as const,
    ...(reasons.length > 0 ? { reason: reasons.join(' ') } : {}),
  };

  (result as Record<string, unknown>).evidence = evidence;
  return result;
}
