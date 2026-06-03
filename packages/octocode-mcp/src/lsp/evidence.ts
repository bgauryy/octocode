/**
 * Shared LSP evidence attacher.
 *
 * `lsp_call_hierarchy` and `lsp_find_references` both annotate their results
 * with a `kind` / `answerReady` / `complete` / `confidence` block so the bulk
 * runner can lift it to the response envelope. The shapes diverge in two
 * details only:
 *   - the `kind` discriminator (e.g. 'calls' vs 'references')
 *   - the pagination key name ('outputPagination' vs 'pagination')
 *
 * Results are always derived from a real language server (there is no text
 * fallback), so confidence is always `high`.
 */
export function attachLspEvidence<T>(
  result: T,
  opts: {
    kind: 'calls' | 'references';
    paginationKey: 'pagination' | 'outputPagination';
  }
): T {
  // Only annotate well-shaped LSP results. Raw error envelopes
  // (`{ isError, message }`) lack `status` and are returned as-is. The
  // lean contract: ABSENT status ≡ success; only 'empty' / 'error' emit.
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

  // Mutate in place so any non-enumerable raw-chars symbol attached upstream
  // (see attachRawResponseChars) survives.
  (result as Record<string, unknown>).evidence = evidence;
  return result;
}
